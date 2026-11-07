//! Southstar — streaming brotli decoding of response bodies over brotli-decompressor.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use brotli_decompressor::{BrotliDecompressStream, BrotliResult, BrotliState, StandardAlloc};

use crate::inflate::{Emit, Failure, OUT_CHUNK};

pub struct Brotli {
    state: BrotliState<StandardAlloc, StandardAlloc, StandardAlloc>,
    finished: bool,
}

impl Brotli {
    pub fn new() -> Option<Brotli> {
        Some(Brotli {
            state: BrotliState::new(
                StandardAlloc::default(),
                StandardAlloc::default(),
                StandardAlloc::default(),
            ),
            finished: false,
        })
    }

    pub fn feed(&mut self, input: &[u8], emit: &mut Emit) -> Result<(), Failure> {
        let mut available_in = input.len();
        let mut input_offset = 0;
        let mut out = [0u8; OUT_CHUNK];
        while !self.finished {
            let mut available_out = OUT_CHUNK;
            let mut output_offset = 0;
            let mut total_out = 0;
            let result = BrotliDecompressStream(
                &mut available_in,
                &mut input_offset,
                input,
                &mut available_out,
                &mut output_offset,
                &mut out,
                &mut total_out,
                &mut self.state,
            );
            if output_offset > 0 && !emit(&out[..output_offset]) {
                return Err(Failure::Stopped);
            }
            match result {
                BrotliResult::ResultSuccess => self.finished = true,
                BrotliResult::NeedsMoreInput => return Ok(()),
                BrotliResult::NeedsMoreOutput => {}
                BrotliResult::ResultFailure => return Err(Failure::Corrupt),
            }
        }
        Ok(())
    }
}
