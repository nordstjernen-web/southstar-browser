//! Southstar — unit tests for the raster decoders on hand-built BMP and GIF files.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::{decode, decode_animation, dims_ok, supports};

fn bmp(width: i32, height: i32, bpp: u16, palette: &[[u8; 4]], rows: &[&[u8]]) -> Vec<u8> {
    let offset = 14 + 40 + palette.len() * 4;
    let stride = (usize::from(bpp) * width.unsigned_abs() as usize).div_ceil(32) * 4;
    let mut file = Vec::new();
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&((offset + stride * rows.len()) as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&(offset as u32).to_le_bytes());
    file.extend_from_slice(&40u32.to_le_bytes());
    file.extend_from_slice(&width.to_le_bytes());
    file.extend_from_slice(&height.to_le_bytes());
    file.extend_from_slice(&1u16.to_le_bytes());
    file.extend_from_slice(&bpp.to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&((stride * rows.len()) as u32).to_le_bytes());
    file.extend_from_slice(&[0; 8]);
    file.extend_from_slice(&(palette.len() as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    for entry in palette {
        file.extend_from_slice(entry);
    }
    for row in rows {
        let mut padded = row.to_vec();
        padded.resize(stride, 0);
        file.extend_from_slice(&padded);
    }
    file
}

fn pixel(canvas: &crate::Canvas, x: usize, y: usize) -> [u8; 4] {
    let at = y * canvas.stride() + x * 4;
    canvas.pixels[at..at + 4].try_into().expect("four channels")
}

#[test]
fn decodes_a_bottom_up_24_bit_bmp() {
    let file = bmp(
        2,
        2,
        24,
        &[],
        &[&[255, 0, 0, 0, 255, 0], &[0, 0, 255, 255, 255, 255]],
    );
    assert!(supports(&file));
    let canvas = decode(&file).expect("24-bit BMP decodes");
    assert_eq!((canvas.width, canvas.height), (2, 2));
    assert_eq!(pixel(&canvas, 0, 0), [0, 0, 255, 255]);
    assert_eq!(pixel(&canvas, 1, 0), [255, 255, 255, 255]);
    assert_eq!(pixel(&canvas, 0, 1), [255, 0, 0, 255]);
    assert_eq!(pixel(&canvas, 1, 1), [0, 255, 0, 255]);
}

#[test]
fn decodes_a_top_down_8_bit_palette_bmp() {
    let palette = [[10, 20, 30, 0], [40, 50, 60, 0]];
    let file = bmp(3, -1, 8, &palette, &[&[1, 0, 1]]);
    let canvas = decode(&file).expect("8-bit BMP decodes");
    assert_eq!((canvas.width, canvas.height), (3, 1));
    assert_eq!(pixel(&canvas, 0, 0), [40, 50, 60, 255]);
    assert_eq!(pixel(&canvas, 1, 0), [10, 20, 30, 255]);
}

#[test]
fn rejects_bad_dimensions_and_truncated_files() {
    assert!(decode(&bmp(0, 1, 24, &[], &[])).is_none());
    assert!(decode(&bmp(1, 100_000, 24, &[], &[&[1, 2, 3]])).is_none());
    let file = bmp(
        2,
        2,
        24,
        &[],
        &[&[1, 2, 3, 4, 5, 6], &[7, 8, 9, 10, 11, 12]],
    );
    for len in 0..file.len() {
        let _ = decode(&file[..len]);
    }
    assert!(!dims_ok(0, 1));
    assert!(!dims_ok(16385, 1));
    assert!(!dims_ok(16384, 16384));
    assert!(dims_ok(8192, 8192));
}

#[test]
fn decodes_a_one_pixel_gif() {
    let gif = [
        b'G', b'I', b'F', b'8', b'9', b'a', 1, 0, 1, 0, 0x80, 0, 0, 0xff, 0, 0, 0, 0, 0xff, 0x2c,
        0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 1, 0, 0x3b,
    ];
    let canvas = decode(&gif).expect("GIF decodes");
    assert_eq!((canvas.width, canvas.height), (1, 1));
    assert_eq!(pixel(&canvas, 0, 0), [0, 0, 255, 255]);
    let frames = decode_animation(&gif).expect("a still GIF is a one-frame animation");
    assert_eq!(frames.len(), 1);
}

#[test]
fn ignores_unknown_formats() {
    assert!(!supports(b"RIFF\0\0\0\0WEBP"));
    assert!(decode(b"not an image").is_none());
    assert!(decode(&[]).is_none());
}

#[test]
fn rejects_a_gif_frame_beyond_the_size_limits() {
    let mut gif = vec![
        b'G', b'I', b'F', b'8', b'9', b'a', 1, 0, 1, 0, 0x80, 0, 0, 0xff, 0, 0, 0, 0, 0xff, 0x2c,
        0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 1, 0, 0x3b,
    ];
    gif[24..28].copy_from_slice(&[0xff, 0xff, 0xff, 0xff]);
    assert!(decode(&gif).is_none());
    assert!(decode_animation(&gif).is_none());
}
