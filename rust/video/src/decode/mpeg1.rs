//! Southstar — the MPEG-1 backend: the in-tree pl_mpeg port over an in-memory stream.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_plmpeg::Plm;

use super::{Next, pixel_buffer};
use crate::ffi::texture_from_bgra;

pub struct Mpeg1 {
    plm: Plm,
}

impl Mpeg1 {
    pub fn open(stream: Vec<u8>) -> Option<Mpeg1> {
        let mut plm = Plm::new(stream);
        plm.set_audio_enabled(false);
        plm.set_loop(false);
        Some(Mpeg1 { plm })
    }

    pub fn width(&mut self) -> i32 {
        self.plm.width()
    }

    pub fn height(&mut self) -> i32 {
        self.plm.height()
    }

    pub fn audio_streams(&mut self) -> i32 {
        self.plm.num_audio_streams()
    }

    pub fn duration(&mut self) -> f64 {
        self.plm.duration()
    }

    pub fn rewind(&mut self) {
        self.plm.rewind();
    }

    pub fn next(&mut self, w: i32, h: i32) -> Next {
        let Some(frame) = self.plm.decode_video() else {
            return if self.plm.has_ended() {
                Next::Ended
            } else {
                Next::Stalled
            };
        };
        let stride = w as usize * 4;
        let tex = pixel_buffer(stride * h as usize, 0xFF).and_then(|mut bgra| {
            frame.to_bgra(&mut bgra, stride);
            texture_from_bgra(w, h, &bgra, stride)
        });
        Next::Frame(tex, frame.time)
    }
}
