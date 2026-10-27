//! Southstar — requests through the network crate, blocking ones on a nested main loop, the response view, stylesheet bytes and their cache, resource timings and preloads.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int, c_void};
use core::ptr::{self, NonNull};
use std::collections::{BTreeMap, HashMap};
use std::ffi::CString;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use southstar_glib::{self as glib, GBoolean};
pub use southstar_net::{Outcome, Response};

#[repr(C)]
struct GMainLoop {
    _private: [u8; 0],
}

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
}

pub fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub struct NestedLoop(NonNull<GMainLoop>);

impl NestedLoop {
    pub fn new() -> NestedLoop {
        NestedLoop(
            NonNull::new(unsafe { g_main_loop_new(ptr::null_mut(), glib::FALSE) })
                .expect("g_main_loop_new"),
        )
    }

    pub fn run_blocking(&self) {
        let _blocking = southstar_net::BlockingFetch::enter();
        unsafe { g_main_loop_run(self.0.as_ptr()) };
    }

    pub fn quit(&self) {
        // SAFETY: the loop is live while this handle owns its reference.
        unsafe { g_main_loop_quit(self.0.as_ptr()) };
    }
}

impl Drop for NestedLoop {
    fn drop(&mut self) {
        unsafe { g_main_loop_unref(self.0.as_ptr()) };
    }
}

pub fn nosniff(resp: &Response) -> bool {
    resp.x_content_type_options()
        .is_some_and(|value| southstar_net::header_is_nosniff(value.to_bytes()))
}

fn accept_headers(dest: Dest) -> Vec<Vec<u8>> {
    southstar_net::accept_header_lines(dest as c_int)
}

struct Request<'a> {
    url: &'a CStr,
    top_url: Option<&'a CStr>,
    method: &'a [u8],
    body: Option<&'a [u8]>,
    content_type: Option<&'a CStr>,
    headers: Vec<Vec<u8>>,
}

impl Request<'_> {
    fn submit(&self, done: impl FnOnce(Outcome) + 'static) {
        let req = southstar_net::FetchRequest {
            url: self.url.to_bytes(),
            top_url: self.top_url.map(CStr::to_bytes),
            method: Some(self.method),
            body: self.body,
            content_type: self.content_type.map(CStr::to_bytes),
            headers: &self.headers,
        };
        southstar_net::request_with(&req, None, done);
    }

    fn blocking(&self) -> Outcome {
        let main_loop = Rc::new(NestedLoop::new());
        let outcome = Rc::new(RefCell::new(None));
        let (done_loop, done) = (Rc::clone(&main_loop), Rc::clone(&outcome));
        self.submit(move |result| {
            done.replace(Some(result));
            done_loop.quit();
        });
        main_loop.run_blocking();
        outcome.take().unwrap_or_else(|| Err(Vec::new()))
    }
}

fn navigation_headers(user_activated: bool, initiator: Option<&CStr>) -> Vec<Vec<u8>> {
    let mut headers = vec![b"X-ND-Navigate: 1".to_vec()];
    if user_activated {
        headers.push(b"X-ND-User-Activated: 1".to_vec());
    }
    if let Some(initiator) = initiator.filter(|initiator| !initiator.is_empty()) {
        headers.push([&b"X-ND-Initiator: "[..], initiator.to_bytes()].concat());
    }
    headers
}

pub fn fetch_blocking_with_headers(
    url: &CStr,
    top_url: Option<&CStr>,
    dest: Dest,
) -> Option<Response> {
    Request {
        url,
        top_url,
        method: b"GET",
        body: None,
        content_type: None,
        headers: accept_headers(dest),
    }
    .blocking()
    .ok()
}

pub struct Post<'a> {
    pub body: &'a [u8],
    pub content_type: Option<&'a CStr>,
}

pub fn navigate_blocking(
    url: &CStr,
    top_url: Option<&CStr>,
    initiator: Option<&CStr>,
    user_activated: bool,
    post: Option<&Post<'_>>,
) -> Result<Response, Option<Vec<u8>>> {
    Request {
        url,
        top_url,
        method: if post.is_some() { b"POST" } else { b"GET" },
        body: post.map(|p| p.body),
        content_type: post.and_then(|p| p.content_type),
        headers: navigation_headers(user_activated, initiator),
    }
    .blocking()
    .map_err(|message| (!message.is_empty()).then_some(message))
}

pub fn fetch_blocking(url: &CStr, top_url: Option<&CStr>) -> Option<Response> {
    Request {
        url,
        top_url,
        method: b"GET",
        body: None,
        content_type: None,
        headers: Vec::new(),
    }
    .blocking()
    .ok()
}

#[derive(Clone)]
pub struct Bytes(Arc<[u8]>);

impl Bytes {
    pub fn new(data: &[u8]) -> Bytes {
        Bytes(Arc::from(data))
    }

    pub fn data(&self) -> &[u8] {
        &self.0
    }

    pub fn addr(&self) -> usize {
        Arc::as_ptr(&self.0).cast::<u8>() as usize
    }

    pub fn clone_ref(&self) -> Bytes {
        self.clone()
    }
}

static CSS_CACHES: Mutex<BTreeMap<u64, HashMap<CString, Bytes>>> = Mutex::new(BTreeMap::new());
static NEXT_CSS_CACHE: AtomicU64 = AtomicU64::new(1);

fn css_caches() -> MutexGuard<'static, BTreeMap<u64, HashMap<CString, Bytes>>> {
    CSS_CACHES.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(Clone, Copy)]
pub struct CssCache(u64);

impl CssCache {
    pub fn create() -> CssCache {
        let id = NEXT_CSS_CACHE.fetch_add(1, Ordering::Relaxed);
        css_caches().insert(id, HashMap::new());
        CssCache(id)
    }

    pub fn free(self) {
        css_caches().remove(&self.0);
    }

    pub fn lookup(self, url: &CStr) -> Option<Bytes> {
        css_caches().get(&self.0)?.get(url).cloned()
    }

    pub fn insert(self, url: &CStr, bytes: Bytes) {
        if let Some(cache) = css_caches().get_mut(&self.0) {
            cache.insert(url.to_owned(), bytes);
        }
    }
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

pub fn preload_request(url: &[u8], base: &CStr, dest: Dest) {
    let url = cstring(url);
    let headers = accept_headers(dest);
    if dest != Dest::Default
        && let Some(key) = southstar_net::request_key(
            url.to_bytes(),
            Some(base.to_bytes()),
            Some(b"GET"),
            &headers,
        )
    {
        southstar_net::preload_expect(&key);
    }
    Request {
        url: &url,
        top_url: Some(base),
        method: b"GET",
        body: None,
        content_type: None,
        headers,
    }
    .submit(drop);
}

pub fn request_async(url: &CStr, base: &CStr, dest: Dest, done: impl FnOnce(Outcome) + 'static) {
    Request {
        url,
        top_url: Some(base),
        method: b"GET",
        body: None,
        content_type: None,
        headers: accept_headers(dest),
    }
    .submit(done);
}
