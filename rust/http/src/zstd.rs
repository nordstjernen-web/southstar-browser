//! Southstar — streaming zstd decoding of response bodies over ruzstd, with the window capped at 8 MiB.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use ruzstd::decoding::FrameDecoder;

use crate::inflate::{Emit, Failure, OUT_CHUNK};

const MAX_WINDOW_SIZE: u64 = 1 << 23;
const MAX_FRAME_HEADER: usize = 18;

pub struct Zstd {
    decoder: FrameDecoder,
    pending: Vec<u8>,
    in_frame: bool,
}

impl Zstd {
    pub fn new() -> Option<Zstd> {
        let mut decoder = FrameDecoder::new();
        decoder.set_max_window_size(MAX_WINDOW_SIZE);
        Some(Zstd {
            decoder,
            pending: Vec::new(),
            in_frame: false,
        })
    }

    fn start_frame(&mut self) -> Result<bool, Failure> {
        let mut header: &[u8] = &self.pending;
        match self.decoder.reset(&mut header) {
            Ok(()) => {
                let used = self.pending.len() - header.len();
                self.pending.drain(..used);
                self.in_frame = true;
                Ok(true)
            }
            Err(_) if self.pending.len() < MAX_FRAME_HEADER => Ok(false),
            Err(_) => Err(Failure::Corrupt),
        }
    }

    pub fn feed(&mut self, input: &[u8], emit: &mut Emit) -> Result<(), Failure> {
        self.pending.extend_from_slice(input);
        let mut out = [0u8; OUT_CHUNK];
        loop {
            if !self.in_frame && (self.pending.is_empty() || !self.start_frame()?) {
                return Ok(());
            }
            let (read, written) = self
                .decoder
                .decode_from_to(&self.pending, &mut out)
                .map_err(|_| Failure::Corrupt)?;
            self.pending.drain(..read.min(self.pending.len()));
            if written > 0 && !emit(&out[..written]) {
                return Err(Failure::Stopped);
            }
            if self.decoder.is_finished() && self.decoder.can_collect() == 0 {
                self.in_frame = false;
                continue;
            }
            if read == 0 && written == 0 {
                return Ok(());
            }
        }
    }
}
