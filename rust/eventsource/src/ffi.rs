//! Southstar — the EventSource connection: the transfer over rust/http and main-loop dispatch.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_uint, c_void};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Duration;

use southstar_glib::{FALSE, GBoolean};
use southstar_http::{Fetch, Handler};

use crate::{Parser, Post, Shared};

#[derive(Clone, Copy)]
pub struct Events {
    pub on_open: fn(data: usize),
    pub on_message: fn(event: &[u8], text: &[u8], last_id: &[u8], data: usize),
    pub on_error: fn(fatal: bool, data: usize),
    pub busy: fn(data: usize) -> bool,
}

struct Source {
    shared: Shared,
    events: Events,
    data: usize,
}

pub struct EventSource {
    source: Arc<Source>,
    thread: Option<JoinHandle<()>>,
}

type SourceFn = unsafe extern "C" fn(*mut c_void) -> GBoolean;

unsafe extern "C" {
    fn g_idle_add(function: SourceFn, data: *mut c_void) -> c_uint;
    fn g_timeout_add(interval: c_uint, function: SourceFn, data: *mut c_void) -> c_uint;
}

struct Dispatch {
    source: Arc<Source>,
    post: Post,
}

unsafe extern "C" fn dispatch_run(data: *mut c_void) -> GBoolean {
    let dispatch = data.cast::<Dispatch>();
    let source = unsafe { &(*dispatch).source };
    let detached = source.shared.detached.load(Ordering::SeqCst);
    let data_arg = source.data;
    if !detached && (source.events.busy)(data_arg) {
        unsafe { g_timeout_add(4, dispatch_run, data) };
        return FALSE;
    }
    let dispatch = unsafe { Box::from_raw(dispatch) };
    if !detached {
        let events = dispatch.source.events;
        match &dispatch.post {
            Post::Open => (events.on_open)(data_arg),
            Post::Message(message) => (events.on_message)(
                until_nul(&message.event),
                until_nul(&message.data),
                until_nul(&message.last_id),
                data_arg,
            ),
            Post::Error { fatal } => (events.on_error)(*fatal, data_arg),
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

impl EventSource {
    pub fn open(
        url: &[u8],
        origin: Option<&[u8]>,
        last_event_id: &[u8],
        events: Events,
        data: usize,
    ) -> EventSource {
        let source = Arc::new(Source {
            shared: Shared::new(
                url.to_vec(),
                origin.map(<[u8]>::to_vec),
                last_event_id.to_vec(),
            ),
            events,
            data,
        });
        let worker_source = Arc::clone(&source);
        let thread = std::thread::Builder::new()
            .name("nd-eventsource".into())
            .spawn(move || worker(worker_source))
            .ok();
        EventSource { source, thread }
    }

    pub fn close(&self) {
        self.source
            .shared
            .exit_requested
            .store(true, Ordering::SeqCst);
    }

    pub fn free(mut self) {
        self.source.shared.detached.store(true, Ordering::SeqCst);
        self.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
