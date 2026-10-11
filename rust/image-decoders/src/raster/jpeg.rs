//! Southstar — JPEG decoding over zune-jpeg: baseline and progressive, grayscale, YCbCr and CMYK/YCCK, to opaque BGRA.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use zune_jpeg::JpegDecoder;
use zune_jpeg::zune_core::bytestream::ZCursor;
use zune_jpeg::zune_core::colorspace::ColorSpace;
use zune_jpeg::zune_core::options::DecoderOptions;

use super::pixel::Canvas;
use super::{MAX_DIM, dims_ok};

pub fn decode(data: &[u8]) -> Option<Canvas> {
    let options = DecoderOptions::default()
        .set_max_width(MAX_DIM as usize)
        .set_max_height(MAX_DIM as usize)
        .set_strict_mode(true)
        .jpeg_set_out_colorspace(ColorSpace::RGB);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), options);
    decoder.decode_headers().ok()?;
    let (w, h) = decoder.dimensions()?;
    let (w, h) = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
    if !dims_ok(w, h) {
        return None;
    }
    let rgb = decoder.decode().ok()?;
    let mut canvas = Canvas::new(w, h)?;
    if rgb.len() < canvas.pixels.len() / 4 * 3 {
        return None;
    }
    for (d, s) in canvas
        .pixels
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(rgb.as_chunks::<3>().0.iter())
    {
        d.copy_from_slice(&[s[2], s[1], s[0], 0xFF]);
    }
    Some(canvas)
}
