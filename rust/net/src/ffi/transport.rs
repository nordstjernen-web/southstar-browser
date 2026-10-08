//! Southstar — the C ABI of the transport layer: init and teardown of curl, the multi-handle thread every easy handle runs on, the shared DNS, TLS-session and connection caches, TLS options, cancellation, per-origin slots, dead-origin tracking and header-list logging.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::UnsafeCell;
use core::ffi::{c_char, c_int, c_long, c_void};
use core::ptr;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use southstar_glib::{self as glib, GBoolean};

use super::curl::{self, Code};
use super::sys;
use crate::storage::{self, Slot};
use crate::transport;

#[repr(C)]
struct GMutexStorage([usize; 2]);

struct ShareLocks([UnsafeCell<GMutexStorage>; curl::LOCK_DATA_SLOTS]);

unsafe impl Sync for ShareLocks {}

static SHARE_LOCKS: ShareLocks =
    ShareLocks([const { UnsafeCell::new(GMutexStorage([0; 2])) }; curl::LOCK_DATA_SLOTS]);
static SHARE: AtomicUsize = AtomicUsize::new(0);
static ABORTING: AtomicBool = AtomicBool::new(false);
static CA_BUNDLE: Slot = Slot::new();
static ACCEPT_ENCODING: Slot = Slot::new();
static POST_QUANTUM: AtomicBool = AtomicBool::new(false);
static RNG_WARMUP: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

unsafe extern "C" {
    fn g_mutex_lock(mutex: *mut GMutexStorage);
    fn g_mutex_unlock(mutex: *mut GMutexStorage);
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

struct Transfer {
    easy: usize,
    result: Option<Code>,
}

struct Multi {
    handle: usize,
    thread: Option<JoinHandle<()>>,
    quit: bool,
    next_id: u64,
    incoming: VecDeque<u64>,
    active: BTreeMap<usize, u64>,
    transfers: BTreeMap<u64, Transfer>,
}

static MULTI: Mutex<Multi> = Mutex::new(Multi {
    handle: 0,
    thread: None,
    quit: false,
    next_id: 0,
    incoming: VecDeque::new(),
    active: BTreeMap::new(),
    transfers: BTreeMap::new(),
});
static TRANSFER_DONE: Condvar = Condvar::new();

fn finish(m: &mut Multi, id: u64, result: Code) {
    if let Some(t) = m.transfers.get_mut(&id) {
        t.result = Some(result);
    }
    TRANSFER_DONE.notify_all();
}

fn multi_loop(handle: usize) {
    let multi = handle as *mut c_void;
    loop {
        {
            let mut m = lock(&MULTI);
            if m.quit {
                let active: Vec<(usize, u64)> =
                    core::mem::take(&mut m.active).into_iter().collect();
                for (easy, id) in active {
                    unsafe { curl::curl_multi_remove_handle(multi, easy as *mut c_void) };
                    finish(&mut m, id, curl::E_ABORTED_BY_CALLBACK);
                }
                while let Some(id) = m.incoming.pop_front() {
                    finish(&mut m, id, curl::E_ABORTED_BY_CALLBACK);
                }
                return;
            }
            while let Some(id) = m.incoming.pop_front() {
                let easy = m.transfers.get(&id).map_or(0, |t| t.easy);
                if unsafe { curl::curl_multi_add_handle(multi, easy as *mut c_void) } == curl::M_OK
                {
                    m.active.insert(easy, id);
                } else {
                    finish(&mut m, id, curl::E_FAILED_INIT);
                }
            }
        }
        let mut running = 0;
        unsafe { curl::curl_multi_perform(multi, &mut running) };
        let mut queued = 0;
        loop {
            let msg = unsafe { curl::curl_multi_info_read(multi, &mut queued) };
            let Some(msg) = (unsafe { msg.as_ref() }) else {
                break;
            };
            if msg.msg != curl::MSG_DONE {
                continue;
            }
            let (easy, result) = (msg.easy_handle, msg.result);
            unsafe { curl::curl_multi_remove_handle(multi, easy) };
            let mut m = lock(&MULTI);
            if let Some(id) = m.active.remove(&(easy as usize)) {
                finish(&mut m, id, result);
            }
        }
        let mut timeout: c_long = -1;
        unsafe { curl::curl_multi_timeout(multi, &mut timeout) };
        let wait_ms = if !(0..=1000).contains(&timeout) {
            1000
        } else {
            timeout as c_int
        };
        unsafe { curl::curl_multi_poll(multi, ptr::null_mut(), 0, wait_ms, ptr::null_mut()) };
    }
}

fn multi_start() -> usize {
    let mut m = lock(&MULTI);
    if m.thread.is_none() {
        let handle = unsafe { curl::curl_multi_init() } as usize;
        m.handle = handle;
        if handle != 0 {
            unsafe {
                curl::curl_multi_setopt(
                    handle as *mut c_void,
                    curl::MOPT_PIPELINING,
                    curl::PIPE_MULTIPLEX,
                )
            };
            m.thread = std::thread::Builder::new()
                .name("ns-net-multi".into())
                .spawn(move || multi_loop(handle))
                .ok();
        }
    }
    m.handle
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_multi_perform(
    easy: *mut c_void,
    _cancellable: *mut c_void,
) -> Code {
    let handle = multi_start();
    if handle == 0 {
        return unsafe { curl::curl_easy_perform(easy) };
    }
    let mut m = lock(&MULTI);
    if m.quit {
        drop(m);
        return unsafe { curl::curl_easy_perform(easy) };
    }
    let id = m.next_id;
    m.next_id += 1;
    m.transfers.insert(
        id,
        Transfer {
            easy: easy as usize,
            result: None,
        },
    );
    m.incoming.push_back(id);
    unsafe { curl::curl_multi_wakeup(handle as *mut c_void) };
    loop {
        if let Some(result) = m.transfers.get(&id).and_then(|t| t.result) {
            m.transfers.remove(&id);
            return result;
        }
        m = TRANSFER_DONE
            .wait_timeout(m, Duration::from_millis(250))
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_multi_shutdown() {
    let thread = {
        let mut m = lock(&MULTI);
        let thread = m.thread.take();
        if thread.is_some() {
            m.quit = true;
            unsafe { curl::curl_multi_wakeup(m.handle as *mut c_void) };
        }
        thread
    };
    if let Some(thread) = thread {
        let _ = thread.join();
    }
    let mut m = lock(&MULTI);
    if m.handle != 0 {
        unsafe { curl::curl_multi_cleanup(m.handle as *mut c_void) };
        m.handle = 0;
    }
    m.active.clear();
    m.quit = false;
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_http_version() -> c_long {
    static VERSION: std::sync::OnceLock<c_long> = std::sync::OnceLock::new();
    *VERSION.get_or_init(|| {
        let http3 = curl::version().is_some_and(|v| v.features & curl::VERSION_HTTP3 != 0);
        if http3 && std::env::var_os("NS_FORCE_HTTP3").is_some() {
            curl::HTTP_VERSION_3
        } else {
            curl::HTTP_VERSION_2TLS
        }
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_xferinfo_cb(
    clientp: *mut c_void,
    _dltotal: i64,
    _dlnow: i64,
    _ultotal: i64,
    _ulnow: i64,
) -> c_int {
    if ABORTING.load(Ordering::SeqCst) {
        return 1;
    }
    c_int::from(!clientp.is_null() && unsafe { g_cancellable_is_cancelled(clientp) } != 0)
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

unsafe extern "C" fn share_lock(
    _handle: *mut c_void,
    data: c_int,
    _access: c_int,
    _user: *mut c_void,
) {
    if let Some(slot) = usize::try_from(data)
        .ok()
        .and_then(|d| SHARE_LOCKS.0.get(d))
    {
        unsafe { g_mutex_lock(slot.get()) };
    }
}

unsafe extern "C" fn share_unlock(_handle: *mut c_void, data: c_int, _user: *mut c_void) {
    if let Some(slot) = usize::try_from(data)
        .ok()
        .and_then(|d| SHARE_LOCKS.0.get(d))
    {
        unsafe { g_mutex_unlock(slot.get()) };
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
        sys::info(c"ns_net: no CA bundle file found; relying on CURLSSLOPT_NATIVE_CA via the Windows certificate store. If HTTPS fails, install mingw-w64-x86_64-ca-certificates or set CURL_CA_BUNDLE.");
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

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_init() {
    resolve_ca_bundle();
    unsafe { curl::curl_global_init(curl::GLOBAL_DEFAULT) };
    let version = curl::version();
    if version
        .as_ref()
        .is_some_and(|v| transport::post_quantum_curves(&v.ssl_version))
    {
        POST_QUANTUM.store(true, Ordering::SeqCst);
    }
    let features = version.as_ref().map_or(0, |v| v.features);
    ACCEPT_ENCODING.set(Some(transport::accept_encoding(
        features & curl::VERSION_LIBZ != 0,
        features & curl::VERSION_BROTLI != 0,
        features & curl::VERSION_ZSTD != 0,
    )));
    let share = unsafe { curl::curl_share_init() };
    if !share.is_null() {
        for data in [
            curl::LOCK_DATA_DNS,
            curl::LOCK_DATA_SSL_SESSION,
            curl::LOCK_DATA_CONNECT,
            curl::LOCK_DATA_PSL,
            curl::LOCK_DATA_HSTS,
        ] {
            unsafe { curl::curl_share_setopt(share, curl::SHOPT_SHARE, data) };
        }
        unsafe {
            curl::curl_share_setopt(share, curl::SHOPT_LOCKFUNC, share_lock as curl::LockFn);
            curl::curl_share_setopt(
                share,
                curl::SHOPT_UNLOCKFUNC,
                share_unlock as curl::UnlockFn,
            );
        }
    }
    SHARE.store(share as usize, Ordering::SeqCst);
    warm_rng();
    storage::hsts_path();
    storage::altsvc_path();
    storage::cookie_dir();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_transport_shutdown() {
    let share = SHARE.swap(0, Ordering::SeqCst);
    if share != 0 {
        unsafe { curl::curl_share_cleanup(share as *mut c_void) };
    }
    unsafe { curl::curl_global_cleanup() };
    ACCEPT_ENCODING.set(None);
    super::storage::ns_net_state_shutdown();
    CA_BUNDLE.set(None);
    transport::shutdown_origin_slots();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_share() -> *mut c_void {
    SHARE.load(Ordering::SeqCst) as *mut c_void
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
pub unsafe extern "C" fn ns_net_apply_curl_tls(curl_handle: *mut c_void) {
    let set_str = |option: c_int, value: *const c_char| unsafe {
        curl::curl_easy_setopt(curl_handle, option, value)
    };
    unsafe {
        curl::curl_easy_setopt(curl_handle, curl::OPT_SSL_VERIFYPEER, 1 as c_long);
        curl::curl_easy_setopt(curl_handle, curl::OPT_SSL_VERIFYHOST, 2 as c_long);
    }
    set_str(
        curl::OPT_SSL_CIPHER_LIST,
        c"ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:ECDHE-RSA-AES128-SHA:ECDHE-RSA-AES256-SHA:AES128-GCM-SHA256:AES256-GCM-SHA384:AES128-SHA:AES256-SHA".as_ptr(),
    );
    set_str(
        curl::OPT_TLS13_CIPHERS,
        c"TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256".as_ptr(),
    );
    set_str(curl::OPT_SSL_EC_CURVES, ns_net_ec_curves());
    let ech = curl::version().is_some_and(|v| {
        v.version_num >= curl::ECH_MIN_VERSION
            && v.feature_names
                .iter()
                .any(|f| f.eq_ignore_ascii_case(b"ECH"))
    });
    if ech {
        set_str(curl::OPT_ECH, c"true".as_ptr());
    }
    let bundle = ns_net_ca_bundle_path();
    if !bundle.is_null() {
        set_str(curl::OPT_CAINFO, bundle);
    }
    if cfg!(windows) {
        unsafe {
            curl::curl_easy_setopt(curl_handle, curl::OPT_SSL_OPTIONS, curl::SSLOPT_NATIVE_CA)
        };
    }
    if let Some(cfg) = southstar_config::get() {
        if text(cfg.doh_url).is_some_and(|d| d.starts_with(b"https://")) {
            set_str(curl::OPT_DOH_URL, cfg.doh_url);
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_slist_serialize(list: *const curl::Slist) -> *mut c_char {
    if list.is_null() {
        return ptr::null_mut();
    }
    let mut lines = Vec::new();
    let mut node = list;
    while let Some(n) = unsafe { node.as_ref() } {
        if let Some(line) = text(n.data) {
            lines.push(line);
        }
        node = n.next;
    }
    glib::strdup(&transport::serialize_headers(lines.into_iter()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_response_budget() -> u64 {
    transport::response_budget()
}
