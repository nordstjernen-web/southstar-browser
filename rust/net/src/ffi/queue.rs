//! Southstar — the C ABI of the request queue: asynchronous and blocking fetches over GTask, the limits of 32 fetches at once and 6 per host, sharing in-flight and preloaded responses, preconnects, blob: URLs, idling and shutdown.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::collections::{BTreeMap, VecDeque};
use std::sync::{Condvar, Mutex, MutexGuard};
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

type ReadyCallback =
    Option<unsafe extern "C" fn(source: *mut c_void, result: *mut c_void, user_data: *mut c_void)>;
type ThreadFunc = unsafe extern "C" fn(
    task: *mut c_void,
    source: *mut c_void,
    task_data: *mut c_void,
    cancellable: *mut c_void,
);
type BlobResolver = Option<
    unsafe extern "C" fn(
        url: *const c_char,
        out_type: *mut *mut c_char,
        user_data: *mut c_void,
    ) -> *mut c_void,
>;

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
    fn g_bytes_get_data(bytes: *mut c_void, size: *mut usize) -> *const u8;
    fn g_bytes_unref(bytes: *mut c_void);
    fn g_return_if_fail_warning(
        log_domain: *const c_char,
        pretty_function: *const c_char,
        expression: *const c_char,
    );
    fn ns_debug_log_emit_take(level: c_int, category: *const c_char, message: *mut c_char);
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

static BLOB_RESOLVER: Mutex<(BlobResolver, usize)> = Mutex::new((None, 0));

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
    unsafe { ns_debug_log_emit_take(DLOG_NET, c"fetch".as_ptr(), glib::strdup(&message)) };
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

fn blob_response(url: *const c_char) -> Option<Response> {
    let (resolver, user_data) = *lock(&BLOB_RESOLVER);
    let resolve = resolver.filter(|_| text(url).is_some_and(|u| u.starts_with(b"blob:")))?;
    let mut content_type: *mut c_char = ptr::null_mut();
    let bytes = unsafe { resolve(url, &mut content_type, user_data as *mut c_void) };
    let mut resp = Response::new();
    let r = resp.get();
    r.final_url = unsafe { glib::g_strdup(url) };
    if bytes.is_null() {
        r.status = 404;
        r.error = glib::strdup(b"blob URL not found");
        unsafe { glib::g_free(content_type.cast()) };
    } else {
        let mut len = 0usize;
        let data = unsafe { g_bytes_get_data(bytes, &mut len) };
        resp.append(unsafe { glib::slice(data, len) });
        let r = resp.get();
        r.status = 200;
        r.content_type = content_type;
        unsafe { g_bytes_unref(bytes) };
    }
    Some(resp)
}

fn coalescing_key(
    url: &[u8],
    top_url: Option<&[u8]>,
    method: Option<&[u8]>,
    headers: &[Vec<u8>],
    cancellable: *mut c_void,
    body: *const c_void,
) -> Option<Vec<u8>> {
    if !cancellable.is_null() || !body.is_null() {
        return None;
    }
    fetch::request_key(url, top_url, method, headers)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_idle() -> GBoolean {
    let throttle = lock(&THROTTLE);
    glib::boolean(throttle.active == 0 && throttle.preconnects == 0)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_shutdown() {
    ns_net_join_rng();
    if !drain(DRAIN_TIMEOUT) {
        return;
    }
    hop::backend_shutdown();
    ns_net_transport_shutdown();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_preload_expect(key: *const c_char) {
    if let Some(key) = text(key) {
        lock(&SHARED).coalescer.expect(key);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_preload_clear() {
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_preconnect_async(url: *const c_char) {
    if !text(url).is_some_and(url::is_http_or_https) {
        return;
    }
    let task = Task::new(ptr::null_mut(), None, ptr::null_mut());
    unsafe {
        g_task_set_task_data(task.0, glib::g_strdup(url).cast(), Some(glib::g_free));
        g_task_run_in_thread(task.0, preconnect_thread);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_set_blob_resolver(resolver: BlobResolver, user_data: *mut c_void) {
    *lock(&BLOB_RESOLVER) = (resolver, user_data as usize);
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
    if let Some(resp) = blob_response(url) {
        let task = fetch_async_tag(Task::new(cancellable, callback, user_data));
        return_outcome(task.0, Ok(resp));
        return;
    }

    let headers = unsafe { fetch::header_lines(extra_headers) };
    let key = coalescing_key(
        url_bytes,
        text(top_url),
        text(method),
        &headers,
        cancellable,
        body,
    );
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
        url: url_bytes.to_vec(),
        host: url::host_from(url_bytes),
        top_url: text(top_url).map(<[u8]>::to_vec),
        method: text(method).filter(|m| !m.is_empty()).map(<[u8]>::to_vec),
        content_type: text(content_type)
            .filter(|ct| !ct.is_empty())
            .map(<[u8]>::to_vec),
        body: (!body.is_null() && body_len > 0)
            .then(|| unsafe { glib::slice(body.cast(), body_len) }.to_vec()),
        headers,
        coalesce_key: key,
    });
    let task = Task::new(cancellable, callback, user_data).tagged(
        ns_net_request_async as *const c_void,
        c"ns_net_request_async",
    );
    unsafe { g_task_set_task_data(task.0, Box::into_raw(ctx).cast(), Some(fetch_ctx_free)) };
    submit(task);
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
    let headers = unsafe { fetch::header_lines(extra_headers) };
    let key = coalescing_key(
        url,
        text(top_url),
        text(method),
        &headers,
        cancellable,
        body,
    );
    let outcome = match key.as_deref().and_then(join_blocking) {
        Some(shared) => shared,
        None => {
            let outcome = fetch::fetch(&Fetch {
                url,
                top_url: text(top_url),
                method: text(method),
                body: (!body.is_null()).then(|| unsafe { glib::slice(body.cast(), body_len) }),
                content_type: text(content_type),
                headers: &headers,
                cancellable,
            });
            if let Some(key) = &key {
                deliver(key, outcome.as_ref());
            }
            outcome
        }
    };
    match outcome {
        Ok(resp) => resp.into_raw(),
        Err(failure) => {
            failure.set(error);
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_fetch_blocking(
    url: *const c_char,
    cancellable: *mut c_void,
    error: *mut *mut GError,
) -> *mut NsResponse {
    unsafe {
        ns_net_request_blocking(
            url,
            ptr::null(),
            c"GET".as_ptr(),
            ptr::null(),
            0,
            ptr::null(),
            ptr::null(),
            cancellable,
            error,
        )
    }
}

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
