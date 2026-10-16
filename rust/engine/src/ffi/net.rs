//! Southstar — blocking requests on a nested main loop, the response view, stylesheet bytes and their cache, resource timings and preloads.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, Ordering};

use southstar_glib::{self as glib, GBoolean, GError, GHashTable, GPtrArray};
pub use southstar_net::{NsResponse, Response};

use crate::fetch::CssFetch;

#[repr(C)]
struct ResourceTiming {
    top_url: *mut c_char,
    url: *mut c_char,
    initiator: *const c_char,
    start_us: i64,
    end_us: i64,
    resp: *mut NsResponse,
    render_blocking: GBoolean,
    in_frame: GBoolean,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<ResourceTiming>() == 56);

#[repr(C)]
pub struct GBytes {
    _private: [u8; 0],
}

#[repr(C)]
struct GMainLoop {
    _private: [u8; 0],
}

pub type AsyncReady =
    unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, user_data: *mut c_void);

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dest {
    Default = 0,
    Script = 1,
    Style = 2,
    Image = 3,
}

unsafe extern "C" {
    fn g_main_loop_new(context: *mut c_void, is_running: GBoolean) -> *mut GMainLoop;
    fn g_main_loop_run(main_loop: *mut GMainLoop);
    fn g_main_loop_quit(main_loop: *mut GMainLoop);
    fn g_main_loop_unref(main_loop: *mut GMainLoop);
    fn g_get_monotonic_time() -> i64;
    fn g_bytes_new(data: *const c_void, size: usize) -> *mut GBytes;
    fn g_bytes_ref(bytes: *mut GBytes) -> *mut GBytes;
    fn g_bytes_unref(bytes: *mut GBytes);
    fn g_bytes_get_data(bytes: *mut GBytes, size: *mut usize) -> *const c_void;
    fn g_ptr_array_new_with_free_func(free: glib::GDestroyNotify) -> *mut GPtrArray;
}

static BLOCKING_DEPTH: AtomicI32 = AtomicI32::new(0);

pub fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

fn ptr_or_null(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

pub fn in_blocking_fetch() -> bool {
    BLOCKING_DEPTH.load(Ordering::Relaxed) > 0
}

pub struct NestedLoop(NonNull<GMainLoop>);

impl NestedLoop {
    pub fn new() -> NestedLoop {
        NestedLoop(
            NonNull::new(unsafe { g_main_loop_new(ptr::null_mut(), glib::FALSE) })
                .expect("g_main_loop_new"),
        )
    }

    pub fn raw(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    pub fn run_blocking(&self) {
        BLOCKING_DEPTH.fetch_add(1, Ordering::Relaxed);
        unsafe { g_main_loop_run(self.0.as_ptr()) };
        BLOCKING_DEPTH.fetch_sub(1, Ordering::Relaxed);
    }
}

impl Drop for NestedLoop {
    fn drop(&mut self) {
        unsafe { g_main_loop_unref(self.0.as_ptr()) };
    }
}

pub unsafe fn quit_loop(main_loop: *mut c_void) {
    unsafe { g_main_loop_quit(main_loop.cast()) };
}

pub fn nosniff(resp: &Response) -> bool {
    resp.x_content_type_options()
        .is_some_and(|value| southstar_net::header_is_nosniff(value.to_bytes()))
}

pub fn accept_headers(dest: Dest) -> *const *const c_char {
    southstar_net::accept_headers_for(dest as c_int)
}

struct FetchState {
    main_loop: *mut c_void,
    resp: *mut NsResponse,
    error: *mut GError,
}

unsafe extern "C" fn on_fetch_done(_src: *mut c_void, result: *mut c_void, ud: *mut c_void) {
    let st = unsafe { &mut *ud.cast::<FetchState>() };
    st.resp = unsafe { southstar_net::fetch_finish(result, &mut st.error) };
    unsafe { quit_loop(st.main_loop) };
}

pub struct Request<'a> {
    pub url: *const c_char,
    pub top_url: *const c_char,
    pub method: &'a CStr,
    pub body: *const c_void,
    pub body_len: usize,
    pub content_type: *const c_char,
    pub navigation: bool,
    pub user_activated: bool,
    pub headers: *const *const c_char,
}

pub unsafe fn request_blocking(r: &Request, error: *mut *mut GError) -> *mut NsResponse {
    let main_loop = NestedLoop::new();
    let mut st = FetchState {
        main_loop: main_loop.raw(),
        resp: ptr::null_mut(),
        error: ptr::null_mut(),
    };
    let navigate = [
        c"X-ND-Navigate: 1".as_ptr(),
        c"X-ND-User-Activated: 1".as_ptr(),
        ptr::null(),
    ];
    let redirect = [c"X-ND-Navigate: 1".as_ptr(), ptr::null()];
    let headers = match (r.navigation, r.user_activated) {
        (true, true) => navigate.as_ptr(),
        (true, false) => redirect.as_ptr(),
        _ => r.headers,
    };
    unsafe {
        southstar_net::request_async(
            r.url,
            r.top_url,
            r.method.as_ptr(),
            r.body,
            r.body_len,
            r.content_type,
            headers,
            ptr::null_mut(),
            Some(on_fetch_done),
            (&mut st as *mut FetchState).cast(),
        )
    };
    main_loop.run_blocking();
    drop(main_loop);
    if error.is_null() {
        if !st.error.is_null() {
            unsafe { glib::g_error_free(st.error) };
        }
    } else {
        unsafe { *error = st.error };
    }
    st.resp
}

pub fn fetch_blocking_with_headers(
    url: &CStr,
    top_url: Option<&CStr>,
    dest: Dest,
) -> Option<Response> {
    let r = Request {
        url: url.as_ptr(),
        top_url: ptr_or_null(top_url),
        method: c"GET",
        body: ptr::null(),
        body_len: 0,
        content_type: ptr::null(),
        navigation: false,
        user_activated: false,
        headers: accept_headers(dest),
    };
    unsafe { Response::from_raw(request_blocking(&r, ptr::null_mut())) }
}

pub struct Post<'a> {
    pub body: &'a [u8],
    pub content_type: Option<&'a CStr>,
}

fn take_error(error: *mut GError) -> Option<Vec<u8>> {
    // SAFETY: error is null or a GError the finished request handed over.
    let e = unsafe { error.as_ref() }?;
    // SAFETY: a GError's message is null or a NUL-terminated string it owns.
    let message = unsafe { glib::bytes(e.message) }.map(<[u8]>::to_vec);
    // SAFETY: error is owned here and nothing else refers to it.
    unsafe { glib::g_error_free(error) };
    message
}

pub fn navigate_blocking(
    url: &CStr,
    top_url: Option<&CStr>,
    user_activated: bool,
    post: Option<&Post<'_>>,
) -> Result<Response, Option<Vec<u8>>> {
    let r = Request {
        url: url.as_ptr(),
        top_url: ptr_or_null(top_url),
        method: if post.is_some() { c"POST" } else { c"GET" },
        body: post.map_or(ptr::null(), |p| p.body.as_ptr().cast()),
        body_len: post.map_or(0, |p| p.body.len()),
        content_type: post.map_or(ptr::null(), |p| ptr_or_null(p.content_type)),
        navigation: true,
        user_activated,
        headers: ptr::null(),
    };
    let mut error = ptr::null_mut();
    // SAFETY: the request's strings and body outlive the blocking call, and error starts null.
    let resp = unsafe { request_blocking(&r, &mut error) };
    let message = take_error(error);
    // SAFETY: request_blocking hands over a response the caller owns, or null.
    unsafe { Response::from_raw(resp) }.ok_or(message)
}

pub fn fetch_blocking(url: &CStr, top_url: Option<&CStr>) -> Option<Response> {
    let r = Request {
        url: url.as_ptr(),
        top_url: ptr_or_null(top_url),
        method: c"GET",
        body: ptr::null(),
        body_len: 0,
        content_type: ptr::null(),
        navigation: false,
        user_activated: false,
        headers: ptr::null(),
    };
    // SAFETY: the request's strings outlive the blocking call, and a null error slot is allowed.
    let resp = unsafe { request_blocking(&r, ptr::null_mut()) };
    // SAFETY: request_blocking hands over a response the caller owns, or null.
    unsafe { Response::from_raw(resp) }
}

pub struct Bytes(NonNull<GBytes>);

impl Bytes {
    pub fn new(data: &[u8]) -> Bytes {
        Bytes(
            NonNull::new(unsafe { g_bytes_new(data.as_ptr().cast(), data.len()) })
                .expect("g_bytes_new"),
        )
    }

    pub unsafe fn borrow(p: *mut GBytes) -> Option<Bytes> {
        NonNull::new(p)
            .map(|b| Bytes(NonNull::new(unsafe { g_bytes_ref(b.as_ptr()) }).expect("g_bytes_ref")))
    }

    pub fn data(&self) -> &[u8] {
        let mut len = 0usize;
        let data = unsafe { g_bytes_get_data(self.0.as_ptr(), &mut len) };
        if data.is_null() || len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(data.cast(), len) }
    }

    pub fn addr(&self) -> usize {
        self.0.as_ptr() as usize
    }

    pub fn clone_ref(&self) -> Bytes {
        Bytes(NonNull::new(unsafe { g_bytes_ref(self.0.as_ptr()) }).expect("g_bytes_ref"))
    }

    pub fn into_raw(self) -> *mut GBytes {
        let raw = self.0.as_ptr();
        core::mem::forget(self);
        raw
    }
}

impl Drop for Bytes {
    fn drop(&mut self) {
        unsafe { g_bytes_unref(self.0.as_ptr()) };
    }
}

unsafe extern "C" fn bytes_unref(bytes: *mut c_void) {
    // SAFETY: the table only holds GBytes values, each carrying the reference the table owns.
    unsafe { g_bytes_unref(bytes.cast()) };
}

#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct CssCache(NonNull<GHashTable>);

impl CssCache {
    pub fn create() -> CssCache {
        // SAFETY: the keys are g_strdup'd strings and the values GBytes, matching the hash, equality and destroy functions.
        let table = unsafe {
            glib::g_hash_table_new_full(
                Some(glib::g_str_hash),
                Some(glib::g_str_equal),
                Some(glib::g_free),
                Some(bytes_unref),
            )
        };
        CssCache(NonNull::new(table).expect("g_hash_table_new_full"))
    }

    /// # Safety
    ///
    /// No copy of this handle may be used afterwards.
    pub unsafe fn free(self) {
        // SAFETY: the table was made by create() and the caller guarantees nothing uses it again.
        unsafe { glib::g_hash_table_destroy(self.0.as_ptr()) };
    }

    pub fn lookup(self, url: &CStr) -> Option<Bytes> {
        unsafe {
            Bytes::borrow(glib::g_hash_table_lookup(self.0.as_ptr(), url.as_ptr().cast()).cast())
        }
    }

    pub fn insert(self, url: &CStr, bytes: Bytes) {
        unsafe {
            glib::g_hash_table_insert(
                self.0.as_ptr(),
                glib::g_strdup(url.as_ptr()).cast(),
                bytes.into_raw().cast(),
            )
        };
    }
}

struct TimingPtr(*mut ResourceTiming);

unsafe impl Send for TimingPtr {}

const TIMING_CAP: usize = 256;

static TIMINGS: Mutex<Vec<TimingPtr>> = Mutex::new(Vec::new());

unsafe extern "C" fn resource_timing_free(p: *mut c_void) {
    let Some(t) = (unsafe { p.cast::<ResourceTiming>().as_mut() }) else {
        return;
    };
    unsafe {
        glib::g_free(t.top_url.cast());
        glib::g_free(t.url.cast());
        drop(Response::from_raw(t.resp));
        glib::g_free(p);
    }
}

pub fn record_timing(f: &CssFetch, start_us: i64, resp: Response) {
    let Some(top_url) = f.top_url else {
        return;
    };
    let t =
        unsafe { glib::g_malloc0(core::mem::size_of::<ResourceTiming>()) }.cast::<ResourceTiming>();
    unsafe {
        t.write(ResourceTiming {
            top_url: glib::g_strdup(top_url.as_ptr()),
            url: glib::g_strdup(f.url.as_ptr()),
            initiator: f.initiator.as_ptr(),
            start_us,
            end_us: monotonic_us(),
            resp: resp.into_raw(),
            render_blocking: glib::boolean(f.render_blocking),
            in_frame: glib::boolean(f.in_frame),
        })
    };
    let Ok(mut timings) = TIMINGS.lock() else {
        return;
    };
    if timings.len() >= TIMING_CAP {
        let oldest = timings.remove(0);
        unsafe { resource_timing_free(oldest.0.cast()) };
    }
    timings.push(TimingPtr(t));
}

pub fn take_resource_timings(top_url: Option<&CStr>) -> *mut GPtrArray {
    let out = unsafe { g_ptr_array_new_with_free_func(Some(resource_timing_free)) };
    let Some(top_url) = top_url else {
        return out;
    };
    let Ok(mut timings) = TIMINGS.lock() else {
        return out;
    };
    let mut kept = Vec::with_capacity(timings.len());
    for t in timings.drain(..) {
        let matches = c_str(unsafe { (*t.0).top_url }) == Some(top_url);
        if matches {
            unsafe { glib::g_ptr_array_add(out, t.0.cast()) };
        } else {
            kept.push(t);
        }
    }
    *timings = kept;
    out
}

pub fn url_resolve(base: &CStr, href: &CStr) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(Some(base.to_bytes()), href.to_bytes())
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn cstring(bytes: &[u8]) -> CString {
    CString::new(until_nul(bytes)).unwrap_or_default()
}

pub fn url_is_http_or_https(url: &[u8]) -> bool {
    southstar_url::bytes::is_http_or_https(until_nul(url))
}

pub fn url_origin(url: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::origin_from(until_nul(url))
}

pub fn preload_clear() {
    southstar_net::preload_clear();
}

pub fn preconnect(origin: &[u8]) {
    southstar_net::preconnect_async(&cstring(origin));
}

unsafe extern "C" fn on_preload_fetched(_src: *mut c_void, result: *mut c_void, _ud: *mut c_void) {
    drop(unsafe { Response::from_raw(southstar_net::fetch_finish(result, ptr::null_mut())) });
}

pub fn preload_request(url: &[u8], base: &CStr, dest: Dest) {
    let url = cstring(url);
    let headers = accept_headers(dest);
    if dest != Dest::Default
        && let Some(key) = southstar_net::request_key(
            url.to_bytes(),
            Some(base.to_bytes()),
            Some(b"GET"),
            &southstar_net::accept_header_lines(dest as c_int),
        )
    {
        southstar_net::preload_expect(&key);
    }
    unsafe {
        southstar_net::request_async(
            url.as_ptr(),
            base.as_ptr(),
            c"GET".as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            headers,
            ptr::null_mut(),
            Some(on_preload_fetched),
            ptr::null_mut(),
        )
    };
}

pub fn request_async(
    url: *const c_char,
    base: &CStr,
    dest: Dest,
    callback: AsyncReady,
    ud: *mut c_void,
) {
    unsafe {
        southstar_net::request_async(
            url,
            base.as_ptr(),
            c"GET".as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            accept_headers(dest),
            ptr::null_mut(),
            Some(callback),
            ud,
        )
    };
}

pub unsafe fn finish(result: *mut c_void) -> Option<Response> {
    let mut err: *mut GError = ptr::null_mut();
    let resp = unsafe { southstar_net::fetch_finish(result, &mut err) };
    if !err.is_null() {
        unsafe { glib::g_error_free(err) };
    }
    unsafe { Response::from_raw(resp) }
}
