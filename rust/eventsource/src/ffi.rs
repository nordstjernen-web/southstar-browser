//! Southstar — the C ABI of EventSource, as declared in src/eventsource.h, the libcurl transfer and main-loop dispatch.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Duration;

use southstar_glib::{self as glib, FALSE, GBoolean};

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

#[repr(C)]
struct Curl {
    _private: [u8; 0],
}

#[repr(C)]
struct CurlSlist {
    _private: [u8; 0],
}

type WriteFn = unsafe extern "C" fn(*mut c_char, usize, usize, *mut c_void) -> usize;
type XferInfoFn = unsafe extern "C" fn(*mut c_void, i64, i64, i64, i64) -> c_int;
type SourceFn = unsafe extern "C" fn(*mut c_void) -> GBoolean;

const CURLOPT_WRITEDATA: c_int = 10001;
const CURLOPT_URL: c_int = 10002;
const CURLOPT_WRITEFUNCTION: c_int = 20011;
const CURLOPT_TIMEOUT: c_int = 13;
const CURLOPT_USERAGENT: c_int = 10018;
const CURLOPT_HTTPHEADER: c_int = 10023;
const CURLOPT_HEADERDATA: c_int = 10029;
const CURLOPT_NOPROGRESS: c_int = 43;
const CURLOPT_FOLLOWLOCATION: c_int = 52;
const CURLOPT_XFERINFODATA: c_int = 10057;
const CURLOPT_CONNECTTIMEOUT: c_int = 78;
const CURLOPT_HEADERFUNCTION: c_int = 20079;
const CURLOPT_NOSIGNAL: c_int = 99;
const CURLOPT_ACCEPT_ENCODING: c_int = 10102;
const CURLOPT_XFERINFOFUNCTION: c_int = 20219;
const CURLOPT_PROTOCOLS_STR: c_int = 10318;
const CURLOPT_REDIR_PROTOCOLS_STR: c_int = 10319;

unsafe extern "C" {
    fn curl_easy_init() -> *mut Curl;
    fn curl_easy_setopt(curl: *mut Curl, option: c_int, ...) -> c_int;
    fn curl_easy_perform(curl: *mut Curl) -> c_int;
    fn curl_easy_cleanup(curl: *mut Curl);
    fn curl_slist_append(list: *mut CurlSlist, text: *const c_char) -> *mut CurlSlist;
    fn curl_slist_free_all(list: *mut CurlSlist);
    fn ns_net_apply_curl_tls(curl: *mut c_void);
    fn ns_net_apply_curl_proxy(curl: *mut c_void, url: *const c_char);
    fn ns_user_agent_for_mode(compat_mode: *const c_char) -> *const c_char;
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
    if !detached {
        if let Some(busy) = source.callbacks.busy {
            if unsafe { busy(user_data) } != FALSE {
                unsafe { g_timeout_add(4, dispatch_run, data) };
                return FALSE;
            }
        }
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

unsafe fn chunk<'a>(ptr: *const c_char, size: usize, nitems: usize) -> Option<&'a [u8]> {
    if size != 0 && nitems > usize::MAX / size {
        return None;
    }
    Some(unsafe { glib::slice(ptr.cast(), size * nitems) })
}

unsafe extern "C" fn header_cb(
    buffer: *mut c_char,
    size: usize,
    nitems: usize,
    userdata: *mut c_void,
) -> usize {
    let transfer = unsafe { &mut *userdata.cast::<Transfer>() };
    let Some(header) = (unsafe { chunk(buffer, size, nitems) }) else {
        return 0;
    };
    let source = Arc::clone(&transfer.source);
    transfer
        .parser
        .header(header, &mut |message| post(&source, message));
    if transfer.source.shared.exiting() {
        0
    } else {
        header.len()
    }
}

unsafe extern "C" fn write_cb(
    ptr: *mut c_char,
    size: usize,
    nmemb: usize,
    userdata: *mut c_void,
) -> usize {
    let transfer = unsafe { &mut *userdata.cast::<Transfer>() };
    if transfer.source.shared.exiting() || transfer.parser.fatal {
        return 0;
    }
    let Some(data) = (unsafe { chunk(ptr, size, nmemb) }) else {
        return 0;
    };
    let source = Arc::clone(&transfer.source);
    transfer
        .parser
        .feed(data, &source.shared, &mut |message| post(&source, message));
    data.len()
}

unsafe extern "C" fn progress_cb(clientp: *mut c_void, _: i64, _: i64, _: i64, _: i64) -> c_int {
    let source = unsafe { &*clientp.cast::<Source>() };
    c_int::from(source.shared.exiting())
}

fn connect_once(source: &Arc<Source>) -> bool {
    let curl = unsafe { curl_easy_init() };
    if curl.is_null() {
        return false;
    }
    let mut transfer = Transfer {
        source: Arc::clone(source),
        parser: Parser::new(),
    };
    let shared = &source.shared;
    let url = cstring(&shared.url);
    let redirect_protocols = if shared.url.starts_with(b"https://") {
        c"https"
    } else {
        c"http,https"
    };
    let mut headers: *mut CurlSlist = ptr::null_mut();
    let mut add_header = |text: &[u8]| {
        let text = cstring(text);
        headers = unsafe { curl_slist_append(headers, text.as_ptr()) };
    };
    add_header(b"Accept: text/event-stream");
    add_header(b"Cache-Control: no-cache");
    if let Some(origin) = shared.origin.as_deref().filter(|o| !o.is_empty()) {
        add_header(&[b"Origin: ".as_slice(), origin].concat());
    }
    let last_id = shared.last_event_id();
    if !last_id.is_empty() {
        add_header(&[b"Last-Event-ID: ".as_slice(), &last_id].concat());
    }
    let userdata = (&raw mut transfer).cast::<c_void>();
    let source_ptr = Arc::as_ptr(source).cast_mut().cast::<c_void>();
    unsafe {
        curl_easy_setopt(curl, CURLOPT_URL, url.as_ptr());
        curl_easy_setopt(curl, CURLOPT_USERAGENT, ns_user_agent_for_mode(ptr::null()));
        curl_easy_setopt(curl, CURLOPT_NOSIGNAL, 1 as c_long);
        ns_net_apply_curl_tls(curl.cast());
        ns_net_apply_curl_proxy(curl.cast(), url.as_ptr());
        curl_easy_setopt(curl, CURLOPT_FOLLOWLOCATION, 1 as c_long);
        curl_easy_setopt(curl, CURLOPT_PROTOCOLS_STR, c"http,https".as_ptr());
        curl_easy_setopt(
            curl,
            CURLOPT_REDIR_PROTOCOLS_STR,
            redirect_protocols.as_ptr(),
        );
        curl_easy_setopt(curl, CURLOPT_CONNECTTIMEOUT, 30 as c_long);
        curl_easy_setopt(curl, CURLOPT_TIMEOUT, 0 as c_long);
        curl_easy_setopt(curl, CURLOPT_ACCEPT_ENCODING, c"".as_ptr());
        curl_easy_setopt(curl, CURLOPT_WRITEFUNCTION, write_cb as WriteFn);
        curl_easy_setopt(curl, CURLOPT_WRITEDATA, userdata);
        curl_easy_setopt(curl, CURLOPT_HEADERFUNCTION, header_cb as WriteFn);
        curl_easy_setopt(curl, CURLOPT_HEADERDATA, userdata);
        curl_easy_setopt(curl, CURLOPT_NOPROGRESS, 0 as c_long);
        curl_easy_setopt(curl, CURLOPT_XFERINFOFUNCTION, progress_cb as XferInfoFn);
        curl_easy_setopt(curl, CURLOPT_XFERINFODATA, source_ptr);
        curl_easy_setopt(curl, CURLOPT_HTTPHEADER, headers);
        curl_easy_perform(curl);
        curl_slist_free_all(headers);
        curl_easy_cleanup(curl);
    }
    !transfer.parser.fatal
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
