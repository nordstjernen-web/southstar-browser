//! Southstar — WebP stills and animations over image-webp, as premultiplied BGRA canvases.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::io::Cursor;

use image_webp::WebPDecoder;

use crate::raster;

pub struct Info {
    pub width: u32,
    pub height: u32,
    pub animated: bool,
    pub frame_count: u32,
}

pub struct Decoder<'a> {
    inner: WebPDecoder<Cursor<&'a [u8]>>,
    scratch: Vec<u8>,
}

impl<'a> Decoder<'a> {
    pub fn new(data: &'a [u8]) -> Option<Decoder<'a>> {
        let inner = WebPDecoder::new(Cursor::new(data)).ok()?;
        Some(Decoder {
            inner,
            scratch: Vec::new(),
        })
    }

    pub fn info(&self) -> Info {
        let (width, height) = self.inner.dimensions();
        Info {
            width,
            height,
            animated: self.inner.is_animated(),
            frame_count: self.inner.num_frames(),
        }
    }

    fn prepare(&mut self) -> bool {
        let Some(size) = self.inner.output_buffer_size() else {
            return false;
        };
        self.scratch.clear();
        if self.scratch.try_reserve_exact(size).is_err() {
            return false;
        }
        self.scratch.resize(size, 0);
        true
    }

    fn convert(&self, out: &mut [u8]) {
        let source_channels = if self.inner.has_alpha() { 4 } else { 3 };
        for (src, dst) in self
            .scratch
            .chunks_exact(source_channels)
            .zip(out.chunks_exact_mut(4))
        {
            dst[0] = src[2];
            dst[1] = src[1];
            dst[2] = src[0];
            dst[3] = if source_channels == 4 { src[3] } else { 255 };
        }
        raster::premultiply_bgra(out);
    }

    pub fn still(&mut self, out: &mut [u8]) -> bool {
        if !self.prepare() || self.inner.read_image(&mut self.scratch).is_err() {
            return false;
        }
        self.convert(out);
        true
    }

    pub fn next_frame(&mut self, out: &mut [u8]) -> Option<u32> {
        if !self.prepare() {
            return None;
        }
        let delay_ms = self.inner.read_frame(&mut self.scratch).ok()?;
        self.convert(out);
        Some(delay_ms)
    }
}
