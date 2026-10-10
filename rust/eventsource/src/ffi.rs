//! Southstar — the C ABI of EventSource, as declared in src/eventsource.h, the transfer over rust/http and main-loop dispatch.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_uint, c_void};
use std::ffi::CString;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Duration;

use southstar_glib::{self as glib, FALSE, GBoolean};
use southstar_http::{Fetch, Handler};

use crate::{Parser, Post, Shared};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Callbacks {
    on_open: Option<unsafe extern "C" fn(user_data: *mut c_void)>,
    on_message: Option<
        unsafe extern "C" fn(
            event: *const c_char,
            data: *const c_char,
            last_id: *const c_char,
            user_data: *mut c_void,
        ),
    >,
    on_error: Option<unsafe extern "C" fn(fatal: GBoolean, user_data: *mut c_void)>,
    busy: Option<unsafe extern "C" fn(user_data: *mut c_void) -> GBoolean>,
}

struct Source {
    shared: Shared,
    callbacks: Callbacks,
    user_data: *mut c_void,
}

unsafe impl Send for Source {}
unsafe impl Sync for Source {}

pub struct EventSource {
    source: Arc<Source>,
    thread: Option<JoinHandle<()>>,
}

type SourceFn = unsafe extern "C" fn(*mut c_void) -> GBoolean;

unsafe extern "C" {
    fn g_idle_add(function: SourceFn, data: *mut c_void) -> c_uint;
    fn g_timeout_add(interval: c_uint, function: SourceFn, data: *mut c_void) -> c_uint;
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

struct Dispatch {
    source: Arc<Source>,
    post: Post,
}

unsafe extern "C" fn dispatch_run(data: *mut c_void) -> GBoolean {
    let dispatch = data.cast::<Dispatch>();
    let source = unsafe { &(*dispatch).source };
    let detached = source.shared.detached.load(Ordering::SeqCst);
    let user_data = source.user_data;
    if !detached
        && let Some(busy) = source.callbacks.busy
        && unsafe { busy(user_data) } != FALSE
    {
        unsafe { g_timeout_add(4, dispatch_run, data) };
        return FALSE;
    }
    let dispatch = unsafe { Box::from_raw(dispatch) };
    if !detached {
        let callbacks = dispatch.source.callbacks;
        match &dispatch.post {
            Post::Open => {
                if let Some(on_open) = callbacks.on_open {
                    unsafe { on_open(user_data) };
                }
            }
            Post::Message(message) => {
                if let Some(on_message) = callbacks.on_message {
                    let (event, body, last_id) = (
                        cstring(&message.event),
                        cstring(&message.data),
                        cstring(&message.last_id),
                    );
                    unsafe {
                        on_message(event.as_ptr(), body.as_ptr(), last_id.as_ptr(), user_data)
                    };
                }
            }
            Post::Error { fatal } => {
                if let Some(on_error) = callbacks.on_error {
                    unsafe { on_error(glib::boolean(*fatal), user_data) };
                }
            }
        }
    }
    dispatch
        .source
        .shared
        .pending
        .fetch_sub(1, Ordering::SeqCst);
    FALSE
}

fn post(source: &Arc<Source>, post: Post) {
    if !source.shared.try_reserve_post() {
        return;
    }
    let dispatch = Box::new(Dispatch {
        source: Arc::clone(source),
        post,
    });
    unsafe { g_idle_add(dispatch_run, Box::into_raw(dispatch).cast()) };
}

struct Transfer {
    source: Arc<Source>,
    parser: Parser,
}

impl Transfer {
    fn post_header(&mut self, line: &[u8]) {
        let source = Arc::clone(&self.source);
        self.parser
            .header(line, &mut |message| post(&source, message));
    }
}

impl Handler for Transfer {
    fn should_abort(&self) -> bool {
        self.source.shared.exiting()
    }

    fn status_line(&mut self, line: &[u8]) {
        self.post_header(line);
    }

    fn header(&mut self, line: &[u8], _name: &[u8], _value: &[u8]) {
        self.post_header(line);
    }

    fn headers_done(&mut self) {
        self.post_header(
            b"
",
        );
    }

    fn body(&mut self, data: &[u8]) -> bool {
        if self.source.shared.exiting() || self.parser.fatal {
            return false;
        }
        let source = Arc::clone(&self.source);
        self.parser
            .feed(data, &source.shared, &mut |message| post(&source, message));
        true
    }
}

fn connect_once(source: &Arc<Source>) -> bool {
    let mut transfer = Transfer {
        source: Arc::clone(source),
        parser: Parser::new(),
    };
    let shared = &source.shared;
    let mut headers = vec![
        b"Accept: text/event-stream".to_vec(),
        b"Cache-Control: no-cache".to_vec(),
    ];
    if let Some(origin) = shared.origin.as_deref().filter(|o| !o.is_empty()) {
        headers.push([b"Origin: ".as_slice(), until_nul(origin)].concat());
    }
    let last_id = shared.last_event_id();
    if !last_id.is_empty() {
        headers.push([b"Last-Event-ID: ".as_slice(), until_nul(&last_id)].concat());
    }
    let request = Fetch {
        url: until_nul(&shared.url).to_vec(),
        method: b"GET",
        body: b"",
        user_agent: Some(southstar_net::route::user_agent()),
        headers,
        timeout: Duration::MAX,
        connect_timeout: Duration::from_secs(30),
        max_redirects: 50,
        https_only_redirects: shared.url.starts_with(b"https://"),
        route: &southstar_net::route::route,
    };
    southstar_http::fetch(&request, &mut transfer);
    !transfer.parser.fatal
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}

fn worker(source: Arc<Source>) {
    let shared = &source.shared;
    while !shared.exiting() {
        let retriable = connect_once(&source);
        if shared.exiting() {
            break;
        }
        if !retriable {
            post(&source, Post::Error { fatal: true });
            break;
        }
        post(&source, Post::Error { fatal: false });
        let mut wait_ms = shared.reconnect_ms();
        if wait_ms <= 0 {
            wait_ms = 3000;
        }
        let mut waited = 0;
        while waited < wait_ms && !shared.exiting() {
            std::thread::sleep(Duration::from_millis(50));
            waited += 50;
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_es_new(
    url: *const c_char,
    origin: *const c_char,
    last_event_id: *const c_char,
    cbs: *const Callbacks,
    user_data: *mut c_void,
) -> *mut EventSource {
    let (url, origin, last_event_id) = unsafe {
        (
            glib::bytes(url).unwrap_or_default().to_vec(),
            glib::bytes(origin).map(<[u8]>::to_vec),
            glib::bytes(last_event_id).unwrap_or_default().to_vec(),
        )
    };
    let callbacks = unsafe { cbs.as_ref() }.copied().unwrap_or(Callbacks {
        on_open: None,
        on_message: None,
        on_error: None,
        busy: None,
    });
    let source = Arc::new(Source {
        shared: Shared::new(url, origin, last_event_id),
        callbacks,
        user_data,
    });
    let worker_source = Arc::clone(&source);
    let thread = std::thread::Builder::new()
        .name("nd-eventsource".into())
        .spawn(move || worker(worker_source))
        .ok();
    Box::into_raw(Box::new(EventSource { source, thread }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_es_close(es: *mut EventSource) {
    if let Some(es) = unsafe { es.as_ref() } {
        es.source
            .shared
            .exit_requested
            .store(true, Ordering::SeqCst);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_es_free(es: *mut EventSource) {
    if es.is_null() {
        return;
    }
    let mut es = unsafe { Box::from_raw(es) };
    es.source.shared.detached.store(true, Ordering::SeqCst);
    es.source
        .shared
        .exit_requested
        .store(true, Ordering::SeqCst);
    if let Some(thread) = es.thread.take() {
        let _ = thread.join();
    }
}
