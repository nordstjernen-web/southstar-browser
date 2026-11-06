//! Southstar — streaming inflate of zlib, gzip and raw deflate data over flate2's zlib-rs backend, with zlib's window-bit conventions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use flate2::{Decompress, FlushDecompress, Status};

pub enum Failure {
    Corrupt,
    Stopped,
}

pub type Emit<'a> = dyn FnMut(&[u8]) -> bool + 'a;

pub const OUT_CHUNK: usize = 16_384;

const GZIP_MAGIC: u8 = 0x1f;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Inflated {
    Progress,
    StreamEnd,
    BufError,
    Failed,
}

enum Stream {
    Detect(u8),
    Ready(Decompress),
}

pub struct Inflate {
    stream: Stream,
    finished: bool,
}

fn window(bits: i32) -> Option<u8> {
    u8::try_from(bits)
        .ok()
        .filter(|bits| (9..=15).contains(bits))
}

fn decompressor(window_bits: i32) -> Option<Decompress> {
    match window_bits {
        bits if bits < 0 => Some(Decompress::new_with_window_bits(false, window(-bits)?)),
        bits if bits > 31 => None,
        bits if bits > 15 => Some(Decompress::new_gzip(window(bits - 16)?)),
        bits => Some(Decompress::new_with_window_bits(true, window(bits)?)),
    }
}

impl Inflate {
    pub fn new(window_bits: i32) -> Option<Inflate> {
        let stream = if window_bits > 31 {
            Stream::Detect(window(window_bits - 32)?)
        } else {
            Stream::Ready(decompressor(window_bits)?)
        };
        Some(Inflate {
            stream,
            finished: false,
        })
    }

    fn ready(&mut self, first: u8) -> Option<&mut Decompress> {
        if let Stream::Detect(bits) = self.stream {
            let bits = i32::from(bits);
            let wrapped = if first == GZIP_MAGIC { bits + 16 } else { bits };
            self.stream = Stream::Ready(decompressor(wrapped)?);
        }
        match &mut self.stream {
            Stream::Ready(decompress) => Some(decompress),
            Stream::Detect(_) => None,
        }
    }

    pub fn step(&mut self, input: &[u8], out: &mut [u8], finish: bool) -> (Inflated, usize, usize) {
        let Some(&first) = input.first() else {
            if let Stream::Detect(_) = self.stream {
                return (Inflated::BufError, 0, 0);
            }
            return self.run(input, out, finish);
        };
        if self.ready(first).is_none() {
            return (Inflated::Failed, 0, 0);
        }
        self.run(input, out, finish)
    }

    fn run(&mut self, input: &[u8], out: &mut [u8], finish: bool) -> (Inflated, usize, usize) {
        let Stream::Ready(decompress) = &mut self.stream else {
            return (Inflated::Failed, 0, 0);
        };
        let (before_in, before_out) = (decompress.total_in(), decompress.total_out());
        let flush = if finish {
            FlushDecompress::Finish
        } else {
            FlushDecompress::None
        };
        let result = decompress.decompress(input, out, flush);
        let consumed = usize::try_from(decompress.total_in() - before_in).unwrap_or(input.len());
        let produced = usize::try_from(decompress.total_out() - before_out).unwrap_or(out.len());
        let status = match result {
            Ok(Status::Ok) => Inflated::Progress,
            Ok(Status::StreamEnd) => Inflated::StreamEnd,
            Ok(Status::BufError) => Inflated::BufError,
            Err(_) => Inflated::Failed,
        };
        (status, consumed, produced)
    }

    pub fn feed(&mut self, input: &[u8], emit: &mut Emit) -> Result<usize, Failure> {
        if self.finished {
            return Ok(input.len());
        }
        let mut out = [0u8; OUT_CHUNK];
        let mut rest = input;
        loop {
            let (status, consumed, produced) = self.step(rest, &mut out, false);
            rest = &rest[consumed..];
            if produced > 0 && !emit(&out[..produced]) {
                return Err(Failure::Stopped);
            }
            match status {
                Inflated::StreamEnd => {
                    self.finished = true;
                    return Ok(rest.len());
                }
                Inflated::Progress | Inflated::BufError => {
                    if rest.is_empty() && produced < OUT_CHUNK {
                        return Ok(0);
                    }
                    if consumed == 0 && produced == 0 {
                        return Err(Failure::Corrupt);
                    }
                }
                Inflated::Failed => return Err(Failure::Corrupt),
            }
        }
    }
}
