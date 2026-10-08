//! Southstar — WebSocket client over libcurl's native WebSocket API: frame reassembly, the close handshake and the outgoing queue.
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
