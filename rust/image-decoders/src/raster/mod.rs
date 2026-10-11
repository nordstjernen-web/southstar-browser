//! Southstar — PNG/APNG, GIF, BMP and JPEG decoding to premultiplied BGRA canvases, still and animated.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod bmp;
mod gif;
mod jpeg;
mod pixel;
mod png;

pub use pixel::{Canvas, premultiply_bgra};

pub const MAX_DIM: u32 = 16384;
pub const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const MAX_FRAMES: usize = 1024;
const MAX_ANIM_BYTES: usize = 512 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Png,
    Gif,
    Bmp,
    Jpeg,
}

fn detect(data: &[u8]) -> Option<Format> {
    if data.len() < 4 {
        return None;
    }
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        return Some(Format::Png);
    }
    if data.len() >= 6
        && data.starts_with(b"GIF8")
        && matches!(data[4], b'7' | b'9')
        && data[5] == b'a'
    {
        return Some(Format::Gif);
    }
    if data.starts_with(b"BM") {
        return Some(Format::Bmp);
    }
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(Format::Jpeg);
    }
    None
}

pub fn supports(data: &[u8]) -> bool {
    detect(data).is_some()
}

pub fn dims_ok(width: u32, height: u32) -> bool {
    width != 0
        && height != 0
        && width <= MAX_DIM
        && height <= MAX_DIM
        && u64::from(width) * u64::from(height) <= MAX_PIXELS
}

pub fn decode(data: &[u8]) -> Option<Canvas> {
    match detect(data)? {
        Format::Png => png::decode_still(data),
        Format::Gif => gif::decode_still(data),
        Format::Bmp => bmp::decode(data),
        Format::Jpeg => jpeg::decode(data),
    }
}

pub struct AnimFrame {
    pub canvas: Canvas,
    pub delay_ms: i32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Disposal {
    None,
    Background,
    Previous,
}

#[derive(Clone, Copy)]
pub struct Rect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

pub struct Animator {
    canvas: Canvas,
    backup: Option<Vec<u8>>,
    prev_disposal: Disposal,
    prev_rect: Rect,
    frames: Vec<AnimFrame>,
    total_bytes: usize,
}

impl Animator {
    fn new(width: u32, height: u32) -> Option<Animator> {
        Some(Animator {
            canvas: Canvas::new(width, height)?,
            backup: None,
            prev_disposal: Disposal::None,
            prev_rect: Rect {
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
            },
            frames: Vec::new(),
            total_bytes: 0,
        })
    }

    fn full(&self) -> bool {
        self.frames.len() >= MAX_FRAMES
    }

    fn begin_frame(&mut self, disposal: Disposal) {
        match self.prev_disposal {
            Disposal::Background => self.canvas.clear_rect(self.prev_rect),
            Disposal::Previous => {
                if let Some(backup) = &self.backup {
                    self.canvas.pixels.copy_from_slice(backup);
                }
            }
            Disposal::None => {}
        }
        if disposal == Disposal::Previous {
            if self.backup.is_none() {
                let mut backup = Vec::new();
                if backup.try_reserve_exact(self.canvas.pixels.len()).is_ok() {
                    backup.extend_from_slice(&self.canvas.pixels);
                    self.backup = Some(backup);
                }
            } else if let Some(backup) = &mut self.backup {
                backup.copy_from_slice(&self.canvas.pixels);
            }
        }
    }

    fn end_frame(&mut self, disposal: Disposal, rect: Rect, flicks: u64) -> bool {
        self.prev_disposal = disposal;
        self.prev_rect = rect;
        let bytes = self.canvas.pixels.len();
        self.total_bytes += bytes;
        if self.total_bytes > MAX_ANIM_BYTES {
            return false;
        }
        let mut copy = Vec::new();
        if copy.try_reserve_exact(bytes).is_err() {
            return false;
        }
        copy.extend_from_slice(&self.canvas.pixels);
        let ms = (flicks / 705_600) as i32;
        let delay_ms = if ms <= 0 { 100 } else { ms };
        self.frames.push(AnimFrame {
            canvas: Canvas {
                width: self.canvas.width,
                height: self.canvas.height,
                pixels: copy,
            },
            delay_ms,
        });
        true
    }

    fn finish(self) -> Option<Vec<AnimFrame>> {
        (!self.frames.is_empty()).then_some(self.frames)
    }
}

pub fn png_is_animated(data: &[u8]) -> bool {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    if data.len() < 16 || data[..8] != SIG {
        return false;
    }
    let mut pos = 8usize;
    while pos + 8 <= data.len() {
        let size = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let tag = &data[pos + 4..pos + 8];
        if tag == b"IDAT" {
            return false;
        }
        if tag == b"acTL" {
            return true;
        }
        if tag == b"IEND" {
            return false;
        }
        if size as usize > data.len() - pos - 8 {
            return false;
        }
        pos += 12 + size as usize;
    }
    false
}

pub fn decode_animation(data: &[u8]) -> Option<Vec<AnimFrame>> {
    if data.len() < 6 {
        return None;
    }
    match detect(data)? {
        Format::Gif => gif::decode_animation(data),
        Format::Png if png_is_animated(data) => png::decode_animation(data),
        _ => None,
    }
}
