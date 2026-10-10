//! Southstar — EventSource / Server-Sent Events: the stream parser, the response-header checks and the reconnecting worker.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

const MAX_LINE: usize = 1 << 20;
const MAX_EVENT: usize = 8 << 20;
const MAX_PENDING: i32 = 256;

pub struct Message {
    pub event: Vec<u8>,
    pub data: Vec<u8>,
    pub last_id: Vec<u8>,
}

pub enum Post {
    Open,
    Message(Message),
    Error { fatal: bool },
}

pub struct Stream {
    pub last_event_id: Vec<u8>,
    pub reconnect_ms: i64,
}

pub struct Shared {
    pub url: Vec<u8>,
    pub origin: Option<Vec<u8>>,
    pub stream: Mutex<Stream>,
    pub exit_requested: AtomicBool,
    pub detached: AtomicBool,
    pub pending: AtomicI32,
}

impl Shared {
    pub fn new(url: Vec<u8>, origin: Option<Vec<u8>>, last_event_id: Vec<u8>) -> Shared {
        Shared {
            url,
            origin,
            stream: Mutex::new(Stream {
                last_event_id,
                reconnect_ms: 3000,
            }),
            exit_requested: AtomicBool::new(false),
            detached: AtomicBool::new(false),
            pending: AtomicI32::new(0),
        }
    }

    fn stream(&self) -> std::sync::MutexGuard<'_, Stream> {
        self.stream.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn exiting(&self) -> bool {
        self.exit_requested.load(Ordering::SeqCst)
    }

    pub fn last_event_id(&self) -> Vec<u8> {
        self.stream().last_event_id.clone()
    }

    pub fn reconnect_ms(&self) -> i64 {
        self.stream().reconnect_ms
    }

    pub fn try_reserve_post(&self) -> bool {
        if self.detached.load(Ordering::SeqCst)
            || self.pending.load(Ordering::SeqCst) >= MAX_PENDING
        {
            return false;
        }
        self.pending.fetch_add(1, Ordering::SeqCst);
        true
    }
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end])
}

fn strtoll_saturating(digits: &[u8]) -> i64 {
    digits.iter().fold(0i64, |acc, &d| {
        acc.saturating_mul(10).saturating_add(i64::from(d - b'0'))
    })
}

pub struct Parser {
    line: Vec<u8>,
    last_was_cr: bool,
    data: Vec<u8>,
    event_type: Option<Vec<u8>>,
    pub status: i64,
    pub opened: bool,
    pub fatal: bool,
    pub is_event_stream: bool,
}

impl Default for Parser {
    fn default() -> Parser {
        Parser::new()
    }
}

impl Parser {
    pub fn new() -> Parser {
        Parser {
            line: Vec::new(),
            last_was_cr: false,
            data: Vec::new(),
            event_type: None,
            status: 0,
            opened: false,
            fatal: false,
            is_event_stream: false,
        }
    }

    fn dispatch_event(&mut self, shared: &Shared, post: &mut impl FnMut(Post)) {
        if self.data.is_empty() {
            self.event_type = None;
            return;
        }
        if self.data.last() == Some(&b'\n') {
            self.data.pop();
        }
        let message = Message {
            event: self
                .event_type
                .take()
                .unwrap_or_else(|| b"message".to_vec()),
            data: std::mem::take(&mut self.data),
            last_id: shared.last_event_id(),
        };
        post(Post::Message(message));
    }

    fn process_line(&mut self, line: &[u8], shared: &Shared, post: &mut impl FnMut(Post)) {
        let line = until_nul(line);
        if line.is_empty() {
            self.dispatch_event(shared, post);
            return;
        }
        if line[0] == b':' {
            return;
        }
        let (field, value) = match line.iter().position(|&c| c == b':') {
            Some(colon) => {
                let value = &line[colon + 1..];
                (&line[..colon], value.strip_prefix(b" ").unwrap_or(value))
            }
            None => (line, &b""[..]),
        };
        match field {
            b"data" => {
                if self.data.len() + value.len() + 1 > MAX_EVENT {
                    self.fatal = true;
                } else {
                    self.data.extend_from_slice(value);
                    self.data.push(b'\n');
                }
            }
            b"event" => self.event_type = (!value.is_empty()).then(|| value.to_vec()),
            b"id" => shared.stream().last_event_id = value.to_vec(),
            b"retry" if !value.is_empty() && value.iter().all(u8::is_ascii_digit) => {
                let ms = strtoll_saturating(value);
                if ms > 0 {
                    shared.stream().reconnect_ms = ms.min(86_400_000);
                }
            }
            _ => {}
        }
    }

    pub fn feed(&mut self, buf: &[u8], shared: &Shared, post: &mut impl FnMut(Post)) {
        for &c in buf {
            if c == b'\n' && self.last_was_cr {
                self.last_was_cr = false;
                continue;
            }
            self.last_was_cr = false;
            if c == b'\r' || c == b'\n' {
                if c == b'\r' {
                    self.last_was_cr = true;
                }
                let line = std::mem::take(&mut self.line);
                self.process_line(&line, shared, post);
            } else {
                if self.line.len() >= MAX_LINE {
                    self.fatal = true;
                    return;
                }
                self.line.push(c);
            }
        }
    }

    pub fn header(&mut self, header: &[u8], post: &mut impl FnMut(Post)) {
        let total = header.len();
        if total >= 5 && header[..5].eq_ignore_ascii_case(b"HTTP/") {
            let mut code = 0;
            if let Some(sp) = header.iter().position(|&c| c == b' ') {
                let digits: Vec<u8> = header[sp + 1..]
                    .iter()
                    .take(7)
                    .take_while(|c| c.is_ascii_digit())
                    .copied()
                    .collect();
                code = strtoll_saturating(&digits);
            }
            self.status = code;
            self.is_event_stream = false;
        } else if total >= 13 && header[..13].eq_ignore_ascii_case(b"Content-Type:") {
            if until_nul(header)
                .windows(17)
                .any(|w| w == b"text/event-stream")
            {
                self.is_event_stream = true;
            }
        } else if total <= 2 && !self.opened {
            if self.status == 200 && self.is_event_stream {
                self.opened = true;
                post(Post::Open);
            } else if self.status >= 200 && (self.status < 300 || self.status >= 400) {
                self.fatal = true;
            }
        }
    }
}
