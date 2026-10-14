//! Southstar — PNG stills and APNG animations over the png crate: every colour type and bit depth with tRNS transparency, interlacing, frame placement, disposal and delays.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::io::Cursor;

use png::{BitDepth, ColorType, Decoder, DisposeOp, Info, Limits, Reader, Transformations};

use super::pixel::{Blend, Canvas, Row, draw_row, premul_palette};
use super::{AnimFrame, Animator, Disposal, Rect};

fn open(data: &[u8]) -> Option<Reader<Cursor<&[u8]>>> {
    let mut decoder = Decoder::new_with_limits(Cursor::new(data), Limits { bytes: usize::MAX });
    decoder.set_transformations(Transformations::IDENTITY);
    decoder.read_info().ok()
}

enum Pixels {
    Indexed(Vec<u8>, Box<[[u8; 4]; 256]>),
    Rgba8(Vec<u8>),
    Rgba16(Vec<u16>),
}

struct Frame {
    width: u32,
    height: u32,
    pixels: Pixels,
}

impl Frame {
    fn row(&self, y: u32) -> Row<'_> {
        let w = self.width as usize;
        let y = y as usize;
        match &self.pixels {
            Pixels::Indexed(idx, pal) => Row::Indexed(&idx[y * w..(y + 1) * w], pal),
            Pixels::Rgba8(px) => Row::Rgba8(&px[y * w * 4..(y + 1) * w * 4]),
            Pixels::Rgba16(px) => Row::Rgba16(&px[y * w * 4..(y + 1) * w * 4]),
        }
    }
}

fn sample(row: &[u8], x: usize, depth: u8) -> u8 {
    match depth {
        8 => row[x],
        d => {
            let per = 8 / usize::from(d);
            let shift = 8 - usize::from(d) * (x % per + 1);
            (row[x / per] >> shift) & ((1u8 << d) - 1)
        }
    }
}

fn scale_gray(v: u8, depth: u8) -> u8 {
    match depth {
        1 => v * 0xFF,
        2 => v * 0x55,
        4 => v * 0x11,
        _ => v,
    }
}

fn be16(row: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([row[i * 2], row[i * 2 + 1]])
}

fn try_vec<T: Copy>(len: usize, fill: T) -> Option<Vec<T>> {
    let mut v = Vec::new();
    v.try_reserve_exact(len).ok()?;
    v.resize(len, fill);
    Some(v)
}

fn expand(info: &Info<'_>, width: u32, height: u32, buf: &[u8], stride: usize) -> Option<Frame> {
    let (w, h) = (width as usize, height as usize);
    let depth = info.bit_depth as u8;
    let trns = info.trns.as_deref();
    let pixels = match info.color_type {
        ColorType::Indexed => {
            let mut rgba = [[0u8, 0, 0, 0xFF]; 256];
            if let Some(pal) = info.palette.as_deref() {
                for (e, c) in rgba.iter_mut().zip(pal.as_chunks::<3>().0.iter()) {
                    *e = [c[0], c[1], c[2], 0xFF];
                }
            }
            if let Some(t) = trns {
                for (e, &a) in rgba.iter_mut().zip(t.iter()) {
                    e[3] = a;
                }
            }
            let mut idx = try_vec(w * h, 0u8)?;
            for y in 0..h {
                let row = &buf[y * stride..];
                for x in 0..w {
                    idx[y * w + x] = sample(row, x, depth);
                }
            }
            Pixels::Indexed(idx, Box::new(premul_palette(&rgba)))
        }
        _ if info.bit_depth == BitDepth::Sixteen => {
            let mut px = try_vec(w * h * 4, 0u16)?;
            for y in 0..h {
                let row = &buf[y * stride..];
                for x in 0..w {
                    let o = &mut px[(y * w + x) * 4..(y * w + x) * 4 + 4];
                    match info.color_type {
                        ColorType::Grayscale => {
                            let v = be16(row, x);
                            let opaque = trns.is_none_or(|t| t.len() < 2 || be16(t, 0) != v);
                            o.copy_from_slice(&[v, v, v, if opaque { 0xFFFF } else { 0 }]);
                        }
                        ColorType::GrayscaleAlpha => {
                            let v = be16(row, x * 2);
                            o.copy_from_slice(&[v, v, v, be16(row, x * 2 + 1)]);
                        }
                        ColorType::Rgb => {
                            let (r, g, b) =
                                (be16(row, x * 3), be16(row, x * 3 + 1), be16(row, x * 3 + 2));
                            let opaque = trns.is_none_or(|t| {
                                t.len() < 6 || (be16(t, 0), be16(t, 1), be16(t, 2)) != (r, g, b)
                            });
                            o.copy_from_slice(&[r, g, b, if opaque { 0xFFFF } else { 0 }]);
                        }
                        _ => {
                            for (c, v) in o.iter_mut().enumerate() {
                                *v = be16(row, x * 4 + c);
                            }
                        }
                    }
                }
            }
            Pixels::Rgba16(px)
        }
        _ => {
            let mut px = try_vec(w * h * 4, 0u8)?;
            for y in 0..h {
                let row = &buf[y * stride..];
                for x in 0..w {
                    let o = &mut px[(y * w + x) * 4..(y * w + x) * 4 + 4];
                    match info.color_type {
                        ColorType::Grayscale => {
                            let raw = sample(row, x, depth);
                            let v = scale_gray(raw, depth);
                            let opaque = trns.is_none_or(|t| t.first() != Some(&raw));
                            o.copy_from_slice(&[v, v, v, if opaque { 0xFF } else { 0 }]);
                        }
                        ColorType::GrayscaleAlpha => {
                            o.copy_from_slice(&[
                                row[x * 2],
                                row[x * 2],
                                row[x * 2],
                                row[x * 2 + 1],
                            ]);
                        }
                        ColorType::Rgb => {
                            let (r, g, b) = (row[x * 3], row[x * 3 + 1], row[x * 3 + 2]);
                            let opaque = trns.is_none_or(|t| t.get(..3) != Some(&[r, g, b][..]));
                            o.copy_from_slice(&[r, g, b, if opaque { 0xFF } else { 0 }]);
                        }
                        _ => o.copy_from_slice(&row[x * 4..x * 4 + 4]),
                    }
                }
            }
            Pixels::Rgba8(px)
        }
    };
    Some(Frame {
        width,
        height,
        pixels,
    })
}

fn next_frame(reader: &mut Reader<Cursor<&[u8]>>, buf: &mut [u8]) -> Option<Frame> {
    let out = reader.next_frame(buf).ok()?;
    let info = reader.info();
    let stride = if info.interlaced {
        info.raw_row_length_from_width(info.width) - 1
    } else {
        out.line_size
    };
    expand(info, out.width, out.height, buf, stride)
}

fn draw(canvas: &mut Canvas, frame: &Frame, x0: u32, y0: u32, blend: Blend) {
    if x0 >= canvas.width {
        return;
    }
    let x1 = x0.saturating_add(frame.width).min(canvas.width);
    for y in 0..frame.height {
        let cy = y0.saturating_add(y);
        if cy >= canvas.height {
            break;
        }
        let dst = &mut canvas.row_mut(cy)[x0 as usize * 4..x1 as usize * 4];
        draw_row(dst, &frame.row(y), blend);
    }
}

pub fn decode_still(data: &[u8]) -> Option<Canvas> {
    let mut reader = open(data)?;
    let (w, h) = (reader.info().width, reader.info().height);
    let mut canvas = Canvas::new(w, h)?;
    let mut buf = try_vec(reader.output_buffer_size()?, 0u8)?;
    let mut frame = next_frame(&mut reader, &mut buf)?;
    let (mut x0, mut y0) = (0, 0);
    if reader.info().animation_control.is_some() && reader.info().frame_control.is_none() {
        frame = next_frame(&mut reader, &mut buf)?;
        let fc = reader.info().frame_control?;
        (x0, y0) = (fc.x_offset, fc.y_offset);
    }
    draw(&mut canvas, &frame, x0, y0, Blend::Src);
    Some(canvas)
}

pub fn decode_animation(data: &[u8]) -> Option<Vec<AnimFrame>> {
    let mut reader = open(data)?;
    let (w, h) = (reader.info().width, reader.info().height);
    let mut anim = Animator::new(w, h)?;
    let mut buf = try_vec(reader.output_buffer_size()?, 0u8)?;
    while !anim.full() {
        let Some(frame) = next_frame(&mut reader, &mut buf) else {
            break;
        };
        let Some(fc) = reader.info().frame_control else {
            continue;
        };
        let disposal = match fc.dispose_op {
            DisposeOp::None => Disposal::None,
            DisposeOp::Background => Disposal::Background,
            DisposeOp::Previous => Disposal::Previous,
        };
        anim.begin_frame(disposal);
        draw(
            &mut anim.canvas,
            &frame,
            fc.x_offset,
            fc.y_offset,
            Blend::Over,
        );
        let rect = Rect {
            x0: fc.x_offset.min(w),
            y0: fc.y_offset.min(h),
            x1: fc.x_offset.saturating_add(fc.width).min(w),
            y1: fc.y_offset.saturating_add(fc.height).min(h),
        };
        let flicks = if fc.delay_den == 0 {
            u64::from(fc.delay_num) * 7_056_000
        } else {
            u64::from(fc.delay_num) * 705_600_000 / u64::from(fc.delay_den)
        };
        if !anim.end_frame(disposal, rect, flicks) {
            break;
        }
    }
    anim.finish()
}
