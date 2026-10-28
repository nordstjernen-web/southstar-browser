//! Southstar — the request queue: asynchronous fetches on their own threads with the result handed back to the caller's main context, blocking fetches, the limits of 32 fetches at once and 6 per host, sharing in-flight and preloaded responses, preconnects, blob: URLs, idling and shutdown.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int};
use core::mem::ManuallyDrop;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};

use southstar_glib::MainContext;

use super::fetch::{self, Failure, Fetch};
use super::hop;
use super::transport::{
    ns_net_aborting, ns_net_begin_abort, ns_net_join_rng, ns_net_transport_shutdown,
};
use crate::coalesce::{Claim, Coalescer, Shareable};
use crate::request::MAX_TIMEOUT_S;
use crate::response::{Response, cstr, text as field};
use crate::{netlog, url};

const MAX_CONCURRENT_FETCHES: usize = 32;
const MAX_FETCHES_PER_HOST: usize = 6;
const JOIN_MAX_WAIT: Duration = Duration::from_secs(MAX_TIMEOUT_S as u64 + 5);
const DRAIN_TIMEOUT: Duration = Duration::from_millis(3000);
const PRECONNECT_CONNECT_TIMEOUT_S: i64 = 6;
const PRECONNECT_TIMEOUT_S: i64 = 10;
const DLOG_NET: c_int = 4;

pub type BlobResolver = fn(url: &[u8]) -> Option<(Arc<[u8]>, Option<Vec<u8>>)>;
pub type Outcome = Result<Response, Vec<u8>>;

type Done = Box<dyn FnOnce(Outcome)>;

thread_local! {
    static CALLBACKS: RefCell<HashMap<u64, ManuallyDrop<Done>>> = RefCell::new(HashMap::new());
}

static NEXT_CALLBACK: AtomicU64 = AtomicU64::new(1);

struct Callback {
    id: u64,
    context: MainContext,
}

impl Callback {
    fn register(done: Done) -> Callback {
        let id = NEXT_CALLBACK.fetch_add(1, Ordering::Relaxed);
        let _ = CALLBACKS.try_with(|callbacks| {
            callbacks.borrow_mut().insert(id, ManuallyDrop::new(done));
        });
        Callback {
            id,
            context: MainContext::thread_default(),
        }
    }

    fn complete(self, outcome: Result<Response, Failure>) {
        let id = self.id;
        let outcome = outcome.map_err(|failure| failure.message().to_vec());
        self.context.defer(move || run_callback(id, outcome));
    }
}

fn run_callback(id: u64, outcome: Outcome) {
    let done = CALLBACKS
        .try_with(|callbacks| callbacks.borrow_mut().remove(&id))
        .ok()
        .flatten();
    if let Some(done) = done {
        ManuallyDrop::into_inner(done)(outcome);
    }
}

#[derive(Clone, Default)]
pub struct Cancellable(Arc<AtomicBool>);

impl Cancellable {
    pub fn new() -> Cancellable {
        Cancellable::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
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
    max_bytes: Option<u64>,
}

struct Pending {
    ctx: FetchCtx,
    cancellable: Option<Cancellable>,
    callback: Callback,
}

impl Shareable for Response {
    fn preloadable_len(&self) -> Option<usize> {
        let forbids_reuse = field(&self.raw_headers).is_some_and(|raw| {
            raw.to_ascii_lowercase()
                .windows(b"no-store".len())
                .any(|w| w == b"no-store")
        });
        (self.status == 200 && self.error.is_none() && !forbids_reuse).then_some(self.body.len())
    }

    fn duplicate(&self) -> Response {
        self.clone()
    }
}

enum Waiter {
    Async(Callback),
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
    queue: VecDeque<Pending>,
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

fn aborting() -> bool {
    ns_net_aborting() != 0
}

fn copy_outcome(outcome: Result<&Response, &Failure>) -> Result<Response, Failure> {
    match outcome {
        Ok(resp) => Ok(resp.clone()),
        Err(failure) => Err(failure.clone()),
    }
}

fn deliver(key: &[u8], outcome: Result<&Response, &Failure>) {
    let mut shared = lock(&SHARED);
    let Some(waiters) = shared.coalescer.deliver(key, outcome.ok()) else {
        return;
    };
    let mut callbacks = Vec::new();
    let mut woke = false;
    for waiter in waiters {
        match waiter {
            Waiter::Async(callback) => callbacks.push(callback),
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
    for callback in callbacks {
        callback.complete(copy_outcome(outcome));
    }
}

fn join_async(key: &[u8], callback: Callback) -> Option<Callback> {
    let mut joiner = Some(callback);
    let claim = lock(&SHARED).coalescer.claim(key, || {
        Waiter::Async(joiner.take().expect("the coalescer calls the joiner once"))
    });
    match (claim, joiner) {
        (Claim::Lead, Some(callback)) => Some(callback),
        (Claim::Preloaded(resp), Some(callback)) => {
            callback.complete(Ok(resp));
            None
        }
        _ => None,
    }
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

fn release_slot(host: Option<&[u8]>) {
    let mut throttle = lock(&THROTTLE);
    throttle.active = throttle.active.saturating_sub(1);
    adjust_host(&mut throttle.per_host, host, false);
    IDLE.notify_all();
}

fn dispatch() {
    loop {
        let pending =
            {
                let mut guard = lock(&THROTTLE);
                let throttle = &mut *guard;
                if throttle.active >= MAX_CONCURRENT_FETCHES || throttle.queue.is_empty() {
                    return;
                }
                let per_host = &throttle.per_host;
                let Some(at) = throttle.queue.iter().position(|pending| {
                    pending.ctx.host.as_ref().is_none_or(|h| {
                        per_host.get(h).copied().unwrap_or(0) < MAX_FETCHES_PER_HOST
                    })
                }) else {
                    return;
                };
                let Some(pending) = throttle.queue.remove(at) else {
                    return;
                };
                throttle.active += 1;
                adjust_host(&mut throttle.per_host, pending.ctx.host.as_deref(), true);
                pending
            };
        let handed = Arc::new(Mutex::new(Some(pending)));
        let taken = Arc::clone(&handed);
        let spawned = thread::Builder::new()
            .name("net-fetch".into())
            .spawn(move || {
                if let Some(pending) = lock(&taken).take() {
                    run_fetch(pending);
                }
            });
        if spawned.is_err()
            && let Some(pending) = lock(&handed).take()
        {
            release_slot(pending.ctx.host.as_deref());
            fail_pending(
                pending,
                Failure::Net(1, b"cannot start a fetch thread".to_vec()),
            );
        }
    }
}

fn submit(pending: Pending) {
    lock(&THROTTLE).queue.push_back(pending);
    dispatch();
}

fn log_fetch(message: Vec<u8>) {
    southstar_debuglog::emit(DLOG_NET, b"fetch", &message);
}

fn log_outcome(url: &[u8], outcome: Result<&Response, &Failure>) {
    match outcome {
        Err(failure) => log_fetch([b"failed ", url, b": ", failure.message()].concat()),
        Ok(resp) => match field(&resp.error) {
            Some(error) => log_fetch([b"error ", url, b": ", error].concat()),
            None => {
                let shown = field(&resp.final_url).unwrap_or(url);
                let len = resp.body.len();
                let head = format!("{} ", resp.status).into_bytes();
                let tail = format!(" ({len} bytes)").into_bytes();
                log_fetch([&head[..], shown, &tail].concat());
            }
        },
    }
}

fn fail_pending(pending: Pending, failure: Failure) {
    if let Some(key) = &pending.ctx.coalesce_key {
        deliver(key, Err(&failure));
    }
    pending.callback.complete(Err(failure));
}

fn run_fetch(pending: Pending) {
    let ctx = &pending.ctx;
    let outcome = fetch::fetch(&Fetch {
        url: &ctx.url,
        top_url: ctx.top_url.as_deref(),
        method: ctx.method.as_deref(),
        body: ctx.body.as_deref(),
        content_type: ctx.content_type.as_deref(),
        headers: &ctx.headers,
        cancellable: pending.cancellable.as_ref(),
        max_bytes: ctx.max_bytes,
    });
    if let Some(key) = &ctx.coalesce_key {
        deliver(key, outcome.as_ref());
    }
    if netlog::logging() {
        log_outcome(&ctx.url, outcome.as_ref());
    }
    let host = ctx.host.clone();
    pending.callback.complete(outcome);
    release_slot(host.as_deref());
    dispatch();
}

fn drain(timeout: Duration) -> bool {
    ns_net_begin_abort();
    let dropped: Vec<Pending> = lock(&THROTTLE).queue.drain(..).collect();
    for pending in dropped {
        fail_pending(pending, Failure::Net(1, b"shutting down".to_vec()));
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

fn blob_response(url: &[u8], requester: Option<&[u8]>) -> Option<Response> {
    let resolve = lock(&BLOB_RESOLVER).filter(|_| url.starts_with(b"blob:"))?;
    let mut resp = Response::new();
    resp.final_url = Some(cstr(url));
    let reachable = requester
        .filter(|requester| !requester.is_empty())
        .is_none_or(|requester| southstar_url::bytes::blob_url_reachable(url, requester));
    match resolve(url).filter(|_| reachable) {
        Some((bytes, content_type)) => {
            resp.body.extend_from_slice(&bytes);
            resp.status = 200;
            resp.content_type = content_type.map(|ct| cstr(&ct));
        }
        None => {
            resp.status = 404;
            resp.error = Some(cstr(b"blob URL not found"));
        }
    }
    Some(resp)
}

fn coalescing_key(req: &FetchRequest<'_>, cancellable: Option<&Cancellable>) -> Option<Vec<u8>> {
    if cancellable.is_some() || req.body.is_some() || req.max_bytes.is_some() {
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
    pub max_bytes: Option<u64>,
}

pub fn request_with(
    req: &FetchRequest<'_>,
    cancellable: Option<&Cancellable>,
    done: impl FnOnce(Outcome) + 'static,
) {
    let callback = Callback::register(Box::new(done));
    if let Some(resp) = blob_response(req.url, req.top_url) {
        callback.complete(Ok(resp));
        return;
    }
    let key = coalescing_key(req, cancellable);
    let callback = match &key {
        Some(key) => match join_async(key, callback) {
            Some(callback) => callback,
            None => return,
        },
        None => callback,
    };
    let ctx = FetchCtx {
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
        max_bytes: req.max_bytes,
    };
    submit(Pending {
        ctx,
        cancellable: cancellable.cloned(),
        callback,
    });
}

pub fn request_blocking(req: &FetchRequest<'_>) -> Result<Response, Failure> {
    blocking(req, None)
}

pub(crate) fn blocking(
    req: &FetchRequest<'_>,
    cancellable: Option<&Cancellable>,
) -> Result<Response, Failure> {
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
        max_bytes: req.max_bytes,
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

fn run_preconnect(url: Vec<u8>) {
    if !aborting() {
        preconnect(&url);
    }
    let mut throttle = lock(&THROTTLE);
    throttle.preconnects = throttle.preconnects.saturating_sub(1);
    IDLE.notify_all();
}

pub fn preconnect_async(url: &CStr) {
    if !url::is_http_or_https(url.to_bytes()) {
        return;
    }
    let url = url.to_bytes().to_vec();
    lock(&THROTTLE).preconnects += 1;
    let spawned = thread::Builder::new()
        .name("net-preconnect".into())
        .spawn(move || run_preconnect(url));
    if spawned.is_err() {
        let mut throttle = lock(&THROTTLE);
        throttle.preconnects = throttle.preconnects.saturating_sub(1);
        IDLE.notify_all();
    }
}

pub fn set_blob_resolver(resolver: BlobResolver) {
    *lock(&BLOB_RESOLVER) = Some(resolver);
}

pub fn fetch_blocking(url: &CStr) -> Option<Response> {
    request_blocking(&FetchRequest {
        url: url.to_bytes(),
        method: Some(b"GET"),
        ..FetchRequest::default()
    })
    .ok()
}

pub fn download_blocking(url: &CStr, top_url: Option<&CStr>, max_bytes: u64) -> Option<Response> {
    request_blocking(&FetchRequest {
        url: url.to_bytes(),
        top_url: top_url.map(CStr::to_bytes),
        method: Some(b"GET"),
        max_bytes: Some(max_bytes),
        ..FetchRequest::default()
    })
    .ok()
}
