//! Southstar — the pixel buffers the raster (PNG/GIF/BMP/JPEG), ICO and WebP decoders hand their pixels in.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(feature = "avif")]
pub(crate) mod libavif;
pub(crate) mod libwebp;

use core::ops::{Deref, DerefMut};

pub struct Buffer {
    bytes: Vec<u8>,
}

impl Buffer {
    pub fn try_new(len: usize) -> Option<Buffer> {
        if len == 0 {
            return None;
        }
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(len).ok()?;
        bytes.resize(len, 0);
        Some(Buffer { bytes })
    }
}

impl Deref for Buffer {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

impl DerefMut for Buffer {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}
