//! Southstar — the C ABI of WebSocket, as declared in src/ws.h, the connection over rust/http's HTTP/1.1 upgrade and main-loop dispatch.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Duration;

use southstar_glib::{self as glib, FALSE, GBoolean};

use southstar_http::{Received, Request, Upgraded};

use crate::{
    ACCEPT_GUID, Assembly, Decoded, Decoder, Frame, OP_BINARY, OP_CLOSE, OP_PONG, OP_TEXT, Out,
    Post, STATE_CLOSED, STATE_CLOSING, STATE_OPEN, Shared, base64, encode_frame, has_token, header,
    ws_to_http,
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

type SourceFn = unsafe extern "C" fn(*mut c_void) -> GBoolean;

const POLL_INTERVAL: Duration = Duration::from_millis(10);

unsafe extern "C" {
    fn SHA1(data: *const u8, len: usize, digest: *mut u8) -> *mut u8;
    fn RAND_bytes(buf: *mut u8, num: c_int) -> c_int;
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
    if !detached
        && let Some(busy) = socket.callbacks.busy
        && unsafe { busy(user_data) } != FALSE
    {
        unsafe { g_timeout_add(4, dispatch_run, data) };
        return FALSE;
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
    stream: Upgraded,
    decoder: Decoder,
}

impl Connection {
    fn send(&mut self, shared: &Shared, opcode: u8, payload: &[u8]) -> bool {
        let frame = encode_frame(opcode, payload, mask_key());
        self.stream.write_all(&frame, &|| shared.exiting())
    }

    fn send_close(&mut self, shared: &Shared, code: i32, reason: Option<&[u8]>) {
        let payload = crate::close_frame(code, reason);
        self.send(shared, OP_CLOSE, &payload);
    }
}

fn random(buf: &mut [u8]) {
    unsafe { RAND_bytes(buf.as_mut_ptr(), buf.len() as c_int) };
}

fn mask_key() -> [u8; 4] {
    let mut key = [0u8; 4];
    random(&mut key);
    key
}

fn accept_for(key: &[u8]) -> Vec<u8> {
    let input = [key, ACCEPT_GUID].concat();
    let mut digest = [0u8; 20];
    unsafe { SHA1(input.as_ptr(), input.len(), digest.as_mut_ptr()) };
    base64(&digest)
}

fn handshake(socket: &Arc<Socket>) -> Result<Connection, Vec<u8>> {
    let shared = &socket.shared;
    let url = crate::until_nul(&shared.url);
    let http_url = ws_to_http(url).ok_or_else(|| b"unsupported URL scheme".to_vec())?;
    let target = southstar_http::parse_target(&http_url).ok_or_else(|| b"invalid URL".to_vec())?;
    let mut nonce = [0u8; 16];
    random(&mut nonce);
    let key = base64(&nonce);
    let mut lines = vec![
        b"Upgrade: websocket".to_vec(),
        b"Sec-WebSocket-Version: 13".to_vec(),
        [&b"Sec-WebSocket-Key: "[..], &key].concat(),
    ];
    if let Some(origin) = shared.origin.as_deref().filter(|o| !o.is_empty()) {
        lines.push([b"Origin: ".as_slice(), crate::until_nul(origin)].concat());
    }
    if !shared.protocols.is_empty() {
        lines.push(
            [
                b"Sec-WebSocket-Protocol: ".as_slice(),
                &shared.protocols.join(&b", "[..]),
            ]
            .concat(),
        );
    }
    let route = southstar_net::route::route(&http_url, &target.host);
    let request = Request {
        url: &http_url,
        https: target.https,
        host: &target.host,
        port: target.port,
        authority: &target.authority,
        path: &target.path,
        method: b"GET",
        user_agent: Some(southstar_net::route::user_agent()),
        referer: None,
        cookie: None,
        extra_headers: lines.iter().map(Vec::as_slice).collect(),
        body: b"",
        timeout: Duration::MAX,
        connect_timeout: Duration::from_secs(15),
        allow_insecure: route.allow_insecure,
        tls: route.tls,
        proxy: route.proxy,
    };
    let mut stream =
        southstar_http::upgrade(&request, &|| shared.exiting()).map_err(String::into_bytes)?;
    if stream.status != 101 {
        return Err(format!("WebSocket handshake failed (HTTP {})", stream.status).into_bytes());
    }
    let upgraded = header(&stream.headers, b"upgrade").is_some_and(|v| has_token(v, b"websocket"))
        && header(&stream.headers, b"connection").is_some_and(|v| has_token(v, b"upgrade"));
    let accepted = header(&stream.headers, b"sec-websocket-accept") == Some(&accept_for(&key)[..]);
    if !upgraded || !accepted {
        return Err(b"WebSocket handshake failed (bad upgrade response)".to_vec());
    }
    if let Some(chosen) = header(&stream.headers, b"sec-websocket-protocol") {
        if !shared.protocols.iter().any(|p| p.as_slice() == chosen) {
            return Err(b"WebSocket handshake failed (unrequested subprotocol)".to_vec());
        }
        *shared.protocol.lock().unwrap_or_else(|e| e.into_inner()) = chosen.to_vec();
    }
    stream.set_poll_interval(POLL_INTERVAL);
    Ok(Connection {
        stream,
        decoder: Decoder::default(),
    })
}

fn worker(socket: Arc<Socket>) {
    let shared = &socket.shared;
    let mut connection = match handshake(&socket) {
        Ok(c) if !shared.exiting() => c,
        Ok(_) => {
            post_close(&socket, 1006, b"aborted", false);
            return;
        }
        Err(message) => {
            if !shared.exiting() {
                post(&socket, Post::Error(message.clone()));
            }
            post_close(&socket, 1006, &message, false);
            return;
        }
    };
    shared.set_state(STATE_OPEN);
    post(&socket, Post::Open);

    let mut clean_close = false;
    let mut close_code = 1006;
    let mut close_reason: Option<Vec<u8>> = None;
    let mut peer_reason: Option<Vec<u8>> = None;
    let mut assembly = Assembly::default();
    let mut buf = vec![0u8; 16_384];

    'outer: while !shared.exiting() {
        while let Some(out) = shared.pop() {
            match out {
                Out::Text(data) => {
                    connection.send(shared, OP_TEXT, &data);
                }
                Out::Binary(data) => {
                    connection.send(shared, OP_BINARY, &data);
                }
                Out::Close { code, reason } => {
                    close_code = code;
                    close_reason = reason.map(|r| crate::until_nul(&r).to_vec());
                    shared.set_state(STATE_CLOSING);
                    connection.send_close(shared, close_code, close_reason.as_deref());
                    clean_close = true;
                    break 'outer;
                }
            }
        }
        loop {
            let frame = match connection.decoder.next_frame() {
                Decoded::NeedMore => break,
                Decoded::Frame(frame) => frame,
                Decoded::TooBig => {
                    shared.set_state(STATE_CLOSING);
                    connection.send_close(shared, 1009, Some(b"message too big"));
                    close_code = 1009;
                    close_reason = Some(b"message too big".to_vec());
                    break 'outer;
                }
                Decoded::ProtocolError => {
                    shared.set_state(STATE_CLOSING);
                    connection.send_close(shared, 1002, Some(b"protocol error"));
                    post(&socket, Post::Error(b"WebSocket protocol error".to_vec()));
                    close_code = 1002;
                    close_reason = Some(b"protocol error".to_vec());
                    break 'outer;
                }
            };
            match assembly.frame(&frame.payload, &frame.meta) {
                Frame::Close { code, reason } => {
                    shared.set_state(STATE_CLOSING);
                    connection.send_close(shared, crate::echo_close_code(code), None);
                    clean_close = true;
                    close_code = code;
                    close_reason = reason.clone();
                    peer_reason = reason;
                    break 'outer;
                }
                Frame::Ping(len) => {
                    connection.send(shared, OP_PONG, &frame.payload[..len]);
                }
                Frame::Message { text, data } => post(&socket, Post::Message { text, data }),
                Frame::TooBig => {
                    shared.set_state(STATE_CLOSING);
                    connection.send_close(shared, 1009, Some(b"message too big"));
                    close_code = 1009;
                    close_reason = Some(b"message too big".to_vec());
                    break 'outer;
                }
                Frame::BadUtf8 => {
                    shared.set_state(STATE_CLOSING);
                    connection.send_close(shared, 1007, Some(b"invalid utf-8"));
                    close_code = 1007;
                    close_reason = Some(b"invalid utf-8".to_vec());
                    break 'outer;
                }
                Frame::Ignored | Frame::Assembled => {}
            }
        }
        match connection.stream.read(&mut buf) {
            Received::Data(n) => connection.decoder.push(&buf[..n]),
            Received::Idle => shared.wait(),
            Received::Closed | Received::Failed => {
                let message = b"connection closed".to_vec();
                post(&socket, Post::Error(message.clone()));
                close_code = 1006;
                close_reason = Some(message);
                break;
            }
        }
    }
    let reason = close_reason.or(peer_reason).unwrap_or_default();
    post_close(&socket, close_code, &reason, clean_close);
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
pub unsafe extern "C" fn ns_ws_protocol(ws: *mut WebSocket) -> *mut c_char {
    let Some(shared) = (unsafe { shared(ws) }) else {
        return core::ptr::null_mut();
    };
    let protocol = shared.protocol.lock().unwrap_or_else(|e| e.into_inner());
    glib::strdup(&protocol)
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
