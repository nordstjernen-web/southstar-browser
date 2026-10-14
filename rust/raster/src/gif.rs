//! Southstar — GIF stills and animations over the gif crate: palettes with the transparent index, interlacing, frame placement, disposal and partially decoded frames.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::io::Cursor;

use gif::{ColorOutput, DecodeOptions, Decoder, DisposalMethod, MemoryLimit};

use super::pixel::{Blend, Canvas, Row, draw_row, premul_palette};
use super::{AnimFrame, Animator, Disposal, Rect};

const MEMORY_LIMIT: u64 = 1 << 30;

fn open(data: &[u8]) -> Option<Decoder<Cursor<&[u8]>>> {
    let mut options = DecodeOptions::new();
    options.set_color_output(ColorOutput::Indexed);
    options.set_memory_limit(MemoryLimit::Bytes(MEMORY_LIMIT.try_into().ok()?));
    options.read_info(Cursor::new(data)).ok()
}

struct FrameData {
    left: u32,
    top: u32,
    width: u32,
    height: u32,
    indexes: Vec<u8>,
    filled: Vec<u32>,
    palette: [[u8; 4]; 256],
    delay_cs: u16,
    disposal: Disposal,
    truncated: bool,
}

fn row_order(height: u32, interlaced: bool) -> Vec<u32> {
    if !interlaced {
        return (0..height).collect();
    }
    let mut rows = Vec::with_capacity(height as usize);
    for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
        let mut y = start;
        while y < height {
            rows.push(y);
            y += step;
        }
    }
    rows
}

enum RowFill {
    Full,
    Ended,
    Failed,
}

fn fill_row(decoder: &mut Decoder<Cursor<&[u8]>>, line: &mut [u8]) -> RowFill {
    match decoder.fill_buffer(line) {
        Ok(true) => RowFill::Full,
        Ok(false) => RowFill::Ended,
        Err(_) => RowFill::Failed,
    }
}

fn partial_row(data: &[u8], frame_index: usize, rows_before: usize, width: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let Some(mut decoder) = open(data) else {
        return out;
    };
    for _ in 0..frame_index {
        if !matches!(decoder.next_frame_info(), Ok(Some(_))) {
            return out;
        }
    }
    if !matches!(decoder.next_frame_info(), Ok(Some(_))) {
        return out;
    }
    let mut line = vec![0u8; width];
    for _ in 0..rows_before {
        if !matches!(fill_row(&mut decoder, &mut line), RowFill::Full) {
            return out;
        }
    }
    let mut px = [0u8; 1];
    while out.len() < width {
        if !matches!(fill_row(&mut decoder, &mut px), RowFill::Full) {
            break;
        }
        out.push(px[0]);
    }
    out
}

fn read_frame(
    decoder: &mut Decoder<Cursor<&[u8]>>,
    data: &[u8],
    frame_index: usize,
) -> Option<FrameData> {
    let frame = decoder.next_frame_info().ok()??;
    let (left, top) = (u32::from(frame.left), u32::from(frame.top));
    let (width, height) = (u32::from(frame.width), u32::from(frame.height));
    let interlaced = frame.interlaced;
    let transparent = frame.transparent;
    let delay_cs = frame.delay;
    let disposal = match frame.dispose {
        DisposalMethod::Background => Disposal::Background,
        DisposalMethod::Previous => Disposal::Previous,
        _ => Disposal::None,
    };
    let local = frame.palette.clone();
    let colors = local.as_deref().or_else(|| decoder.global_palette())?;
    let mut rgba = [[0u8, 0, 0, 0xFF]; 256];
    for (entry, c) in rgba.iter_mut().zip(colors.as_chunks::<3>().0.iter()) {
        *entry = [c[0], c[1], c[2], 0xFF];
    }
    if let Some(t) = transparent {
        rgba[usize::from(t)] = [0, 0, 0, 0];
    }
    let palette = premul_palette(&rgba);
    let w = width as usize;
    let mut indexes = Vec::new();
    indexes.try_reserve_exact(w * height as usize).ok()?;
    indexes.resize(w * height as usize, 0);
    let mut filled = vec![0u32; height as usize];
    let mut truncated = false;
    if w > 0 {
        for (n, y) in row_order(height, interlaced).into_iter().enumerate() {
            let start = y as usize * w;
            match fill_row(decoder, &mut indexes[start..start + w]) {
                RowFill::Full => filled[y as usize] = width,
                _ => {
                    truncated = true;
                    let partial = partial_row(data, frame_index, n, w);
                    indexes[start..start + partial.len()].copy_from_slice(&partial);
                    filled[y as usize] = partial.len() as u32;
                    break;
                }
            }
        }
    }
    Some(FrameData {
        left,
        top,
        width,
        height,
        indexes,
        filled,
        palette,
        delay_cs,
        disposal,
        truncated,
    })
}

fn frame_rect(f: &FrameData, canvas: &Canvas) -> Rect {
    Rect {
        x0: f.left.min(canvas.width),
        y0: f.top.min(canvas.height),
        x1: f.left.saturating_add(f.width).min(canvas.width),
        y1: f.top.saturating_add(f.height).min(canvas.height),
    }
}

fn draw(canvas: &mut Canvas, f: &FrameData, blend: Blend) {
    let r = frame_rect(f, canvas);
    if r.x0 >= r.x1 {
        return;
    }
    for y in 0..f.height {
        let cy = f.top.saturating_add(y);
        let cols = f.filled[y as usize].min(r.x1 - r.x0) as usize;
        if cy >= canvas.height || cols == 0 {
            continue;
        }
        let start = y as usize * f.width as usize;
        let src = &f.indexes[start..start + cols];
        let x0 = r.x0 as usize * 4;
        let dst = &mut canvas.row_mut(cy)[x0..x0 + cols * 4];
        draw_row(dst, &Row::Indexed(src, &f.palette), blend);
    }
}

fn image_size(decoder: &Decoder<Cursor<&[u8]>>, first: &FrameData) -> (u32, u32) {
    (
        u32::from(decoder.width()).max(first.left.saturating_add(first.width)),
        u32::from(decoder.height()).max(first.top.saturating_add(first.height)),
    )
}

pub fn decode_still(data: &[u8]) -> Option<Canvas> {
    let mut decoder = open(data)?;
    let frame = read_frame(&mut decoder, data, 0)?;
    let (w, h) = image_size(&decoder, &frame);
    let mut canvas = Canvas::new(w, h)?;
    draw(&mut canvas, &frame, Blend::Src);
    Some(canvas)
}

pub fn decode_animation(data: &[u8]) -> Option<Vec<AnimFrame>> {
    let mut decoder = open(data)?;
    let mut next = read_frame(&mut decoder, data, 0);
    let (w, h) = image_size(&decoder, next.as_ref()?);
    let mut anim = Animator::new(w, h)?;
    let mut index = 0;
    while !anim.full() {
        let Some(frame) = next.take() else {
            break;
        };
        anim.begin_frame(frame.disposal);
        if frame.truncated {
            break;
        }
        draw(&mut anim.canvas, &frame, Blend::Over);
        let rect = frame_rect(&frame, &anim.canvas);
        if !anim.end_frame(frame.disposal, rect, u64::from(frame.delay_cs) * 7_056_000) {
            break;
        }
        index += 1;
        next = read_frame(&mut decoder, data, index);
    }
    anim.finish()
}
