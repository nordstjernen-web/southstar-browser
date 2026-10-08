//! Southstar — the C ABI of WebSocket, as declared in src/ws.h, the libcurl connection and main-loop dispatch.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Duration;

use southstar_glib::{self as glib, FALSE, GBoolean};

use crate::{
    Assembly, Frame, FrameMeta, Out, Post, STATE_CLOSED, STATE_CLOSING, STATE_OPEN, Shared,
    WS_BINARY, WS_TEXT,
};

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Callbacks {
    on_open: Option<unsafe extern "C" fn(*mut c_void)>,
    on_text: Option<unsafe extern "C" fn(*const c_char, usize, *mut c_void)>,
    on_binary: Option<unsafe extern "C" fn(*const u8, usize, *mut c_void)>,
    on_close: Option<unsafe extern "C" fn(c_int, *const c_char, GBoolean, *mut c_void)>,
    on_error: Option<unsafe extern "C" fn(*const c_char, *mut c_void)>,
    busy: Option<unsafe extern "C" fn(*mut c_void) -> GBoolean>,
}

struct Socket {
    shared: Shared,
    callbacks: Callbacks,
    user_data: *mut c_void,
}

unsafe impl Send for Socket {}
unsafe impl Sync for Socket {}

pub struct WebSocket {
    socket: Arc<Socket>,
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

#[repr(C)]
struct WsFrame {
    age: c_int,
    flags: c_int,
    offset: i64,
    bytesleft: i64,
    len: usize,
}

#[repr(C)]
struct VersionInfo {
    age: c_int,
    version: *const c_char,
    version_num: c_uint,
    host: *const c_char,
    features: c_int,
    ssl_version: *const c_char,
    ssl_version_num: c_long,
    libz_version: *const c_char,
    protocols: *const *const c_char,
}

type XferInfoFn = unsafe extern "C" fn(*mut c_void, i64, i64, i64, i64) -> c_int;
type SourceFn = unsafe extern "C" fn(*mut c_void) -> GBoolean;

const CURLE_OK: c_int = 0;
const CURLE_AGAIN: c_int = 81;
const CURLOPT_URL: c_int = 10002;
const CURLOPT_ERRORBUFFER: c_int = 10010;
const CURLOPT_TIMEOUT: c_int = 13;
const CURLOPT_USERAGENT: c_int = 10018;
const CURLOPT_HTTPHEADER: c_int = 10023;
const CURLOPT_NOPROGRESS: c_int = 43;
const CURLOPT_XFERINFODATA: c_int = 10057;
const CURLOPT_CONNECTTIMEOUT: c_int = 78;
const CURLOPT_HTTP_VERSION: c_int = 84;
const CURLOPT_NOSIGNAL: c_int = 99;
const CURLOPT_CONNECT_ONLY: c_int = 141;
const CURLOPT_XFERINFOFUNCTION: c_int = 20219;
const CURLINFO_RESPONSE_CODE: c_int = 0x0020_0000 + 2;
const CURL_HTTP_VERSION_1_1: c_long = 2;
const CURL_ERROR_SIZE: usize = 256;
const CURLVERSION_FOURTH: c_int = 3;
const CURLWS_CLOSE: c_uint = 1 << 3;
const CURLWS_PONG: c_uint = 1 << 6;

unsafe extern "C" {
    fn curl_easy_init() -> *mut Curl;
    fn curl_easy_setopt(curl: *mut Curl, option: c_int, ...) -> c_int;
    fn curl_easy_getinfo(curl: *mut Curl, info: c_int, ...) -> c_int;
    fn curl_easy_perform(curl: *mut Curl) -> c_int;
    fn curl_easy_cleanup(curl: *mut Curl);
    fn curl_easy_strerror(code: c_int) -> *const c_char;
    fn curl_slist_append(list: *mut CurlSlist, text: *const c_char) -> *mut CurlSlist;
    fn curl_slist_free_all(list: *mut CurlSlist);
    fn curl_version_info(stamp: c_int) -> *const VersionInfo;
    fn curl_ws_recv(
        curl: *mut Curl,
        buffer: *mut c_void,
        buflen: usize,
        recv: *mut usize,
        meta: *mut *const WsFrame,
    ) -> c_int;
    fn curl_ws_send(
        curl: *mut Curl,
        buffer: *const c_void,
        buflen: usize,
        sent: *mut usize,
        fragsize: i64,
        flags: c_uint,
    ) -> c_int;
    fn ns_net_apply_curl_tls(curl: *mut c_void);
    fn ns_net_apply_curl_proxy(curl: *mut c_void, url: *const c_char);
    fn ns_user_agent_for_mode(compat_mode: *const c_char) -> *const c_char;
    fn g_idle_add(function: SourceFn, data: *mut c_void) -> c_uint;
    fn g_timeout_add(interval: c_uint, function: SourceFn, data: *mut c_void) -> c_uint;
}

fn cstring(bytes: &[u8]) -> CString {
    CString::new(crate::until_nul(bytes)).unwrap_or_default()
}

struct Dispatch {
    socket: Arc<Socket>,
    post: Post,
}

unsafe extern "C" fn dispatch_run(data: *mut c_void) -> GBoolean {
    let dispatch = data.cast::<Dispatch>();
    let socket = unsafe { &(*dispatch).socket };
    let detached = socket.shared.detached.load(Ordering::SeqCst);
    let user_data = socket.user_data;
    if !detached {
        if let Some(busy) = socket.callbacks.busy {
            if unsafe { busy(user_data) } != FALSE {
                unsafe { g_timeout_add(4, dispatch_run, data) };
                return FALSE;
            }
        }
    }
    let dispatch = unsafe { Box::from_raw(dispatch) };
    if !detached {
        let callbacks = dispatch.socket.callbacks;
        match &dispatch.post {
            Post::Open => {
                if let Some(on_open) = callbacks.on_open {
                    unsafe { on_open(user_data) };
                }
            }
            Post::Message { text: true, data } => {
                if let Some(on_text) = callbacks.on_text {
                    let text = if data.is_empty() {
                        c"".as_ptr()
                    } else {
                        data.as_ptr().cast()
                    };
                    unsafe { on_text(text, data.len(), user_data) };
                }
            }
            Post::Message { text: false, data } => {
                if let Some(on_binary) = callbacks.on_binary {
                    let bytes = if data.is_empty() {
                        c"".as_ptr().cast()
                    } else {
                        data.as_ptr()
                    };
                    unsafe { on_binary(bytes, data.len(), user_data) };
                }
            }
            Post::Close {
                code,
                reason,
                clean,
            } => {
                if let Some(on_close) = callbacks.on_close {
                    let reason = cstring(reason);
                    unsafe { on_close(*code, reason.as_ptr(), glib::boolean(*clean), user_data) };
                }
            }
            Post::Error(message) => {
                if let Some(on_error) = callbacks.on_error {
                    let message = cstring(message);
                    unsafe { on_error(message.as_ptr(), user_data) };
                }
            }
        }
    }
    dispatch
        .socket
        .shared
        .pending
        .fetch_sub(1, Ordering::SeqCst);
    FALSE
}

fn post(socket: &Arc<Socket>, post: Post) {
    let droppable = matches!(post, Post::Message { .. });
    if !socket.shared.try_reserve_post(droppable) {
        return;
    }
    let dispatch = Box::new(Dispatch {
        socket: Arc::clone(socket),
        post,
    });
    unsafe { g_idle_add(dispatch_run, Box::into_raw(dispatch).cast()) };
}

fn post_close(socket: &Arc<Socket>, code: i32, reason: &[u8], clean: bool) {
    socket.shared.set_state(STATE_CLOSED);
    post(
        socket,
        Post::Close {
            code,
            reason: reason.to_vec(),
            clean,
        },
    );
}

struct Connection {
    curl: *mut Curl,
    headers: *mut CurlSlist,
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            if !self.headers.is_null() {
                curl_slist_free_all(self.headers);
            }
            curl_easy_cleanup(self.curl);
        }
    }
}

impl Connection {
    fn send(&self, shared: &Shared, data: &[u8], flags: c_uint) -> bool {
        let mut off = 0;
        let mut stalls = 0;
        loop {
            if shared.exiting() {
                return false;
            }
            let remain = &data[off..];
            let buffer: *const c_void = if remain.is_empty() {
                c"".as_ptr().cast()
            } else {
                remain.as_ptr().cast()
            };
            let mut sent = 0;
            let rc = unsafe { curl_ws_send(self.curl, buffer, remain.len(), &mut sent, 0, flags) };
            if rc == CURLE_AGAIN {
                stalls += 1;
                if stalls > 5000 {
                    return false;
                }
                std::thread::sleep(Duration::from_micros(2000));
                continue;
            }
            if rc != CURLE_OK {
                return false;
            }
            off += sent;
            if off >= data.len() {
                return true;
            }
            if sent == 0 {
                stalls += 1;
                if stalls > 5000 {
                    return false;
                }
                std::thread::sleep(Duration::from_micros(2000));
            } else {
                stalls = 0;
            }
        }
    }

    fn send_close(&self, code: i32, reason: Option<&[u8]>) {
        let frame = crate::close_frame(code, reason);
        let buffer: *const c_void = if frame.is_empty() {
            c"".as_ptr().cast()
        } else {
            frame.as_ptr().cast()
        };
        let mut sent = 0;
        unsafe { curl_ws_send(self.curl, buffer, frame.len(), &mut sent, 0, CURLWS_CLOSE) };
    }

    fn send_pong(&self, payload: &[u8]) {
        let buffer: *const c_void = if payload.is_empty() {
            ptr::null()
        } else {
            payload.as_ptr().cast()
        };
        let mut sent = 0;
        unsafe { curl_ws_send(self.curl, buffer, payload.len(), &mut sent, 0, CURLWS_PONG) };
    }
}

unsafe extern "C" fn handshake_progress(
    clientp: *mut c_void,
    _: i64,
    _: i64,
    _: i64,
    _: i64,
) -> c_int {
    let socket = unsafe { &*clientp.cast::<Socket>() };
    c_int::from(socket.shared.exiting())
}

fn error_text(errbuf: &[c_char; CURL_ERROR_SIZE], rc: c_int) -> Vec<u8> {
    if errbuf[0] != 0 {
        unsafe { CStr::from_ptr(errbuf.as_ptr()) }
            .to_bytes()
            .to_vec()
    } else {
        unsafe { CStr::from_ptr(curl_easy_strerror(rc)) }
            .to_bytes()
            .to_vec()
    }
}

fn worker(socket: Arc<Socket>) {
    let shared = &socket.shared;
    let curl = unsafe { curl_easy_init() };
    if curl.is_null() {
        post(&socket, Post::Error(b"curl init failed".to_vec()));
        post_close(&socket, 1006, b"init failed", false);
        return;
    }
    let mut connection = Connection {
        curl,
        headers: ptr::null_mut(),
    };
    let url = cstring(&shared.url);
    let mut add_header = |text: &[u8]| {
        let text = cstring(text);
        connection.headers = unsafe { curl_slist_append(connection.headers, text.as_ptr()) };
    };
    if let Some(origin) = shared.origin.as_deref().filter(|o| !o.is_empty()) {
        add_header(&[b"Origin: ".as_slice(), origin].concat());
    }
    if !shared.protocols.is_empty() {
        add_header(
            &[
                b"Sec-WebSocket-Protocol: ".as_slice(),
                &shared.protocols.join(&b", "[..]),
            ]
            .concat(),
        );
    }
    let mut errbuf = [0 as c_char; CURL_ERROR_SIZE];
    let socket_ptr = Arc::as_ptr(&socket).cast_mut().cast::<c_void>();
    let rc = unsafe {
        curl_easy_setopt(curl, CURLOPT_URL, url.as_ptr());
        curl_easy_setopt(curl, CURLOPT_CONNECT_ONLY, 2 as c_long);
        curl_easy_setopt(curl, CURLOPT_HTTP_VERSION, CURL_HTTP_VERSION_1_1);
        curl_easy_setopt(curl, CURLOPT_USERAGENT, ns_user_agent_for_mode(ptr::null()));
        curl_easy_setopt(curl, CURLOPT_NOSIGNAL, 1 as c_long);
        ns_net_apply_curl_tls(curl.cast());
        ns_net_apply_curl_proxy(curl.cast(), url.as_ptr());
        curl_easy_setopt(curl, CURLOPT_CONNECTTIMEOUT, 15 as c_long);
        curl_easy_setopt(curl, CURLOPT_TIMEOUT, 0 as c_long);
        curl_easy_setopt(curl, CURLOPT_NOPROGRESS, 0 as c_long);
        curl_easy_setopt(
            curl,
            CURLOPT_XFERINFOFUNCTION,
            handshake_progress as XferInfoFn,
        );
        curl_easy_setopt(curl, CURLOPT_XFERINFODATA, socket_ptr);
        if !connection.headers.is_null() {
            curl_easy_setopt(curl, CURLOPT_HTTPHEADER, connection.headers);
        }
        curl_easy_setopt(curl, CURLOPT_ERRORBUFFER, errbuf.as_mut_ptr());
        curl_easy_perform(curl)
    };
    if rc != CURLE_OK || shared.exiting() {
        let message = if rc != CURLE_OK {
            error_text(&errbuf, rc)
        } else {
            b"aborted".to_vec()
        };
        if rc != CURLE_OK {
            post(&socket, Post::Error(message.clone()));
        }
        post_close(&socket, 1006, &message, false);
        return;
    }
    let mut code: c_long = 0;
    unsafe { curl_easy_getinfo(curl, CURLINFO_RESPONSE_CODE, &mut code as *mut c_long) };
    if code != 0 && code != 101 {
        let message = format!("WebSocket handshake failed (HTTP {code})").into_bytes();
        post(&socket, Post::Error(message.clone()));
        post_close(&socket, 1006, &message, false);
        return;
    }
    shared.set_state(STATE_OPEN);
    post(&socket, Post::Open);

    let mut clean_close = false;
    let mut close_code = 1006;
    let mut close_reason: Option<Vec<u8>> = None;
    let mut peer_reason: Option<Vec<u8>> = None;
    let mut assembly = Assembly::default();
    let mut buf = vec![0u8; 8192];

    'outer: while !shared.exiting() {
        while let Some(out) = shared.pop() {
            match out {
                Out::Text(data) => {
                    connection.send(shared, &data, WS_TEXT as c_uint);
                }
                Out::Binary(data) => {
                    connection.send(shared, &data, WS_BINARY as c_uint);
                }
                Out::Close { code, reason } => {
                    close_code = code;
                    close_reason = reason.map(|r| crate::until_nul(&r).to_vec());
                    shared.set_state(STATE_CLOSING);
                    connection.send_close(close_code, close_reason.as_deref());
                    clean_close = true;
                    break 'outer;
                }
            }
        }
        let mut got = 0;
        let mut meta: *const WsFrame = ptr::null();
        let rc = unsafe {
            curl_ws_recv(
                curl,
                buf.as_mut_ptr().cast(),
                buf.len(),
                &mut got,
                &mut meta,
            )
        };
        if rc == CURLE_AGAIN {
            shared.wait();
            continue;
        }
        if rc != CURLE_OK {
            let message = error_text(&errbuf, rc);
            post(&socket, Post::Error(message.clone()));
            close_code = 1006;
            close_reason = Some(message);
            break;
        }
        let Some(meta) = (unsafe { meta.as_ref() }) else {
            continue;
        };
        let data = &buf[..got];
        let frame = assembly.frame(
            data,
            &FrameMeta {
                flags: meta.flags,
                offset: meta.offset,
                bytes_left: meta.bytesleft,
            },
        );
        match frame {
            Frame::Close { code, reason } => {
                shared.set_state(STATE_CLOSING);
                connection.send_close(crate::echo_close_code(code), None);
                clean_close = true;
                close_code = code;
                close_reason = reason.clone();
                peer_reason = reason;
                break;
            }
            Frame::Ping(len) => connection.send_pong(&data[..len]),
            Frame::Message { text, data } => post(&socket, Post::Message { text, data }),
            Frame::TooBig => {
                shared.set_state(STATE_CLOSING);
                connection.send_close(1009, Some(b"message too big"));
                clean_close = false;
                close_code = 1009;
                close_reason = Some(b"message too big".to_vec());
                break;
            }
            Frame::BadUtf8 => {
                shared.set_state(STATE_CLOSING);
                connection.send_close(1007, Some(b"invalid utf-8"));
                clean_close = false;
                close_code = 1007;
                close_reason = Some(b"invalid utf-8".to_vec());
                break;
            }
            Frame::Ignored | Frame::Assembled => {}
        }
    }
    let reason = close_reason.or(peer_reason).unwrap_or_default();
    post_close(&socket, close_code, &reason, clean_close);
}

fn curl_native() -> bool {
    let info = unsafe { curl_version_info(CURLVERSION_FOURTH) };
    let Some(info) = (unsafe { info.as_ref() }) else {
        return false;
    };
    if info.protocols.is_null() {
        return false;
    }
    let mut i = 0;
    loop {
        let protocol = unsafe { *info.protocols.add(i) };
        if protocol.is_null() {
            return false;
        }
        let name = unsafe { CStr::from_ptr(protocol) }.to_bytes();
        if name.eq_ignore_ascii_case(b"ws") || name.eq_ignore_ascii_case(b"wss") {
            return true;
        }
        i += 1;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_ws_available() -> GBoolean {
    glib::boolean(curl_native())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ws_new(
    url: *const c_char,
    origin: *const c_char,
    protocols: *const *const c_char,
    cbs: *const Callbacks,
    user_data: *mut c_void,
) -> *mut WebSocket {
    let Some(url) = (unsafe { glib::bytes(url) }) else {
        return ptr::null_mut();
    };
    if !curl_native() {
        return ptr::null_mut();
    }
    let mut list = Vec::new();
    if !protocols.is_null() {
        let mut i = 0;
        loop {
            let protocol = unsafe { *protocols.add(i) };
            if protocol.is_null() {
                break;
            }
            list.push(unsafe { CStr::from_ptr(protocol) }.to_bytes().to_vec());
            i += 1;
        }
    }
    let callbacks = unsafe { cbs.as_ref() }.copied().unwrap_or(Callbacks {
        on_open: None,
        on_text: None,
        on_binary: None,
        on_close: None,
        on_error: None,
        busy: None,
    });
    let origin = unsafe { glib::bytes(origin) }.map(<[u8]>::to_vec);
    let socket = Arc::new(Socket {
        shared: Shared::new(url.to_vec(), origin, list),
        callbacks,
        user_data,
    });
    let worker_socket = Arc::clone(&socket);
    let thread = std::thread::Builder::new()
        .name("nd-ws".into())
        .spawn(move || worker(worker_socket))
        .ok();
    Box::into_raw(Box::new(WebSocket { socket, thread }))
}

unsafe fn shared<'a>(ws: *mut WebSocket) -> Option<&'a Shared> {
    unsafe { ws.as_ref() }.map(|ws| &ws.socket.shared)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ws_send_text(
    ws: *mut WebSocket,
    text: *const c_char,
    len: usize,
) -> GBoolean {
    let Some(shared) = (unsafe { shared(ws) }) else {
        return FALSE;
    };
    let data = unsafe { glib::slice(text.cast(), len) }.to_vec();
    glib::boolean(shared.enqueue(Out::Text(data)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ws_send_binary(
    ws: *mut WebSocket,
    data: *const u8,
    len: usize,
) -> GBoolean {
    let Some(shared) = (unsafe { shared(ws) }) else {
        return FALSE;
    };
    let data = unsafe { glib::slice(data, len) }.to_vec();
    glib::boolean(shared.enqueue(Out::Binary(data)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ws_close(ws: *mut WebSocket, code: c_int, reason: *const c_char) {
    if let Some(shared) = unsafe { shared(ws) } {
        shared.close(code, unsafe { glib::bytes(reason) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ws_state_get(ws: *mut WebSocket) -> c_int {
    unsafe { shared(ws) }.map_or(STATE_CLOSED, Shared::state)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ws_free(ws: *mut WebSocket) {
    if ws.is_null() {
        return;
    }
    let mut ws = unsafe { Box::from_raw(ws) };
    ws.socket.shared.request_exit();
    if let Some(thread) = ws.thread.take() {
        let _ = thread.join();
    }
}
