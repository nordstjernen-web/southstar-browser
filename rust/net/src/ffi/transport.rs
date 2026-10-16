//! Southstar — the C ABI of the transport layer: network init and teardown, the CA bundle and TLS groups the Rust HTTP client uses, cancellation, per-origin slots and dead-origin tracking.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
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
static RNG_WARMUP: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

unsafe extern "C" {
    fn RAND_bytes(buf: *mut u8, num: c_int) -> c_int;
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_aborting() -> GBoolean {
    glib::boolean(ABORTING.load(Ordering::SeqCst))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_begin_abort() {
    ABORTING.store(true, Ordering::SeqCst);
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

pub fn init() {
    resolve_ca_bundle();
    southstar_http::init();
    CA_BUNDLE.with_ptr(|path| southstar_http::preload_trust_roots(path.map(|p| p.as_bytes())));
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

pub fn ca_bundle_path() -> Option<Vec<u8>> {
    CA_BUNDLE.get()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_ec_curves() -> *const c_char {
    c"X25519MLKEM768:X25519:P-256:P-384".as_ptr()
}
