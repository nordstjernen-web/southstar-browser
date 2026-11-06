//! Southstar — the streaming deflate and inflate behind CompressionStream and DecompressionStream, over flate2's zlib-rs backend.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use flate2::{Compress, Compression, FlushCompress, Status};
use southstar_http::{Inflate, Inflated};
use southstar_js_engine::{Scope, Value};

pub(crate) enum Step {
    Progress,
    StreamEnd,
    BufError,
    Failed,
}

pub(crate) struct Deflate {
    stream: Compress,
}

impl Deflate {
    fn new(window_bits: i32) -> Option<Deflate> {
        let level = Compression::default();
        let stream = match window_bits {
            -15 => Compress::new_with_window_bits(level, false, 15),
            15 => Compress::new_with_window_bits(level, true, 15),
            31 => Compress::new_gzip(level, 15),
            _ => return None,
        };
        Some(Deflate { stream })
    }

    fn step(&mut self, input: &mut &[u8], out: &mut [u8], finish: bool) -> (Step, usize) {
        let (before_in, before_out) = (self.stream.total_in(), self.stream.total_out());
        let flush = if finish {
            FlushCompress::Finish
        } else {
            FlushCompress::None
        };
        let result = self.stream.compress(input, out, flush);
        let consumed = usize::try_from(self.stream.total_in() - before_in).unwrap_or(input.len());
        let produced = usize::try_from(self.stream.total_out() - before_out).unwrap_or(out.len());
        *input = &input[consumed.min(input.len())..];
        let step = match result {
            Ok(Status::Ok) => Step::Progress,
            Ok(Status::StreamEnd) => Step::StreamEnd,
            Ok(Status::BufError) => Step::BufError,
            Err(_) => Step::Failed,
        };
        (step, produced)
    }
}

pub(crate) enum Zlib {
    Inflate(Inflate),
    Deflate(Deflate),
}

impl Zlib {
    pub fn new(decompress: bool, window_bits: i32) -> Option<Zlib> {
        if decompress {
            Inflate::new(window_bits).map(Zlib::Inflate)
        } else {
            Deflate::new(window_bits).map(Zlib::Deflate)
        }
    }

    pub fn decompress(&self) -> bool {
        matches!(self, Zlib::Inflate(_))
    }

    pub fn step(&mut self, input: &mut &[u8], out: &mut [u8], finish: bool) -> (Step, usize) {
        match self {
            Zlib::Deflate(deflate) => deflate.step(input, out, finish),
            Zlib::Inflate(inflate) => {
                let (status, consumed, produced) = inflate.step(input, out, finish);
                *input = &input[consumed..];
                let step = match status {
                    Inflated::Progress => Step::Progress,
                    Inflated::StreamEnd => Step::StreamEnd,
                    Inflated::BufError => Step::BufError,
                    Inflated::Failed => Step::Failed,
                };
                (step, produced)
            }
        }
    }
}

pub(crate) fn internal_error(scope: &mut Scope<'_>, message: &str) -> Value {
    scope.internal_error(message)
}

pub fn nsr_compression_install(scope: &mut Scope<'_>, global: &Value) {
    crate::install(scope, global);
}
