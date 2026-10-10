//! Southstar — a client-side HTTP/2 connection without I/O: request submission, frame parsing, HPACK, flow control, SETTINGS, PING, RST_STREAM and GOAWAY.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::{BTreeMap, VecDeque};

use crate::frame::{self, Header};
use crate::hpack;

pub const LOCAL_WINDOW: u32 = 8 * 1024 * 1024;
pub const LOCAL_MAX_CONCURRENT: u32 = 64;
const MAX_HEADER_BLOCK: usize = 2 * 1024 * 1024;

pub enum Event {
    Headers {
        stream: u32,
        fields: Vec<(Vec<u8>, Vec<u8>)>,
    },
    Data {
        stream: u32,
        bytes: Vec<u8>,
    },
    Closed {
        stream: u32,
        code: u32,
    },
}

struct Stream {
    send_window: i64,
    body: Vec<u8>,
    sent: usize,
    local_closed: bool,
    consumed: u32,
}

struct Block {
    stream: u32,
    bytes: Vec<u8>,
    end_stream: bool,
}

pub struct Connection {
    decoder: hpack::Decoder,
    output: Vec<u8>,
    input: Vec<u8>,
    peer_max_frame: usize,
    peer_initial_window: i64,
    peer_max_concurrent: u32,
    send_window: i64,
    consumed: u32,
    streams: BTreeMap<u32, Stream>,
    next_id: u32,
    goaway: bool,
    block: Option<Block>,
    events: VecDeque<Event>,
}

#[derive(Debug)]
pub struct ConnectionError;

impl Connection {
    pub fn new() -> Connection {
        let mut output = frame::PREFACE.to_vec();
        frame::write_settings(
            &mut output,
            &[
                (frame::SETTINGS_MAX_CONCURRENT_STREAMS, LOCAL_MAX_CONCURRENT),
                (frame::SETTINGS_INITIAL_WINDOW_SIZE, LOCAL_WINDOW),
                (frame::SETTINGS_ENABLE_PUSH, 0),
            ],
        );
        frame::write_window_update(&mut output, 0, LOCAL_WINDOW - frame::DEFAULT_WINDOW as u32);
        Connection {
            decoder: hpack::Decoder::new(4096),
            output,
            input: Vec::new(),
            peer_max_frame: frame::DEFAULT_MAX_FRAME,
            peer_initial_window: frame::DEFAULT_WINDOW,
            peer_max_concurrent: u32::MAX,
            send_window: frame::DEFAULT_WINDOW,
            consumed: 0,
            streams: BTreeMap::new(),
            next_id: 1,
            goaway: false,
            block: None,
            events: VecDeque::new(),
        }
    }

    pub fn goaway(&self) -> bool {
        self.goaway
    }

    pub fn can_open(&self) -> bool {
        !self.goaway
            && self.next_id < 0x7fff_ffff
            && (self.streams.len() as u64) < u64::from(self.peer_max_concurrent)
    }

    pub fn active(&self) -> usize {
        self.streams.len()
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }

    pub fn next_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    pub fn submit(&mut self, fields: &[(&[u8], &[u8])], body: Vec<u8>) -> Option<u32> {
        if !self.can_open() {
            return None;
        }
        let id = self.next_id;
        self.next_id += 2;
        let block = hpack::encode(fields);
        let end_stream = body.is_empty();
        let max = self.peer_max_frame;
        let mut chunks = block.chunks(max).peekable();
        let first = chunks.next().unwrap_or(&[]);
        let mut flags = if end_stream {
            frame::FLAG_END_STREAM
        } else {
            0
        };
        if chunks.peek().is_none() {
            flags |= frame::FLAG_END_HEADERS;
        }
        frame::write(&mut self.output, frame::HEADERS, flags, id, first);
        while let Some(chunk) = chunks.next() {
            let flags = if chunks.peek().is_none() {
                frame::FLAG_END_HEADERS
            } else {
                0
            };
            frame::write(&mut self.output, frame::CONTINUATION, flags, id, chunk);
        }
        self.streams.insert(
            id,
            Stream {
                send_window: self.peer_initial_window,
                body,
                sent: 0,
                local_closed: end_stream,
                consumed: 0,
            },
        );
        self.flush_data();
        Some(id)
    }

    pub fn reset(&mut self, stream: u32, code: u32) {
        if self.streams.remove(&stream).is_some() {
            frame::write_rst(&mut self.output, stream, code);
        }
    }

    fn flush_data(&mut self) {
        let max = self.peer_max_frame as i64;
        for (&id, s) in self.streams.iter_mut() {
            while !s.local_closed {
                let remaining = (s.body.len() - s.sent) as i64;
                let n = remaining.min(max).min(s.send_window).min(self.send_window);
                if n <= 0 && remaining > 0 {
                    break;
                }
                let n = n.max(0) as usize;
                let end = s.sent + n == s.body.len();
                let flags = if end { frame::FLAG_END_STREAM } else { 0 };
                frame::write(
                    &mut self.output,
                    frame::DATA,
                    flags,
                    id,
                    &s.body[s.sent..s.sent + n],
                );
                s.sent += n;
                s.send_window -= n as i64;
                self.send_window -= n as i64;
                if end {
                    s.local_closed = true;
                    s.body = Vec::new();
                }
            }
        }
    }

    fn connection_error(&mut self, code: u32) -> ConnectionError {
        let last = self.next_id.saturating_sub(2);
        frame::write_goaway(&mut self.output, last, code);
        self.goaway = true;
        ConnectionError
    }

    pub fn receive(&mut self, bytes: &[u8]) -> Result<(), ConnectionError> {
        self.input.extend_from_slice(bytes);
        let mut pos = 0;
        let result = loop {
            if self.input.len() - pos < frame::HEADER_LEN {
                break Ok(());
            }
            let header = frame::parse_header(&self.input[pos..pos + frame::HEADER_LEN]);
            if header.len > frame::DEFAULT_MAX_FRAME {
                break Err(self.connection_error(frame::FRAME_SIZE_ERROR));
            }
            let total = frame::HEADER_LEN + header.len;
            if self.input.len() - pos < total {
                break Ok(());
            }
            let payload = self.input[pos + frame::HEADER_LEN..pos + total].to_vec();
            pos += total;
            if let Err(e) = self.handle(&header, &payload) {
                break Err(e);
            }
        };
        self.input.drain(..pos);
        self.flush_data();
        result
    }

    fn handle(&mut self, h: &Header, payload: &[u8]) -> Result<(), ConnectionError> {
        if let Some(block) = &self.block
            && (h.kind != frame::CONTINUATION || h.stream != block.stream)
        {
            return Err(self.connection_error(frame::PROTOCOL_ERROR));
        }
        match h.kind {
            frame::DATA => self.on_data(h, payload),
            frame::HEADERS => self.on_headers(h, payload),
            frame::CONTINUATION => self.on_continuation(h, payload),
            frame::RST_STREAM => {
                if payload.len() != 4 || h.stream == 0 {
                    return Err(self.connection_error(frame::FRAME_SIZE_ERROR));
                }
                let code = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                if self.streams.remove(&h.stream).is_some() {
                    self.events.push_back(Event::Closed {
                        stream: h.stream,
                        code,
                    });
                }
                Ok(())
            }
            frame::SETTINGS => self.on_settings(h, payload),
            frame::PUSH_PROMISE => Err(self.connection_error(frame::PROTOCOL_ERROR)),
            frame::PING => {
                if payload.len() != 8 || h.stream != 0 {
                    return Err(self.connection_error(frame::FRAME_SIZE_ERROR));
                }
                if h.flags & frame::FLAG_ACK == 0 {
                    frame::write(&mut self.output, frame::PING, frame::FLAG_ACK, 0, payload);
                }
                Ok(())
            }
            frame::GOAWAY => {
                if payload.len() < 8 {
                    return Err(self.connection_error(frame::FRAME_SIZE_ERROR));
                }
                let last = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]])
                    & 0x7fff_ffff;
                self.goaway = true;
                let refused: Vec<u32> = self.streams.range(last + 1..).map(|(&id, _)| id).collect();
                for id in refused {
                    self.streams.remove(&id);
                    self.events.push_back(Event::Closed {
                        stream: id,
                        code: frame::REFUSED_STREAM,
                    });
                }
                Ok(())
            }
            frame::WINDOW_UPDATE => self.on_window_update(h, payload),
            _ => Ok(()),
        }
    }

    fn on_data(&mut self, h: &Header, payload: &[u8]) -> Result<(), ConnectionError> {
        if h.stream == 0 {
            return Err(self.connection_error(frame::PROTOCOL_ERROR));
        }
        let len = payload.len() as u32;
        self.consumed += len;
        if self.consumed >= LOCAL_WINDOW / 2 {
            frame::write_window_update(&mut self.output, 0, self.consumed);
            self.consumed = 0;
        }
        let Some(data) = frame::strip_padding(h.flags, payload) else {
            return Err(self.connection_error(frame::PROTOCOL_ERROR));
        };
        let end_stream = h.flags & frame::FLAG_END_STREAM != 0;
        let Some(s) = self.streams.get_mut(&h.stream) else {
            return Ok(());
        };
        s.consumed += len;
        if !end_stream && s.consumed >= LOCAL_WINDOW / 2 {
            frame::write_window_update(&mut self.output, h.stream, s.consumed);
            s.consumed = 0;
        }
        self.events.push_back(Event::Data {
            stream: h.stream,
            bytes: data.to_vec(),
        });
        if end_stream {
            self.finish_remote(h.stream);
        }
        Ok(())
    }

    fn finish_remote(&mut self, stream: u32) {
        if let Some(s) = self.streams.remove(&stream) {
            if !s.local_closed {
                frame::write_rst(&mut self.output, stream, frame::NO_ERROR);
            }
            self.events.push_back(Event::Closed {
                stream,
                code: frame::NO_ERROR,
            });
        }
    }

    fn on_headers(&mut self, h: &Header, payload: &[u8]) -> Result<(), ConnectionError> {
        if h.stream == 0 {
            return Err(self.connection_error(frame::PROTOCOL_ERROR));
        }
        let Some(mut fragment) = frame::strip_padding(h.flags, payload) else {
            return Err(self.connection_error(frame::PROTOCOL_ERROR));
        };
        if h.flags & frame::FLAG_PRIORITY != 0 {
            if fragment.len() < 5 {
                return Err(self.connection_error(frame::PROTOCOL_ERROR));
            }
            fragment = &fragment[5..];
        }
        let block = Block {
            stream: h.stream,
            bytes: fragment.to_vec(),
            end_stream: h.flags & frame::FLAG_END_STREAM != 0,
        };
        if h.flags & frame::FLAG_END_HEADERS != 0 {
            self.finish_block(block)
        } else {
            self.block = Some(block);
            Ok(())
        }
    }

    fn on_continuation(&mut self, h: &Header, payload: &[u8]) -> Result<(), ConnectionError> {
        let Some(mut block) = self.block.take() else {
            return Err(self.connection_error(frame::PROTOCOL_ERROR));
        };
        block.bytes.extend_from_slice(payload);
        if block.bytes.len() > MAX_HEADER_BLOCK {
            return Err(self.connection_error(frame::PROTOCOL_ERROR));
        }
        if h.flags & frame::FLAG_END_HEADERS != 0 {
            self.finish_block(block)
        } else {
            self.block = Some(block);
            Ok(())
        }
    }

    fn finish_block(&mut self, block: Block) -> Result<(), ConnectionError> {
        let Ok(fields) = self.decoder.decode(&block.bytes) else {
            return Err(self.connection_error(frame::COMPRESSION_ERROR));
        };
        if !self.streams.contains_key(&block.stream) {
            return Ok(());
        }
        self.events.push_back(Event::Headers {
            stream: block.stream,
            fields,
        });
        if block.end_stream {
            self.finish_remote(block.stream);
        }
        Ok(())
    }

    fn on_settings(&mut self, h: &Header, payload: &[u8]) -> Result<(), ConnectionError> {
        if h.stream != 0 {
            return Err(self.connection_error(frame::PROTOCOL_ERROR));
        }
        if h.flags & frame::FLAG_ACK != 0 {
            return Ok(());
        }
        if !payload.len().is_multiple_of(6) {
            return Err(self.connection_error(frame::FRAME_SIZE_ERROR));
        }
        for entry in payload.chunks(6) {
            let id = u16::from_be_bytes([entry[0], entry[1]]);
            let value = u32::from_be_bytes([entry[2], entry[3], entry[4], entry[5]]);
            match id {
                frame::SETTINGS_MAX_CONCURRENT_STREAMS => self.peer_max_concurrent = value,
                frame::SETTINGS_INITIAL_WINDOW_SIZE => {
                    if i64::from(value) > frame::MAX_WINDOW {
                        return Err(self.connection_error(frame::FLOW_CONTROL_ERROR));
                    }
                    let delta = i64::from(value) - self.peer_initial_window;
                    self.peer_initial_window = i64::from(value);
                    for s in self.streams.values_mut() {
                        s.send_window += delta;
                    }
                }
                frame::SETTINGS_MAX_FRAME_SIZE => {
                    if !(16_384..=16_777_215).contains(&value) {
                        return Err(self.connection_error(frame::PROTOCOL_ERROR));
                    }
                    self.peer_max_frame = value as usize;
                }
                frame::SETTINGS_HEADER_TABLE_SIZE | frame::SETTINGS_ENABLE_PUSH => {}
                _ => {}
            }
        }
        frame::write(&mut self.output, frame::SETTINGS, frame::FLAG_ACK, 0, &[]);
        Ok(())
    }

    fn on_window_update(&mut self, h: &Header, payload: &[u8]) -> Result<(), ConnectionError> {
        if payload.len() != 4 {
            return Err(self.connection_error(frame::FRAME_SIZE_ERROR));
        }
        let increment = i64::from(
            u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]) & 0x7fff_ffff,
        );
        if h.stream == 0 {
            if increment == 0 || self.send_window + increment > frame::MAX_WINDOW {
                return Err(self.connection_error(frame::FLOW_CONTROL_ERROR));
            }
            self.send_window += increment;
        } else if let Some(s) = self.streams.get_mut(&h.stream) {
            if increment == 0 || s.send_window + increment > frame::MAX_WINDOW {
                self.reset(h.stream, frame::FLOW_CONTROL_ERROR);
                self.events.push_back(Event::Closed {
                    stream: h.stream,
                    code: frame::FLOW_CONTROL_ERROR,
                });
            } else {
                s.send_window += increment;
            }
        }
        Ok(())
    }
}
