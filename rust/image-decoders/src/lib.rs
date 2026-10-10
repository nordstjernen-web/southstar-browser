//! Southstar — image decoders: ICO/CUR containers (pixels via the Wuffs chain) and WebP stills and animations over libwebp.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use ffi::libwebp::{self, AnimDecoder};
use ffi::{Buffer, Texture};

pub struct Decoded {
    pub texture: Texture,
    pub width: i32,
    pub height: i32,
}

pub struct Pixels {
    pub buffer: Buffer,
    pub width: i32,
    pub height: i32,
    pub stride: usize,
}

impl Pixels {
    pub fn into_texture(self) -> Option<Decoded> {
        let (width, height) = (self.width, self.height);
        let texture = Texture::new_premultiplied(width, height, self.buffer, self.stride)?;
        Some(Decoded {
            texture,
            width,
            height,
        })
    }
}

pub struct Frame {
    pub pixels: Pixels,
    pub delay_ms: i32,
}

const ICO_MAX_ENTRIES: u16 = 256;
const ICO_MAX_DIM: i32 = 1024;
const ICO_MAX_BMP: u64 = 64 * 1024 * 1024;

fn read_u16(p: &[u8]) -> u16 {
    u16::from_le_bytes([p[0], p[1]])
}

fn read_u32(p: &[u8]) -> u32 {
    u32::from_le_bytes([p[0], p[1], p[2], p[3]])
}

fn is_png(data: &[u8]) -> bool {
    data.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
}

fn decode_dib_entry(p: &[u8]) -> Option<Decoded> {
    let len = p.len() as u64;
    if len < 40 {
        return None;
    }
    let hdr = read_u32(p);
    if hdr < 40 || u64::from(hdr) > len {
        return None;
    }
    let bw = read_u32(&p[4..]) as i32;
    let bh = read_u32(&p[8..]) as i32;
    let bitcount = read_u16(&p[14..]);
    let compression = read_u32(&p[16..]);
    if bw <= 0 || bh <= 0 || bh & 1 != 0 || bw > ICO_MAX_DIM || bh > 2 * ICO_MAX_DIM {
        return None;
    }
    if compression != 0 || !matches!(bitcount, 1 | 4 | 8 | 24 | 32) {
        return None;
    }
    let w = bw as u32;
    let real_h = bh as u32 / 2;
    let clr_used = read_u32(&p[32..]);
    let pal_count = if bitcount <= 8 {
        if clr_used != 0 {
            clr_used
        } else {
            1 << bitcount
        }
    } else {
        0
    };
    if pal_count > 256 {
        return None;
    }
    let pal_bytes = u64::from(pal_count * 4);
    let xor_row = (u64::from(w) * u64::from(bitcount)).div_ceil(32) * 4;
    let xor_size = xor_row * u64::from(real_h);
    let pixel_off = u64::from(hdr) + pal_bytes;
    if pixel_off + xor_size > len {
        return None;
    }
    let data_off = 14 + u64::from(hdr) + pal_bytes;
    let bmp_len = data_off + xor_size;
    if data_off > u64::from(u32::MAX) || bmp_len > ICO_MAX_BMP {
        return None;
    }
    let (data_off, bmp_len, pixel_off, xor_size) = (
        data_off as usize,
        bmp_len as usize,
        pixel_off as usize,
        xor_size as usize,
    );
    let mut bmp = Vec::new();
    bmp.try_reserve_exact(bmp_len).ok()?;
    bmp.resize(bmp_len, 0);
    bmp[..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&(bmp_len as u32).to_le_bytes());
    bmp[10..14].copy_from_slice(&(data_off as u32).to_le_bytes());
    bmp[14..14 + pixel_off].copy_from_slice(&p[..pixel_off]);
    bmp[22..26].copy_from_slice(&real_h.to_le_bytes());
    bmp[34..38].fill(0);
    bmp[data_off..].copy_from_slice(&p[pixel_off..pixel_off + xor_size]);

    let mut pixels = ffi::wuffs_decode_to_bgra(&bmp)?;
    drop(bmp);

    let and_row = u64::from(w).div_ceil(32) * 4;
    let and_off = pixel_off as u64 + xor_size as u64;
    let and_size = and_row * u64::from(real_h);
    if pixels.width == w as i32 && pixels.height == real_h as i32 && and_off + and_size <= len {
        let (and_row, and_off) = (and_row as usize, and_off as usize);
        let stride = pixels.stride;
        let width = pixels.width as usize;
        for y in 0..pixels.height as usize {
            let mask = &p[and_off + (real_h as usize - 1 - y) * and_row..];
            let row = &mut pixels.buffer[y * stride..];
            for x in 0..width {
                if (mask[x >> 3] >> (7 - (x & 7))) & 1 != 0 {
                    row[x * 4..x * 4 + 4].fill(0);
                }
            }
        }
    }
    pixels.into_texture()
}

pub fn decode_ico(data: &[u8]) -> Option<Decoded> {
    if data.len() < 6 || read_u16(data) != 0 {
        return None;
    }
    let kind = read_u16(&data[2..]);
    if kind != 1 && kind != 2 {
        return None;
    }
    let count = read_u16(&data[4..]);
    if count == 0 || count > ICO_MAX_ENTRIES || 6 + usize::from(count) * 16 > data.len() {
        return None;
    }
    let mut best: Option<(usize, u64, u16)> = None;
    for i in 0..usize::from(count) {
        let e = &data[6 + i * 16..];
        let ew = if e[0] != 0 { u64::from(e[0]) } else { 256 };
        let eh = if e[1] != 0 { u64::from(e[1]) } else { 256 };
        let bits = read_u16(&e[6..]);
        let size = read_u32(&e[8..]);
        let off = read_u32(&e[12..]);
        if size == 0 || u64::from(off) + u64::from(size) > data.len() as u64 {
            continue;
        }
        let px = ew * eh;
        let better = match best {
            None => true,
            Some((_, best_px, best_bits)) => px > best_px || (px == best_px && bits > best_bits),
        };
        if better {
            best = Some((i, px, bits));
        }
    }
    let (index, _, _) = best?;
    let e = &data[6 + index * 16..];
    let size = read_u32(&e[8..]) as usize;
    let off = read_u32(&e[12..]) as usize;
    let payload = &data[off..off + size];
    if is_png(payload) {
        return ffi::wuffs_decode(payload);
    }
    decode_dib_entry(payload)
}

const WEBP_MAX_DIM: u32 = 16384;
const WEBP_MAX_PIXELS: u64 = 64 * 1024 * 1024;
const WEBP_MAX_INPUT: usize = 64 * 1024 * 1024;
const WEBP_MAX_FRAMES: u32 = 4096;
const WEBP_MAX_TOTAL_BYTES: usize = 512 * 1024 * 1024;

pub fn webp_supports(data: &[u8]) -> bool {
    data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP"
}

fn webp_canvas_ok(width: u32, height: u32) -> bool {
    width != 0
        && height != 0
        && width <= WEBP_MAX_DIM
        && height <= WEBP_MAX_DIM
        && u64::from(width) * u64::from(height) <= WEBP_MAX_PIXELS
}

fn webp_still(data: &[u8]) -> Option<Pixels> {
    if !webp_supports(data) || data.len() > WEBP_MAX_INPUT {
        return None;
    }
    let (w, h) = libwebp::info(data)?;
    if w <= 0 || h <= 0 || !webp_canvas_ok(w as u32, h as u32) {
        return None;
    }
    let stride = w as usize * 4;
    let mut buffer = Buffer::try_new(stride * h as usize)?;
    if !libwebp::decode_premultiplied_bgra(data, &mut buffer, stride) {
        return None;
    }
    Some(Pixels {
        buffer,
        width: w,
        height: h,
        stride,
    })
}

fn webp_animation_first_frame(data: &[u8]) -> Option<Pixels> {
    if !webp_supports(data) || data.len() > WEBP_MAX_INPUT {
        return None;
    }
    let mut decoder = AnimDecoder::new(data)?;
    let info = decoder.info()?;
    if !webp_canvas_ok(info.canvas_width, info.canvas_height) || !decoder.has_more_frames() {
        return None;
    }
    let (frame, _) = decoder.next_frame(info.canvas_width, info.canvas_height)?;
    let stride = info.canvas_width as usize * 4;
    let mut buffer = Buffer::try_new(stride * info.canvas_height as usize)?;
    buffer.copy_from_slice(frame);
    Some(Pixels {
        buffer,
        width: info.canvas_width as i32,
        height: info.canvas_height as i32,
        stride,
    })
}

pub fn decode_webp(data: &[u8]) -> Option<Pixels> {
    webp_still(data).or_else(|| webp_animation_first_frame(data))
}

pub fn decode_webp_animation(data: &[u8]) -> Option<(Vec<Frame>, i32, i32)> {
    if !webp_supports(data) || data.len() > WEBP_MAX_INPUT {
        return None;
    }
    let mut decoder = AnimDecoder::new(data)?;
    let info = decoder.info()?;
    if info.frame_count < 2
        || info.frame_count > WEBP_MAX_FRAMES
        || !webp_canvas_ok(info.canvas_width, info.canvas_height)
    {
        return None;
    }
    let (width, height) = (info.canvas_width as i32, info.canvas_height as i32);
    let stride = info.canvas_width as usize * 4;
    let size = stride * info.canvas_height as usize;
    let mut frames = Vec::new();
    let mut prev_ts = 0;
    let mut total = 0usize;
    while decoder.has_more_frames() {
        let (frame, ts) = decoder.next_frame(info.canvas_width, info.canvas_height)?;
        if total + size > WEBP_MAX_TOTAL_BYTES {
            break;
        }
        total += size;
        let mut buffer = Buffer::try_new(size)?;
        buffer.copy_from_slice(frame);
        frames.push(Frame {
            pixels: Pixels {
                buffer,
                width,
                height,
                stride,
            },
            delay_ms: ts - prev_ts,
        });
        prev_ts = ts;
    }
    (frames.len() >= 2).then_some((frames, width, height))
}
