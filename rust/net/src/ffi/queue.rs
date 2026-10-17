//! Southstar — the C ABI of the request queue: asynchronous and blocking fetches over GTask, the limits of 32 fetches at once and 6 per host, sharing in-flight and preloaded responses, preconnects, blob: URLs, idling and shutdown.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use southstar_glib::{self as glib, GBoolean, GError};

use super::NsResponse;
use super::fetch::{self, Failure, Fetch, Response};
use super::hop;
use super::transport::{
    ns_net_aborting, ns_net_begin_abort, ns_net_join_rng, ns_net_transport_shutdown,
};
use crate::coalesce::{Claim, Coalescer, Shareable};
use crate::request::MAX_TIMEOUT_S;
use crate::{netlog, url};

const MAX_CONCURRENT_FETCHES: usize = 32;
const MAX_FETCHES_PER_HOST: usize = 6;
const JOIN_MAX_WAIT: Duration = Duration::from_secs(MAX_TIMEOUT_S as u64 + 5);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(3000);
const PRECONNECT_CONNECT_TIMEOUT_S: i64 = 6;
const PRECONNECT_TIMEOUT_S: i64 = 10;
const DLOG_NET: c_int = 4;

pub type ReadyCallback =
    Option<unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, user_data: *mut c_void)>;
type ThreadFunc = unsafe extern "C" fn(
    task: *mut c_void,
    source: *mut c_void,
    task_data: *mut c_void,
    cancellable: *mut c_void,
);
pub type BlobResolver = fn(url: &[u8]) -> Option<(Arc<[u8]>, Option<Vec<u8>>)>;

unsafe extern "C" {
    fn g_task_new(
        source_object: *mut c_void,
        cancellable: *mut c_void,
        callback: ReadyCallback,
        callback_data: *mut c_void,
    ) -> *mut c_void;
    fn g_task_set_source_tag(task: *mut c_void, source_tag: *const c_void);
    fn g_task_get_name(task: *mut c_void) -> *const c_char;
    fn g_task_set_name(task: *mut c_void, name: *const c_char);
    fn g_task_set_task_data(task: *mut c_void, data: *mut c_void, destroy: glib::GDestroyNotify);
    fn g_task_get_task_data(task: *mut c_void) -> *mut c_void;
    fn g_task_run_in_thread(task: *mut c_void, func: ThreadFunc);
    fn g_task_return_pointer(task: *mut c_void, result: *mut c_void, destroy: glib::GDestroyNotify);
    fn g_task_return_error(task: *mut c_void, error: *mut GError);
    fn g_task_return_boolean(task: *mut c_void, result: GBoolean);
    fn g_task_is_valid(result: *mut c_void, source_object: *mut c_void) -> GBoolean;
    fn g_task_propagate_pointer(task: *mut c_void, error: *mut *mut GError) -> *mut c_void;
    fn g_object_unref(object: *mut c_void);
    fn g_object_ref(object: *mut c_void) -> *mut c_void;
    fn g_cancellable_new() -> *mut c_void;
    fn g_cancellable_cancel(cancellable: *mut c_void);
    fn g_return_if_fail_warning(
        log_domain: *const c_char,
        pretty_function: *const c_char,
        expression: *const c_char,
    );
}

struct Task(*mut c_void);

unsafe impl Send for Task {}

impl Task {
    fn new(cancellable: *mut c_void, callback: ReadyCallback, user_data: *mut c_void) -> Task {
        Task(unsafe { g_task_new(ptr::null_mut(), cancellable, callback, user_data) })
    }

    fn tagged(self, tag: *const c_void, name: &CStr) -> Task {
        unsafe {
            g_task_set_source_tag(self.0, tag);
            if g_task_get_name(self.0).is_null() {
                g_task_set_name(self.0, name.as_ptr());
            }
        }
        self
    }

    fn context(&self) -> &FetchCtx {
        unsafe { &*g_task_get_task_data(self.0).cast::<FetchCtx>() }
    }
}

impl Drop for Task {
    fn drop(&mut self) {
        unsafe { g_object_unref(self.0) };
    }
}

unsafe extern "C" fn response_free(data: *mut c_void) {
    unsafe { super::ns_response_free(data.cast()) };
}

fn return_outcome(task: *mut c_void, outcome: Result<Response, Failure>) {
    match outcome {
        Ok(resp) => unsafe {
            g_task_return_pointer(task, resp.into_raw().cast(), Some(response_free))
        },
        Err(failure) => unsafe { g_task_return_error(task, failure.into_error()) },
    }
}

fn fetch_async_tag(task: Task) -> Task {
    task.tagged(ns_net_fetch_async as *const c_void, c"ns_net_fetch_async")
}

struct FetchCtx {
    url: Vec<u8>,
    host: Option<Vec<u8>>,
    top_url: Option<Vec<u8>>,
    method: Option<Vec<u8>>,
    content_type: Option<Vec<u8>>,
    body: Option<Vec<u8>>,
    headers: Vec<Vec<u8>>,
    coalesce_key: Option<Vec<u8>>,
}

unsafe extern "C" fn fetch_ctx_free(data: *mut c_void) {
    drop(unsafe { Box::from_raw(data.cast::<FetchCtx>()) });
}

impl Shareable for Response {
    fn preloadable_len(&self) -> Option<usize> {
        let r = self.view();
        let body = unsafe { r.body.as_ref() }?;
        let forbids_reuse = unsafe { glib::bytes(r.raw_headers) }.is_some_and(|raw| {
            raw.to_ascii_lowercase()
                .windows(b"no-store".len())
                .any(|w| w == b"no-store")
        });
        (r.status == 200 && r.error.is_null() && !forbids_reuse).then_some(body.len as usize)
    }

    fn duplicate(&self) -> Response {
        self.copy()
    }
}

enum Waiter {
    Task(Task),
    Blocking(u64),
}

struct Shared {
    coalescer: Coalescer<Response, Waiter>,
    finished: BTreeMap<u64, Result<Response, Failure>>,
    next_waiter: u64,
}

static SHARED: Mutex<Shared> = Mutex::new(Shared {
    coalescer: Coalescer::new(),
    finished: BTreeMap::new(),
    next_waiter: 0,
});
static SHARED_DONE: Condvar = Condvar::new();

struct Throttle {
    active: usize,
    preconnects: usize,
    queue: VecDeque<Task>,
    per_host: BTreeMap<Vec<u8>, usize>,
}

static THROTTLE: Mutex<Throttle> = Mutex::new(Throttle {
    active: 0,
    preconnects: 0,
    queue: VecDeque::new(),
    per_host: BTreeMap::new(),
});
static IDLE: Condvar = Condvar::new();

static BLOB_RESOLVER: Mutex<Option<BlobResolver>> = Mutex::new(None);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

fn aborting() -> bool {
    ns_net_aborting() != 0
}

fn copy_outcome(outcome: Result<&Response, &Failure>) -> Result<Response, Failure> {
    match outcome {
        Ok(resp) => Ok(resp.copy()),
        Err(failure) => Err(failure.clone()),
    }
}

fn deliver(key: &[u8], outcome: Result<&Response, &Failure>) {
    let mut shared = lock(&SHARED);
    let Some(waiters) = shared.coalescer.deliver(key, outcome.ok()) else {
        return;
    };
    let mut tasks = Vec::new();
    let mut woke = false;
    for waiter in waiters {
        match waiter {
            Waiter::Task(task) => tasks.push(task),
            Waiter::Blocking(id) => {
                shared.finished.insert(id, copy_outcome(outcome));
                woke = true;
            }
        }
    }
    if woke {
        SHARED_DONE.notify_all();
    }
    drop(shared);
    for task in tasks {
        return_outcome(task.0, copy_outcome(outcome));
    }
}

fn join_async(key: &[u8], callback: ReadyCallback, user_data: *mut c_void) -> Claim<Response> {
    lock(&SHARED).coalescer.claim(key, || {
        Waiter::Task(fetch_async_tag(Task::new(
            ptr::null_mut(),
            callback,
            user_data,
        )))
    })
}

fn join_blocking(key: &[u8]) -> Option<Result<Response, Failure>> {
    let mut shared = lock(&SHARED);
    let id = shared.next_waiter;
    match shared.coalescer.claim(key, || Waiter::Blocking(id)) {
        Claim::Lead => None,
        Claim::Preloaded(resp) => Some(Ok(resp)),
        Claim::Joined => {
            shared.next_waiter += 1;
            let deadline = Instant::now() + JOIN_MAX_WAIT;
            loop {
                if let Some(outcome) = shared.finished.remove(&id) {
                    return Some(outcome);
                }
                let now = Instant::now();
                if aborting() || now >= deadline {
                    break;
                }
                shared = SHARED_DONE
                    .wait_timeout(shared, deadline - now)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            shared.coalescer.leave(
                key,
                |w| matches!(w, Waiter::Blocking(other) if *other == id),
            );
            Some(Err(Failure::Net(0, b"shared fetch abandoned".to_vec())))
        }
    }
}

fn adjust_host(per_host: &mut BTreeMap<Vec<u8>, usize>, host: Option<&[u8]>, add: bool) {
    let Some(host) = host else {
        return;
    };
    let count = per_host.get(host).copied().unwrap_or(0);
    let count = if add {
        count + 1
    } else {
        count.saturating_sub(1)
    };
    if count == 0 {
        per_host.remove(host);
    } else {
        per_host.insert(host.to_vec(), count);
    }
}

fn dispatch() {
    loop {
        let task = {
            let mut guard = lock(&THROTTLE);
            let throttle = &mut *guard;
            if throttle.active >= MAX_CONCURRENT_FETCHES || throttle.queue.is_empty() {
                return;
            }
            let per_host = &throttle.per_host;
            let Some(at) = throttle.queue.iter().position(|task| {
                task.context()
                    .host
                    .as_ref()
                    .is_none_or(|h| per_host.get(h).copied().unwrap_or(0) < MAX_FETCHES_PER_HOST)
            }) else {
                return;
            };
            let Some(task) = throttle.queue.remove(at) else {
                return;
            };
            throttle.active += 1;
            let host = task.context().host.clone();
            adjust_host(&mut throttle.per_host, host.as_deref(), true);
            task
        };
        unsafe { g_task_run_in_thread(task.0, fetch_thread) };
    }
}

fn submit(task: Task) {
    lock(&THROTTLE).queue.push_back(task);
    dispatch();
}

fn log_fetch(message: Vec<u8>) {
    southstar_debuglog::emit(DLOG_NET, b"fetch", &message);
}

fn log_outcome(url: &[u8], outcome: Result<&Response, &Failure>) {
    match outcome {
        Err(failure) => log_fetch([b"failed ", url, b": ", failure.message()].concat()),
        Ok(resp) => {
            let r = resp.view();
            match text(r.error) {
                Some(error) => log_fetch([b"error ", url, b": ", error].concat()),
                None => {
                    let shown = text(r.final_url).unwrap_or(url);
                    let len = unsafe { r.body.as_ref() }.map_or(0, |b| b.len);
                    let head = format!("{} ", r.status).into_bytes();
                    let tail = format!(" ({len} bytes)").into_bytes();
                    log_fetch([&head[..], shown, &tail].concat());
                }
            }
        }
    }
}

unsafe extern "C" fn fetch_thread(
    task: *mut c_void,
    _source: *mut c_void,
    task_data: *mut c_void,
    cancellable: *mut c_void,
) {
    let ctx = unsafe { &*task_data.cast::<FetchCtx>() };
    let outcome = fetch::fetch(&Fetch {
        url: &ctx.url,
        top_url: ctx.top_url.as_deref(),
        method: ctx.method.as_deref(),
        body: ctx.body.as_deref(),
        content_type: ctx.content_type.as_deref(),
        headers: &ctx.headers,
        cancellable,
    });
    if let Some(key) = &ctx.coalesce_key {
        deliver(key, outcome.as_ref());
    }
    if netlog::logging() {
        log_outcome(&ctx.url, outcome.as_ref());
    }
    return_outcome(task, outcome);
    {
        let mut throttle = lock(&THROTTLE);
        throttle.active = throttle.active.saturating_sub(1);
        adjust_host(&mut throttle.per_host, ctx.host.as_deref(), false);
        IDLE.notify_all();
    }
    dispatch();
}

fn drain(timeout: Duration) -> bool {
    ns_net_begin_abort();
    let dropped: Vec<Task> = lock(&THROTTLE).queue.drain(..).collect();
    for task in dropped {
        let failure = Failure::Net(1, b"shutting down".to_vec());
        if let Some(key) = &task.context().coalesce_key {
            deliver(key, Err(&failure));
        }
        return_outcome(task.0, Err(failure));
    }
    let deadline = Instant::now() + timeout;
    let mut throttle = lock(&THROTTLE);
    while throttle.active > 0 || throttle.preconnects > 0 {
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        let (guard, waited) = IDLE
            .wait_timeout(throttle, deadline - now)
            .unwrap_or_else(|e| e.into_inner());
        throttle = guard;
        if waited.timed_out() {
            return throttle.active == 0 && throttle.preconnects == 0;
        }
    }
    true
}

fn blob_response(url: &[u8]) -> Option<Response> {
    let resolve = lock(&BLOB_RESOLVER).filter(|_| url.starts_with(b"blob:"))?;
    let mut resp = Response::new();
    resp.get().final_url = glib::strdup(url);
    match resolve(url) {
        Some((bytes, content_type)) => {
            resp.append(&bytes);
            let r = resp.get();
            r.status = 200;
            r.content_type = content_type.map_or(ptr::null_mut(), |ct| glib::strdup(&ct));
        }
        None => {
            let r = resp.get();
            r.status = 404;
            r.error = glib::strdup(b"blob URL not found");
        }
    }
    Some(resp)
}

fn coalescing_key(req: &FetchRequest<'_>, cancellable: *mut c_void) -> Option<Vec<u8>> {
    if !cancellable.is_null() || req.body.is_some() {
        return None;
    }
    fetch::request_key(req.url, req.top_url, req.method, req.headers)
}

#[derive(Clone, Copy, Default)]
pub struct FetchRequest<'a> {
    pub url: &'a [u8],
    pub top_url: Option<&'a [u8]>,
    pub method: Option<&'a [u8]>,
    pub body: Option<&'a [u8]>,
    pub content_type: Option<&'a [u8]>,
    pub headers: &'a [Vec<u8>],
}

pub struct Cancellable(ptr::NonNull<c_void>);

impl Cancellable {
    pub fn new() -> Cancellable {
        // SAFETY: g_cancellable_new has no preconditions and never returns null.
        let raw = unsafe { g_cancellable_new() };
        Cancellable(ptr::NonNull::new(raw).expect("g_cancellable_new"))
    }

    pub fn cancel(&self) {
        // SAFETY: the handle holds a reference to a live GCancellable.
        unsafe { g_cancellable_cancel(self.0.as_ptr()) };
    }

    fn as_ptr(&self) -> *mut c_void {
        self.0.as_ptr()
    }
}

impl Default for Cancellable {
    fn default() -> Cancellable {
        Cancellable::new()
    }
}

impl Clone for Cancellable {
    fn clone(&self) -> Cancellable {
        // SAFETY: the handle holds a reference to a live GCancellable, and the copy owns a new one.
        unsafe { g_object_ref(self.0.as_ptr()) };
        Cancellable(self.0)
    }
}

impl Drop for Cancellable {
    fn drop(&mut self) {
        // SAFETY: the handle owns one reference, given up here.
        unsafe { g_object_unref(self.0.as_ptr()) };
    }
}

// SAFETY: GCancellable is thread-safe and its reference count is atomic.
unsafe impl Send for Cancellable {}

pub type Outcome = Result<Response, Vec<u8>>;

type Done = Box<dyn FnOnce(Outcome)>;

unsafe extern "C" fn request_done(_source: *mut c_void, result: *mut c_void, data: *mut c_void) {
    // SAFETY: data is the boxed callback request_with leaked for this request, reclaimed once.
    let done = unsafe { Box::from_raw(data.cast::<Done>()) };
    let mut error: *mut GError = ptr::null_mut();
    // SAFETY: result is the GAsyncResult of a request task, and error is a null local slot.
    let resp = unsafe { ns_net_fetch_finish(result, &mut error) };
    // SAFETY: the error, if any, is the GError the finish call handed over.
    let message = unsafe { error.as_ref() }
        .map(|e| text(e.message).unwrap_or_default().to_vec())
        .unwrap_or_default();
    if !error.is_null() {
        // SAFETY: the error is owned here and freed once.
        unsafe { glib::g_error_free(error) };
    }
    // SAFETY: a finished request's response is new and owned by nobody else.
    let outcome = unsafe { Response::from_raw(resp) }.ok_or(message);
    (*done)(outcome);
}

pub fn request_with(
    req: &FetchRequest<'_>,
    cancellable: Option<&Cancellable>,
    done: impl FnOnce(Outcome) + 'static,
) {
    let done: Box<Done> = Box::new(Box::new(done));
    submit_request(
        req,
        cancellable.map_or(ptr::null_mut(), Cancellable::as_ptr),
        Some(request_done),
        Box::into_raw(done).cast(),
    );
}

fn submit_request(
    req: &FetchRequest<'_>,
    cancellable: *mut c_void,
    callback: ReadyCallback,
    user_data: *mut c_void,
) {
    if let Some(resp) = blob_response(req.url) {
        let task = fetch_async_tag(Task::new(cancellable, callback, user_data));
        return_outcome(task.0, Ok(resp));
        return;
    }
    let key = coalescing_key(req, cancellable);
    if let Some(key) = &key {
        match join_async(key, callback, user_data) {
            Claim::Preloaded(resp) => {
                let task = fetch_async_tag(Task::new(cancellable, callback, user_data));
                return_outcome(task.0, Ok(resp));
                return;
            }
            Claim::Joined => return,
            Claim::Lead => {}
        }
    }
    let ctx = Box::new(FetchCtx {
        url: req.url.to_vec(),
        host: url::host_from(req.url),
        top_url: req.top_url.map(<[u8]>::to_vec),
        method: req.method.filter(|m| !m.is_empty()).map(<[u8]>::to_vec),
        content_type: req
            .content_type
            .filter(|ct| !ct.is_empty())
            .map(<[u8]>::to_vec),
        body: req.body.filter(|b| !b.is_empty()).map(<[u8]>::to_vec),
        headers: req.headers.to_vec(),
        coalesce_key: key,
    });
    let task = Task::new(cancellable, callback, user_data).tagged(
        ns_net_request_async as *const c_void,
        c"ns_net_request_async",
    );
    // SAFETY: the task is live, and it takes ownership of the context and frees it with fetch_ctx_free.
    unsafe { g_task_set_task_data(task.0, Box::into_raw(ctx).cast(), Some(fetch_ctx_free)) };
    submit(task);
}

pub fn request_blocking(req: &FetchRequest<'_>) -> Result<Response, Failure> {
    blocking(req, ptr::null_mut())
}

fn blocking(req: &FetchRequest<'_>, cancellable: *mut c_void) -> Result<Response, Failure> {
    let key = coalescing_key(req, cancellable);
    if let Some(shared) = key.as_deref().and_then(join_blocking) {
        return shared;
    }
    let outcome = fetch::fetch(&Fetch {
        url: req.url,
        top_url: req.top_url,
        method: req.method,
        body: req.body,
        content_type: req.content_type,
        headers: req.headers,
        cancellable,
    });
    if let Some(key) = &key {
        deliver(key, outcome.as_ref());
    }
    outcome
}

pub fn idle() -> bool {
    let throttle = lock(&THROTTLE);
    throttle.active == 0 && throttle.preconnects == 0
}

pub fn shutdown() {
    ns_net_join_rng();
    if !drain(DRAIN_TIMEOUT) {
        return;
    }
    hop::backend_shutdown();
    ns_net_transport_shutdown();
}

pub fn preload_expect(key: &[u8]) {
    lock(&SHARED).coalescer.expect(key);
}

pub fn preload_clear() {
    lock(&SHARED).coalescer.clear();
}

fn preconnect(url: &[u8]) {
    let Some(target) = southstar_http::parse_target(url) else {
        return;
    };
    let route = crate::route::route(url, &target.host);
    let request = southstar_http::Request {
        url,
        https: target.https,
        host: &target.host,
        port: target.port,
        authority: &target.authority,
        path: &target.path,
        method: b"GET",
        user_agent: None,
        referer: None,
        cookie: None,
        extra_headers: Vec::new(),
        body: b"",
        timeout: Duration::from_secs(PRECONNECT_TIMEOUT_S as u64),
        connect_timeout: Duration::from_secs(PRECONNECT_CONNECT_TIMEOUT_S as u64),
        allow_insecure: route.allow_insecure,
        tls: route.tls,
        proxy: route.proxy,
    };
    southstar_http::preconnect(&request, &aborting);
}

unsafe extern "C" fn preconnect_thread(
    task: *mut c_void,
    _source: *mut c_void,
    task_data: *mut c_void,
    _cancellable: *mut c_void,
) {
    let url = task_data.cast::<c_char>().cast_const();
    lock(&THROTTLE).preconnects += 1;
    if let Some(url) = text(url).filter(|_| !aborting()) {
        preconnect(url);
    }
    {
        let mut throttle = lock(&THROTTLE);
        throttle.preconnects = throttle.preconnects.saturating_sub(1);
        IDLE.notify_all();
    }
    unsafe { g_task_return_boolean(task, 1) };
}

pub fn preconnect_async(url: &CStr) {
    if !url::is_http_or_https(url.to_bytes()) {
        return;
    }
    let task = Task::new(ptr::null_mut(), None, ptr::null_mut());
    unsafe {
        g_task_set_task_data(
            task.0,
            glib::g_strdup(url.as_ptr()).cast(),
            Some(glib::g_free),
        );
        g_task_run_in_thread(task.0, preconnect_thread);
    }
}

pub fn set_blob_resolver(resolver: BlobResolver) {
    *lock(&BLOB_RESOLVER) = Some(resolver);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_fetch_async(
    url: *const c_char,
    top_url: *const c_char,
    cancellable: *mut c_void,
    callback: ReadyCallback,
    user_data: *mut c_void,
) {
    unsafe {
        ns_net_request_async(
            url,
            top_url,
            c"GET".as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            ptr::null(),
            cancellable,
            callback,
            user_data,
        )
    };
}

/// # Safety
///
/// `url` and every non-null `top_url`, `method` and `content_type` are NUL-terminated,
/// `body` holds `body_len` readable bytes or is null, `extra_headers` is null or a
/// NULL-terminated array of NUL-terminated lines, and `cancellable` is null or a live
/// GCancellable; `callback` receives `user_data` on the main context.
#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_net_request_async(
    url: *const c_char,
    top_url: *const c_char,
    method: *const c_char,
    body: *const c_void,
    body_len: usize,
    content_type: *const c_char,
    extra_headers: *const *const c_char,
    cancellable: *mut c_void,
    callback: ReadyCallback,
    user_data: *mut c_void,
) {
    let Some(url_bytes) = text(url) else {
        unsafe {
            g_return_if_fail_warning(
                ptr::null(),
                c"ns_net_request_async".as_ptr(),
                c"url != NULL".as_ptr(),
            )
        };
        return;
    };
    // SAFETY: the caller passes extra_headers as null or a NULL-terminated array of C strings.
    let headers = unsafe { fetch::header_lines(extra_headers) };
    let body = (!body.is_null() && body_len > 0)
        // SAFETY: a non-null body holds body_len readable bytes.
        .then(|| unsafe { glib::slice(body.cast(), body_len) });
    let req = FetchRequest {
        url: url_bytes,
        top_url: text(top_url),
        method: text(method),
        body,
        content_type: text(content_type),
        headers: &headers,
    };
    submit_request(&req, cancellable, callback, user_data);
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_net_request_blocking(
    url: *const c_char,
    top_url: *const c_char,
    method: *const c_char,
    body: *const c_void,
    body_len: usize,
    content_type: *const c_char,
    extra_headers: *const *const c_char,
    cancellable: *mut c_void,
    error: *mut *mut GError,
) -> *mut NsResponse {
    let Some(url) = text(url) else {
        return ptr::null_mut();
    };
    // SAFETY: the caller passes extra_headers as null or a NULL-terminated array of C strings.
    let headers = unsafe { fetch::header_lines(extra_headers) };
    let req = FetchRequest {
        url,
        top_url: text(top_url),
        method: text(method),
        // SAFETY: a non-null body holds body_len readable bytes.
        body: (!body.is_null()).then(|| unsafe { glib::slice(body.cast(), body_len) }),
        content_type: text(content_type),
        headers: &headers,
    };
    match blocking(&req, cancellable) {
        Ok(resp) => resp.into_raw(),
        Err(failure) => {
            failure.set(error);
            ptr::null_mut()
        }
    }
}

pub fn fetch_blocking(url: &CStr) -> Option<Response> {
    request_blocking(&FetchRequest {
        url: url.to_bytes(),
        method: Some(b"GET"),
        ..FetchRequest::default()
    })
    .ok()
}

/// # Safety
///
/// `result` is the GAsyncResult a request callback received, and `error` is null or
/// points to a null GError slot.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_fetch_finish(
    result: *mut c_void,
    error: *mut *mut GError,
) -> *mut NsResponse {
    if unsafe { g_task_is_valid(result, ptr::null_mut()) } == 0 {
        unsafe {
            g_return_if_fail_warning(
                ptr::null(),
                c"ns_net_fetch_finish".as_ptr(),
                c"g_task_is_valid(result, NULL)".as_ptr(),
            )
        };
        return ptr::null_mut();
    }
    unsafe { g_task_propagate_pointer(result, error) }.cast()
}
