//! Southstar — the C ABI of the transport layer: network init and teardown, the CA bundle and TLS groups the Rust HTTP client uses, cancellation, per-origin slots and dead-origin tracking.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::JoinHandle;

use southstar_glib::{self as glib, GBoolean};

use super::sys;
use crate::storage::{self, Slot};
use crate::transport;

static ABORTING: AtomicBool = AtomicBool::new(false);
static CA_BUNDLE: Slot = Slot::new();
static ACCEPT_ENCODING: Slot = Slot::new();
static POST_QUANTUM: AtomicBool = AtomicBool::new(false);
static RNG_WARMUP: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

unsafe extern "C" {
    fn OpenSSL_version(kind: c_int) -> *const c_char;
    fn g_cancellable_is_cancelled(cancellable: *mut c_void) -> GBoolean;
    fn RAND_bytes(buf: *mut u8, num: c_int) -> c_int;
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

fn slot_ptr(slot: &Slot) -> *const c_char {
    slot.with_ptr(|value| value.map_or(ptr::null(), |v| v.as_ptr()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_aborting() -> GBoolean {
    glib::boolean(ABORTING.load(Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_begin_abort() {
    ABORTING.store(true, Ordering::SeqCst);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_host_recently_dead(origin: *const c_char) -> GBoolean {
    glib::boolean(text(origin).is_some_and(transport::host_recently_dead))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_host_mark_dead(origin: *const c_char) {
    if let Some(origin) = text(origin) {
        transport::mark_host_dead(origin);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_host_mark_alive(origin: *const c_char) {
    if let Some(origin) = text(origin) {
        transport::mark_host_alive(origin);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_acquire_origin_slot(
    origin: *const c_char,
    cancellable: *mut c_void,
) -> GBoolean {
    let cancelled =
        || !cancellable.is_null() && unsafe { g_cancellable_is_cancelled(cancellable) } != 0;
    glib::boolean(text(origin).is_some_and(|o| transport::acquire_origin_slot(o, cancelled)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_release_origin_slot(origin: *const c_char) {
    if let Some(origin) = text(origin) {
        transport::release_origin_slot(origin);
    }
}

fn resolve_ca_bundle() {
    if CA_BUNDLE.get().is_some() {
        return;
    }
    let try_path = |path: &[u8]| {
        let found = !path.is_empty() && sys::exists(path);
        if found {
            CA_BUNDLE.set(Some(path.to_vec()));
        }
        found
    };
    let env = std::env::var_os("CURL_CA_BUNDLE").or_else(|| std::env::var_os("SSL_CERT_FILE"));
    if env
        .as_ref()
        .and_then(|e| e.to_str())
        .is_some_and(|e| try_path(e.as_bytes()))
    {
        return;
    }
    if transport::ca_bundle_candidates()
        .iter()
        .any(|path| try_path(path))
    {
        return;
    }
    if cfg!(windows) {
        sys::info(c"ns_net: no CA bundle file found; trusting the Windows root certificate store. If HTTPS fails, install mingw-w64-x86_64-ca-certificates or set CURL_CA_BUNDLE.");
    }
}

fn warm_rng() {
    let thread = std::thread::Builder::new()
        .name("nd-rng-warmup".into())
        .spawn(|| {
            let mut buf = [0u8; 32];
            unsafe { RAND_bytes(buf.as_mut_ptr(), buf.len() as c_int) };
        });
    *lock(&RNG_WARMUP) = thread.ok();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_join_rng() {
    let thread = lock(&RNG_WARMUP).take();
    if let Some(thread) = thread {
        let _ = thread.join();
    }
}

fn openssl_version() -> Vec<u8> {
    let text = text(unsafe { OpenSSL_version(0) }).unwrap_or_default();
    let mut words = text.split(|&c| c == b' ');
    let name = words.next().unwrap_or_default();
    let version = words.next().unwrap_or_default();
    [name, b"/", version].concat()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_init() {
    resolve_ca_bundle();
    southstar_http::init();
    if transport::post_quantum_curves(&openssl_version()) {
        POST_QUANTUM.store(true, Ordering::SeqCst);
    }
    ACCEPT_ENCODING.set(Some(southstar_http::accept_encoding().to_vec()));
    warm_rng();
    storage::hsts_path();
    storage::cookie_dir();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_transport_shutdown() {
    ACCEPT_ENCODING.set(None);
    super::storage::ns_net_state_shutdown();
    CA_BUNDLE.set(None);
    transport::shutdown_origin_slots();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_accept_encoding() -> *const c_char {
    slot_ptr(&ACCEPT_ENCODING)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_ca_bundle_path() -> *const c_char {
    slot_ptr(&CA_BUNDLE)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_ec_curves() -> *const c_char {
    if POST_QUANTUM.load(Ordering::SeqCst) {
        c"X25519MLKEM768:X25519:P-256:P-384".as_ptr()
    } else {
        c"X25519:P-256:P-384".as_ptr()
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_response_budget() -> u64 {
    transport::response_budget()
}
