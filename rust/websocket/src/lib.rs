//! Southstar — WebSocket client: the RFC 6455 frame codec and handshake checks, frame reassembly, the close handshake and the outgoing queue.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub const STATE_CONNECTING: i32 = 0;
pub const STATE_OPEN: i32 = 1;
pub const STATE_CLOSING: i32 = 2;
pub const STATE_CLOSED: i32 = 3;

pub const MAX_MESSAGE: usize = 8 * 1024 * 1024;
const POLL: Duration = Duration::from_millis(10);
const MAX_PENDING: i32 = 256;

pub const WS_TEXT: i32 = 1 << 0;
pub const WS_BINARY: i32 = 1 << 1;
pub const WS_CONT: i32 = 1 << 2;
pub const WS_CLOSE: i32 = 1 << 3;
pub const WS_PING: i32 = 1 << 4;

pub enum Out {
    Text(Vec<u8>),
    Binary(Vec<u8>),
    Close { code: i32, reason: Option<Vec<u8>> },
}

pub enum Post {
    Open,
    Message {
        text: bool,
        data: Vec<u8>,
    },
    Close {
        code: i32,
        reason: Vec<u8>,
        clean: bool,
    },
    Error(Vec<u8>),
}

pub struct Shared {
    pub url: Vec<u8>,
    pub origin: Option<Vec<u8>>,
    pub protocols: Vec<Vec<u8>>,
    pub protocol: Mutex<Vec<u8>>,
    queue: Mutex<VecDeque<Out>>,
    wake: Condvar,
    pub state: AtomicI32,
    pub exit_requested: AtomicBool,
    pub detached: AtomicBool,
    pub pending: AtomicI32,
}

impl Shared {
    pub fn new(url: Vec<u8>, origin: Option<Vec<u8>>, protocols: Vec<Vec<u8>>) -> Shared {
        Shared {
            url,
            origin,
            protocols,
            protocol: Mutex::new(Vec::new()),
            queue: Mutex::new(VecDeque::new()),
            wake: Condvar::new(),
            state: AtomicI32::new(STATE_CONNECTING),
            exit_requested: AtomicBool::new(false),
            detached: AtomicBool::new(false),
            pending: AtomicI32::new(0),
        }
    }

    fn queue(&self) -> std::sync::MutexGuard<'_, VecDeque<Out>> {
        self.queue.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn exiting(&self) -> bool {
        self.exit_requested.load(Ordering::SeqCst)
    }

    pub fn state(&self) -> i32 {
        self.state.load(Ordering::SeqCst)
    }

    pub fn set_state(&self, state: i32) {
        self.state.store(state, Ordering::SeqCst);
    }

    pub fn try_reserve_post(&self, droppable: bool) -> bool {
        if self.detached.load(Ordering::SeqCst)
            || (droppable && self.pending.load(Ordering::SeqCst) >= MAX_PENDING)
        {
            return false;
        }
        self.pending.fetch_add(1, Ordering::SeqCst);
        true
    }

    pub fn enqueue(&self, out: Out) -> bool {
        let state = self.state();
        let is_close = matches!(out, Out::Close { .. });
        if state == STATE_CLOSED || (!is_close && state == STATE_CLOSING) {
            return false;
        }
        if let Out::Text(data) | Out::Binary(data) = &out {
            if data.len() > MAX_MESSAGE {
                return false;
            }
        }
        self.queue().push_back(out);
        self.wake.notify_one();
        true
    }

    pub fn close(&self, code: i32, reason: Option<&[u8]>) {
        let state = self.state();
        if state == STATE_CLOSING || state == STATE_CLOSED {
            return;
        }
        let code = if code <= 0 { 1000 } else { code };
        let reason = reason.filter(|r| !r.is_empty()).map(<[u8]>::to_vec);
        self.enqueue(Out::Close { code, reason });
        self.set_state(STATE_CLOSING);
    }

    pub fn request_exit(&self) {
        self.detached.store(true, Ordering::SeqCst);
        self.exit_requested.store(true, Ordering::SeqCst);
        let _queue = self.queue();
        self.wake.notify_one();
    }

    pub fn pop(&self) -> Option<Out> {
        self.queue().pop_front()
    }

    pub fn wait(&self) {
        let queue = self.queue();
        if queue.is_empty() && !self.exiting() {
            let _ = self.wake.wait_timeout(queue, POLL);
        }
    }
}

pub fn echo_close_code(code: i32) -> i32 {
    if matches!(code, 1000..=1003) || (1007..=1014).contains(&code) || (3000..=4999).contains(&code)
    {
        code
    } else {
        0
    }
}

pub fn close_frame(code: i32, reason: Option<&[u8]>) -> Vec<u8> {
    let mut frame = Vec::with_capacity(125);
    if code > 0 {
        frame.push(((code >> 8) & 0xff) as u8);
        frame.push((code & 0xff) as u8);
        if let Some(reason) = reason.filter(|r| !r.is_empty()) {
            let reason = until_nul(reason);
            frame.extend_from_slice(&reason[..reason.len().min(123)]);
        }
    }
    frame
}

pub fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end])
}

pub struct FrameMeta {
    pub flags: i32,
    pub offset: i64,
    pub bytes_left: i64,
}

pub enum Frame {
    Close { code: i32, reason: Option<Vec<u8>> },
    Ping(usize),
    Ignored,
    Assembled,
    Message { text: bool, data: Vec<u8> },
    BadUtf8,
    TooBig,
}

#[derive(Default)]
pub struct Assembly {
    data: Vec<u8>,
    binary: bool,
    in_message: bool,
}

impl Assembly {
    pub fn frame(&mut self, data: &[u8], meta: &FrameMeta) -> Frame {
        let flags = meta.flags;
        if flags & WS_CLOSE != 0 {
            let mut code = 1005;
            let mut reason = None;
            if (2..=125).contains(&data.len()) {
                code = (i32::from(data[0]) << 8) | i32::from(data[1]);
                if data.len() > 2 {
                    reason = Some(until_nul(&data[2..]).to_vec());
                }
            } else if data.len() == 1 {
                code = 1002;
            }
            return Frame::Close { code, reason };
        }
        if flags & WS_PING != 0 {
            return Frame::Ping(data.len().min(125));
        }
        if flags & (WS_TEXT | WS_BINARY | WS_CONT) == 0 {
            return Frame::Ignored;
        }
        if meta.offset == 0 && !self.in_message && flags & (WS_TEXT | WS_BINARY) != 0 {
            self.data.clear();
            self.binary = flags & WS_BINARY != 0;
            self.in_message = true;
        } else if !self.in_message {
            return Frame::Ignored;
        }
        if !data.is_empty() {
            if self.data.len() + data.len() > MAX_MESSAGE {
                self.data.clear();
                self.in_message = false;
                return Frame::TooBig;
            }
            self.data.extend_from_slice(data);
        }
        if meta.bytes_left != 0 || flags & WS_CONT != 0 {
            return Frame::Assembled;
        }
        self.in_message = false;
        let text = !self.binary;
        let data = std::mem::take(&mut self.data);
        if text && !data.is_empty() && (data.contains(&0) || std::str::from_utf8(&data).is_err()) {
            return Frame::BadUtf8;
        }
        Frame::Message { text, data }
    }
}

pub const OP_CONTINUATION: u8 = 0;
pub const OP_TEXT: u8 = 1;
pub const OP_BINARY: u8 = 2;
pub const OP_CLOSE: u8 = 8;
pub const OP_PING: u8 = 9;
pub const OP_PONG: u8 = 10;

pub fn encode_frame(opcode: u8, payload: &[u8], mask: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 14);
    out.push(0x80 | opcode);
    let len = payload.len();
    if len < 126 {
        out.push(0x80 | len as u8);
    } else if len <= 0xffff {
        out.push(0x80 | 126);
        out.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        out.push(0x80 | 127);
        out.extend_from_slice(&(len as u64).to_be_bytes());
    }
    out.extend_from_slice(&mask);
    out.extend(payload.iter().enumerate().map(|(i, &b)| b ^ mask[i % 4]));
    out
}

pub struct RawFrame {
    pub meta: FrameMeta,
    pub opcode: u8,
    pub payload: Vec<u8>,
}

pub enum Decoded {
    Frame(RawFrame),
    NeedMore,
    TooBig,
    ProtocolError,
}

#[derive(Default)]
pub struct Decoder {
    buf: Vec<u8>,
    message_flag: i32,
}

impl Decoder {
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    pub fn next_frame(&mut self) -> Decoded {
        let b = &self.buf;
        if b.len() < 2 {
            return Decoded::NeedMore;
        }
        let fin = b[0] & 0x80 != 0;
        let opcode = b[0] & 0x0f;
        if b[0] & 0x70 != 0 || b[1] & 0x80 != 0 {
            return Decoded::ProtocolError;
        }
        let (len, header) = match b[1] & 0x7f {
            126 => {
                if b.len() < 4 {
                    return Decoded::NeedMore;
                }
                (u64::from(u16::from_be_bytes([b[2], b[3]])), 4)
            }
            127 => {
                if b.len() < 10 {
                    return Decoded::NeedMore;
                }
                let mut n = [0u8; 8];
                n.copy_from_slice(&b[2..10]);
                (u64::from_be_bytes(n), 10)
            }
            n => (u64::from(n), 2),
        };
        let control = opcode & 0x08 != 0;
        if control && (len > 125 || !fin) {
            return Decoded::ProtocolError;
        }
        if !matches!(
            opcode,
            OP_CONTINUATION | OP_TEXT | OP_BINARY | OP_CLOSE | OP_PING | OP_PONG
        ) {
            return Decoded::ProtocolError;
        }
        if len > MAX_MESSAGE as u64 {
            return Decoded::TooBig;
        }
        let total = header + len as usize;
        if b.len() < total {
            return Decoded::NeedMore;
        }
        let payload = b[header..total].to_vec();
        self.buf.drain(..total);
        let cont = if fin { 0 } else { WS_CONT };
        let flags = match opcode {
            OP_TEXT | OP_BINARY => {
                if self.message_flag != 0 {
                    return Decoded::ProtocolError;
                }
                let kind = if opcode == OP_TEXT {
                    WS_TEXT
                } else {
                    WS_BINARY
                };
                self.message_flag = if fin { 0 } else { kind };
                kind | cont
            }
            OP_CONTINUATION => {
                let kind = self.message_flag;
                if kind == 0 {
                    return Decoded::ProtocolError;
                }
                if fin {
                    self.message_flag = 0;
                }
                kind | cont
            }
            OP_CLOSE => WS_CLOSE,
            OP_PING => WS_PING,
            _ => 0,
        };
        Decoded::Frame(RawFrame {
            meta: FrameMeta {
                flags,
                offset: 0,
                bytes_left: 0,
            },
            opcode,
            payload,
        })
    }
}

pub const ACCEPT_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(B64[(n >> 18) as usize & 63]);
        out.push(B64[(n >> 12) as usize & 63]);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63]
        } else {
            b'='
        });
    }
    out
}

pub fn header<'a>(headers: &'a [(Vec<u8>, Vec<u8>)], name: &[u8]) -> Option<&'a [u8]> {
    headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_slice())
}

pub fn has_token(value: &[u8], token: &[u8]) -> bool {
    value
        .split(|&c| c == b',')
        .any(|t| t.trim_ascii().eq_ignore_ascii_case(token))
}

pub fn ws_to_http(url: &[u8]) -> Option<Vec<u8>> {
    if url.len() >= 6 && url[..6].eq_ignore_ascii_case(b"wss://") {
        Some([&b"https://"[..], &url[6..]].concat())
    } else if url.len() >= 5 && url[..5].eq_ignore_ascii_case(b"ws://") {
        Some([&b"http://"[..], &url[5..]].concat())
    } else if url.len() >= 8 && url[..8].eq_ignore_ascii_case(b"https://")
        || url.len() >= 7 && url[..7].eq_ignore_ascii_case(b"http://")
    {
        Some(url.to_vec())
    } else {
        None
    }
}
