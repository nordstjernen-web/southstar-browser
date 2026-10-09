//! Southstar — one response as the caller sees it: status and header lines, the content decoder, the first-byte time and the stop conditions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::time::Instant;

use crate::decode::{DecodeError, Decoder, Encoding, classify};
use crate::h1::{status_code, until_nul};

pub const MAX_HEADER_BYTES: usize = 1024 * 1024;

pub trait Handler {
    fn should_abort(&self) -> bool;
    fn status_line(&mut self, line: &[u8]);
    fn header(&mut self, line: &[u8], name: &[u8], value: &[u8]);
    fn body(&mut self, data: &[u8]) -> bool;
    fn headers_done(&mut self) {}
}

pub struct Transfer<'h> {
    pub handler: &'h mut dyn Handler,
    pub start: Instant,
    pub deadline: Instant,
    pub status: i64,
    pub informational: bool,
    pub status_line_fed: bool,
    pub got_first_byte: bool,
    pub sink_full: bool,
    pub corrupt: bool,
    pub proto_error: bool,
    pub header_bytes: usize,
    pub first_byte_ms: Option<f64>,
    encoding: Encoding,
    decoder: Option<Decoder>,
}

impl<'h> Transfer<'h> {
    pub fn new(handler: &'h mut dyn Handler, start: Instant, deadline: Instant) -> Transfer<'h> {
        Transfer {
            handler,
            start,
            deadline,
            status: 0,
            informational: false,
            status_line_fed: false,
            got_first_byte: false,
            sink_full: false,
            corrupt: false,
            proto_error: false,
            header_bytes: 0,
            first_byte_ms: None,
            encoding: Encoding::Identity,
            decoder: None,
        }
    }

    pub fn reset(&mut self) {
        self.status = 0;
        self.informational = false;
        self.status_line_fed = false;
        self.got_first_byte = false;
        self.header_bytes = 0;
        self.encoding = Encoding::Identity;
        self.decoder = None;
    }

    pub fn should_abort(&self) -> bool {
        self.handler.should_abort() || Instant::now() > self.deadline
    }

    pub fn ms_since_start(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }

    fn note_header(&mut self, name: &[u8], value: &[u8]) {
        if name.eq_ignore_ascii_case(b"content-encoding") {
            if let Some(e) = classify(value) {
                self.encoding = e;
            }
        }
    }

    pub fn h2_header(&mut self, name: &[u8], value: &[u8], proto: &[u8]) {
        if name.is_empty() || self.proto_error {
            return;
        }
        self.header_bytes += name.len() + value.len();
        if self.header_bytes > MAX_HEADER_BYTES {
            self.proto_error = true;
            return;
        }
        if name[0] == b':' {
            if name == b":status" {
                let status = crate::h1::strtoll(until_nul(value), 10);
                self.informational = status / 100 == 1;
                if self.informational {
                    return;
                }
                self.status = status;
                self.feed_status_line(proto);
            }
            return;
        }
        if self.informational {
            return;
        }
        let name = until_nul(name);
        let value = until_nul(value);
        self.note_header(name, value);
        let mut line = Vec::with_capacity(name.len() + value.len() + 4);
        line.extend_from_slice(name);
        line.extend_from_slice(b": ");
        line.extend_from_slice(value);
        line.extend_from_slice(b"\r\n");
        self.handler.header(&line, name, value);
    }

    fn feed_status_line(&mut self, proto: &[u8]) {
        if self.status_line_fed {
            return;
        }
        self.status_line_fed = true;
        let line = format!("{} {}\r\n", String::from_utf8_lossy(proto), self.status);
        self.handler.status_line(line.as_bytes());
    }

    pub fn h1_status_line(&mut self, line: &[u8]) {
        let line = until_nul(line);
        if let Some(code) = status_code(line) {
            self.status = code;
        }
        self.informational = self.status / 100 == 1;
        if !self.informational {
            let mut fed = line.to_vec();
            fed.extend_from_slice(b"\r\n");
            self.handler.status_line(&fed);
            self.status_line_fed = true;
        }
    }

    pub fn h1_header_line(&mut self, line: &[u8]) {
        let line = until_nul(line);
        if let Some((name, value)) = crate::h1::split_header(line) {
            self.note_header(name, value);
            let mut fed = line.to_vec();
            fed.extend_from_slice(b"\r\n");
            self.handler.header(&fed, name, value);
        } else {
            let mut fed = line.to_vec();
            fed.extend_from_slice(b"\r\n");
            self.handler.header(&fed, b"", b"");
        }
    }

    pub fn body(&mut self, data: &[u8]) {
        if data.is_empty() || self.sink_full || self.corrupt {
            return;
        }
        self.got_first_byte = true;
        if self.first_byte_ms.is_none() {
            self.first_byte_ms = Some(self.ms_since_start());
        }
        if self.decoder.is_none() {
            match Decoder::new(self.encoding) {
                Some(d) => self.decoder = Some(d),
                None => {
                    self.corrupt = true;
                    return;
                }
            }
        }
        let handler = &mut *self.handler;
        let mut emit = |bytes: &[u8]| handler.body(bytes);
        let result = self
            .decoder
            .as_mut()
            .map_or(Ok(()), |d| d.feed(data, &mut emit));
        self.note(result);
    }

    pub fn finish(&mut self) {
        let handler = &mut *self.handler;
        let mut emit = |bytes: &[u8]| handler.body(bytes);
        if let Some(d) = self.decoder.as_mut() {
            let result = d.finish(&mut emit);
            self.note(result);
        }
    }

    fn note(&mut self, result: Result<(), DecodeError>) {
        match result {
            Ok(()) => {}
            Err(DecodeError::Stopped) => self.sink_full = true,
            Err(DecodeError::Corrupt) => self.corrupt = true,
        }
    }

    pub fn stopped(&self) -> bool {
        self.sink_full || self.corrupt
    }
}
