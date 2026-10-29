//! Southstar — the WebSocket connection over rust/http's HTTP/1.1 upgrade and main-loop dispatch.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::JoinHandle;
use std::time::Duration;

use southstar_http::{Received, Request, Upgraded};

use crate::{
    ACCEPT_GUID, Assembly, Decoded, Decoder, Frame, OP_BINARY, OP_CLOSE, OP_PONG, OP_TEXT, Out,
    Post, STATE_CLOSED, STATE_CLOSING, STATE_OPEN, Shared, base64, encode_frame, has_token, header,
    ws_to_http,
};

#[derive(Clone, Copy)]
pub struct Events {
    pub on_open: fn(data: usize),
    pub on_text: fn(text: &[u8], data: usize),
    pub on_binary: fn(bytes: &[u8], data: usize),
    pub on_close: fn(code: i32, reason: &[u8], clean: bool, data: usize),
    pub on_error: fn(message: &[u8], data: usize),
    pub busy: fn(data: usize) -> bool,
}

struct Socket {
    shared: Shared,
    top_url: Option<Vec<u8>>,
    events: Events,
    data: usize,
}

pub struct WebSocket {
    socket: Arc<Socket>,
    thread: Option<JoinHandle<()>>,
}

const POLL_INTERVAL: Duration = Duration::from_millis(10);

unsafe extern "C" {
    fn SHA1(data: *const u8, len: usize, digest: *mut u8) -> *mut u8;
    fn RAND_bytes(buf: *mut u8, num: c_int) -> c_int;
}

struct Dispatch {
    socket: Arc<Socket>,
    post: Post,
}

fn dispatch_run(dispatch: Box<Dispatch>) {
    let socket = &dispatch.socket;
    let detached = socket.shared.detached.load(Ordering::SeqCst);
    let data_arg = socket.data;
    if !detached && (socket.events.busy)(data_arg) {
        southstar_glib::timeout_add_once(4, move || dispatch_run(dispatch));
        return;
    }
    if !detached {
        let events = dispatch.socket.events;
        match &dispatch.post {
            Post::Open => (events.on_open)(data_arg),
            Post::Message { text: true, data } => (events.on_text)(data, data_arg),
            Post::Message { text: false, data } => (events.on_binary)(data, data_arg),
            Post::Close {
                code,
                reason,
                clean,
            } => (events.on_close)(*code, crate::until_nul(reason), *clean, data_arg),
            Post::Error(message) => (events.on_error)(crate::until_nul(message), data_arg),
        }
    }
    dispatch
        .socket
        .shared
        .pending
        .fetch_sub(1, Ordering::SeqCst);
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
    southstar_glib::idle_add_once(move || dispatch_run(dispatch));
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
    let credentials =
        southstar_net::credentials::for_subresource(&http_url, socket.top_url.as_deref());
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
        cookie: credentials.as_ref().and_then(|c| c.cookie_header()),
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
    if let Some(credentials) = &credentials {
        stream
            .headers
            .iter()
            .filter(|(name, _)| name.eq_ignore_ascii_case(b"set-cookie"))
            .for_each(|(_, value)| credentials.store(value));
    }
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

impl WebSocket {
    pub fn open(
        url: &[u8],
        origin: Option<&[u8]>,
        top_url: Option<&[u8]>,
        protocols: Vec<Vec<u8>>,
        events: Events,
        data: usize,
    ) -> WebSocket {
        let socket = Arc::new(Socket {
            shared: Shared::new(url.to_vec(), origin.map(<[u8]>::to_vec), protocols),
            top_url: top_url.map(<[u8]>::to_vec),
            events,
            data,
        });
        let worker_socket = Arc::clone(&socket);
        let thread = std::thread::Builder::new()
            .name("nd-ws".into())
            .spawn(move || worker(worker_socket))
            .ok();
        WebSocket { socket, thread }
    }

    pub fn send_text(&self, text: &[u8]) -> bool {
        self.socket.shared.enqueue(Out::Text(text.to_vec()))
    }

    pub fn send_binary(&self, bytes: &[u8]) -> bool {
        self.socket.shared.enqueue(Out::Binary(bytes.to_vec()))
    }

    pub fn close(&self, code: i32, reason: Option<&[u8]>) {
        self.socket.shared.close(code, reason);
    }

    pub fn state(&self) -> i32 {
        self.socket.shared.state()
    }

    pub fn protocol(&self) -> Vec<u8> {
        self.socket
            .shared
            .protocol
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    pub fn free(mut self) {
        self.socket.shared.request_exit();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
