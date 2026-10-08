//! Southstar — decoded-image texture: an immutable, reference-counted BGRA pixel buffer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub struct Pixels {
    pub width: i32,
    pub height: i32,
    pub stride: usize,
    pub bgra: Vec<u8>,
}

impl Pixels {
    pub fn copy_from(width: i32, height: i32, src: &[u8], stride: usize) -> Option<Pixels> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let needed = stride.checked_mul(height as usize)?;
        if src.len() < needed {
            return None;
        }
        Some(Pixels {
            width,
            height,
            stride,
            bgra: src[..needed].to_vec(),
        })
    }

    pub fn download(&self, dst: &mut [u8], dst_stride: usize) {
        let row = (self.width as usize * 4).min(dst_stride).min(self.stride);
        for y in 0..self.height as usize {
            let from = &self.bgra[y * self.stride..y * self.stride + row];
            dst[y * dst_stride..y * dst_stride + row].copy_from_slice(from);
        }
    }
}
