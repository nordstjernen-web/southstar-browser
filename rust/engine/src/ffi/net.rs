//! Southstar — blocking requests on a nested main loop, the response view, stylesheet bytes and their cache, resource timings and preloads.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI32, Ordering};

use southstar_glib::{self as glib, GBoolean, GError, GHashTable, GPtrArray, GStr};

use crate::fetch::CssFetch;

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

#[repr(C)]
pub struct NsResponse {
    status: c_long,
    final_url: *mut c_char,
    content_type: *mut c_char,
    _content_disposition: *mut c_char,
    _csp_header: *mut c_char,
    _xframe_options: *mut c_char,
    x_content_type_options: *mut c_char,
    cors_allow_origin: *mut c_char,
    _refresh: *mut c_char,
    _content_language: *mut c_char,
    _raw_headers: *mut c_char,
    body: *mut GByteArray,
    error: *mut c_char,
}

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
    fn ns_net_request_async(
        url: *const c_char,
        top_url: *const c_char,
        method: *const c_char,
        body: *const c_void,
        body_len: usize,
        content_type: *const c_char,
        extra_headers: *const *const c_char,
        cancellable: *mut c_void,
        callback: AsyncReady,
        user_data: *mut c_void,
    );
    fn ns_net_fetch_finish(result: *mut c_void, error: *mut *mut GError) -> *mut NsResponse;
    fn ns_net_accept_headers_for(dest: c_int) -> *const *const c_char;
    fn ns_net_header_is_nosniff(value: *const c_char) -> GBoolean;
    fn ns_net_preload_clear();
    fn ns_net_preconnect_async(url: *const c_char);
    fn ns_net_request_key(
        url: *const c_char,
        top_url: *const c_char,
        method: *const c_char,
        extra_headers: *const *const c_char,
    ) -> *mut c_char;
    fn ns_net_preload_expect(key: *const c_char);
    fn ns_response_free(resp: *mut NsResponse);
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_url_is_http_or_https(url: *const c_char) -> GBoolean;
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
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

pub struct Response(NonNull<NsResponse>);

impl Response {
    pub unsafe fn take(resp: *mut NsResponse) -> Option<Response> {
        NonNull::new(resp).map(Response)
    }

    fn raw(&self) -> &NsResponse {
        unsafe { self.0.as_ref() }
    }

    pub fn into_raw(self) -> *mut NsResponse {
        let raw = self.0.as_ptr();
        core::mem::forget(self);
        raw
    }

    pub fn status(&self) -> c_long {
        self.raw().status
    }

    pub fn error(&self) -> Option<&CStr> {
        c_str(self.raw().error)
    }

    pub fn content_type(&self) -> Option<&[u8]> {
        c_str(self.raw().content_type).map(CStr::to_bytes)
    }

    pub fn final_url(&self) -> *const c_char {
        self.raw().final_url
    }

    pub fn cors_allow_origin(&self) -> *const c_char {
        self.raw().cors_allow_origin
    }

    pub fn nosniff(&self) -> bool {
        unsafe { ns_net_header_is_nosniff(self.raw().x_content_type_options) != 0 }
    }

    pub fn body(&self) -> Option<&[u8]> {
        let b = unsafe { self.raw().body.as_ref() }?;
        Some(if b.data.is_null() || b.len == 0 {
            &[]
        } else {
            unsafe { core::slice::from_raw_parts(b.data, b.len as usize) }
        })
    }
}

impl Drop for Response {
    fn drop(&mut self) {
        unsafe { ns_response_free(self.0.as_ptr()) };
    }
}

pub fn accept_headers(dest: Dest) -> *const *const c_char {
    unsafe { ns_net_accept_headers_for(dest as c_int) }
}

struct FetchState {
    main_loop: *mut c_void,
    resp: *mut NsResponse,
    error: *mut GError,
}

unsafe extern "C" fn on_fetch_done(_src: *mut c_void, result: *mut c_void, ud: *mut c_void) {
    let st = unsafe { &mut *ud.cast::<FetchState>() };
    st.resp = unsafe { ns_net_fetch_finish(result, &mut st.error) };
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
        ns_net_request_async(
            r.url,
            r.top_url,
            r.method.as_ptr(),
            r.body,
            r.body_len,
            r.content_type,
            headers,
            ptr::null_mut(),
            on_fetch_done,
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
    unsafe { Response::take(request_blocking(&r, ptr::null_mut())) }
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

#[derive(Clone, Copy)]
pub struct CssCache(NonNull<GHashTable>);

impl CssCache {
    pub unsafe fn from_ptr(table: *mut GHashTable) -> Option<CssCache> {
        NonNull::new(table).map(CssCache)
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
        ns_response_free(t.resp);
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
    unsafe { GStr::take(ns_url_resolve(base.as_ptr(), href.as_ptr())) }
        .map(|s| s.to_bytes().to_vec())
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub fn url_is_http_or_https(url: &[u8]) -> bool {
    unsafe { ns_url_is_http_or_https(cstring(url).as_ptr()) != 0 }
}

pub fn url_origin(url: &[u8]) -> Option<Vec<u8>> {
    unsafe { GStr::take(ns_url_origin_from(cstring(url).as_ptr())) }.map(|s| s.to_bytes().to_vec())
}

pub fn preload_clear() {
    unsafe { ns_net_preload_clear() };
}

pub fn preconnect(origin: &[u8]) {
    unsafe { ns_net_preconnect_async(cstring(origin).as_ptr()) };
}

unsafe extern "C" fn on_preload_fetched(_src: *mut c_void, result: *mut c_void, _ud: *mut c_void) {
    drop(unsafe { Response::take(ns_net_fetch_finish(result, ptr::null_mut())) });
}

pub fn preload_request(url: &[u8], base: &CStr, dest: Dest) {
    let url = cstring(url);
    let headers = accept_headers(dest);
    if dest != Dest::Default {
        let key = unsafe {
            GStr::take(ns_net_request_key(
                url.as_ptr(),
                base.as_ptr(),
                c"GET".as_ptr(),
                headers,
            ))
        };
        unsafe { ns_net_preload_expect(key.as_deref().map_or(ptr::null(), CStr::as_ptr)) };
    }
    unsafe {
        ns_net_request_async(
            url.as_ptr(),
            base.as_ptr(),
            c"GET".as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            headers,
            ptr::null_mut(),
            on_preload_fetched,
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
        ns_net_request_async(
            url,
            base.as_ptr(),
            c"GET".as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            accept_headers(dest),
            ptr::null_mut(),
            callback,
            ud,
        )
    };
}

pub unsafe fn finish(result: *mut c_void) -> Option<Response> {
    let mut err: *mut GError = ptr::null_mut();
    let resp = unsafe { ns_net_fetch_finish(result, &mut err) };
    if !err.is_null() {
        unsafe { glib::g_error_free(err) };
    }
    unsafe { Response::take(resp) }
}
