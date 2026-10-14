//! Southstar — the premultiplied BGRA canvas decoders draw into, with the 8- and 16-bit premultiplication and source-over arithmetic Wuffs used, so decoded pixels stay bit-identical.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::Rect;

pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Option<Canvas> {
        if !super::dims_ok(width, height) {
            return None;
        }
        let len = width as usize * height as usize * 4;
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(len).ok()?;
        pixels.resize(len, 0);
        Some(Canvas {
            width,
            height,
            pixels,
        })
    }

    pub fn stride(&self) -> usize {
        self.width as usize * 4
    }

    pub fn row_mut(&mut self, y: u32) -> &mut [u8] {
        let stride = self.stride();
        let start = y as usize * stride;
        &mut self.pixels[start..start + stride]
    }

    pub fn clear_rect(&mut self, r: Rect) {
        let x1 = r.x1.min(self.width);
        let y1 = r.y1.min(self.height);
        for y in r.y0..y1 {
            if r.x0 < x1 {
                let row = self.row_mut(y);
                row[r.x0 as usize * 4..x1 as usize * 4].fill(0);
            }
        }
    }
}

pub fn premul8(r: u32, g: u32, b: u32, a: u32) -> [u8; 4] {
    let a16 = a * 0x101 * 0x101;
    [
        (((b * a16) / 0xFFFF) >> 8) as u8,
        (((g * a16) / 0xFFFF) >> 8) as u8,
        (((r * a16) / 0xFFFF) >> 8) as u8,
        a as u8,
    ]
}

pub fn premul16(r: u32, g: u32, b: u32, a: u32) -> [u8; 4] {
    [
        (((b * a) / 0xFFFF) >> 8) as u8,
        (((g * a) / 0xFFFF) >> 8) as u8,
        (((r * a) / 0xFFFF) >> 8) as u8,
        (a >> 8) as u8,
    ]
}

fn widen(dst: &[u8]) -> [u64; 4] {
    [
        0x101 * u64::from(dst[0]),
        0x101 * u64::from(dst[1]),
        0x101 * u64::from(dst[2]),
        0x101 * u64::from(dst[3]),
    ]
}

pub fn over_nonpremul16(dst: &mut [u8], r: u64, g: u64, b: u64, a: u64) {
    let [db, dg, dr, da] = widen(dst);
    let ia = 0xFFFF - a;
    let na = a + (da * ia) / 0xFFFF;
    let nr = (r * a + dr * ia) / 0xFFFF;
    let ng = (g * a + dg * ia) / 0xFFFF;
    let nb = (b * a + db * ia) / 0xFFFF;
    dst[0] = (nb >> 8) as u8;
    dst[1] = (ng >> 8) as u8;
    dst[2] = (nr >> 8) as u8;
    dst[3] = (na >> 8) as u8;
}

pub fn over_nonpremul8(dst: &mut [u8], r: u8, g: u8, b: u8, a: u8) {
    over_nonpremul16(
        dst,
        0x101 * u64::from(r),
        0x101 * u64::from(g),
        0x101 * u64::from(b),
        0x101 * u64::from(a),
    );
}

pub fn over_premul8(dst: &mut [u8], src: [u8; 4]) {
    let [db, dg, dr, da] = widen(dst);
    let [sb, sg, sr, sa] = widen(&src);
    let ia = 0xFFFF - sa;
    dst[0] = ((sb + (db * ia) / 0xFFFF) >> 8) as u8;
    dst[1] = ((sg + (dg * ia) / 0xFFFF) >> 8) as u8;
    dst[2] = ((sr + (dr * ia) / 0xFFFF) >> 8) as u8;
    dst[3] = ((sa + (da * ia) / 0xFFFF) >> 8) as u8;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    Src,
    Over,
}

pub enum Row<'a> {
    Indexed(&'a [u8], &'a [[u8; 4]; 256]),
    Rgba8(&'a [u8]),
    Rgba16(&'a [u16]),
}

pub fn draw_row(dst: &mut [u8], src: &Row<'_>, blend: Blend) {
    match (src, blend) {
        (Row::Indexed(idx, pal), Blend::Src) => {
            for (d, &i) in dst.as_chunks_mut::<4>().0.iter_mut().zip(idx.iter()) {
                d.copy_from_slice(&pal[usize::from(i)]);
            }
        }
        (Row::Indexed(idx, pal), Blend::Over) => {
            for (d, &i) in dst.as_chunks_mut::<4>().0.iter_mut().zip(idx.iter()) {
                over_premul8(d, pal[usize::from(i)]);
            }
        }
        (Row::Rgba8(px), Blend::Src) => {
            for (d, s) in dst
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(px.as_chunks::<4>().0.iter())
            {
                d.copy_from_slice(&premul8(
                    u32::from(s[0]),
                    u32::from(s[1]),
                    u32::from(s[2]),
                    u32::from(s[3]),
                ));
            }
        }
        (Row::Rgba8(px), Blend::Over) => {
            for (d, s) in dst
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(px.as_chunks::<4>().0.iter())
            {
                over_nonpremul8(d, s[0], s[1], s[2], s[3]);
            }
        }
        (Row::Rgba16(px), Blend::Src) => {
            for (d, s) in dst
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(px.as_chunks::<4>().0.iter())
            {
                d.copy_from_slice(&premul16(
                    u32::from(s[0]),
                    u32::from(s[1]),
                    u32::from(s[2]),
                    u32::from(s[3]),
                ));
            }
        }
        (Row::Rgba16(px), Blend::Over) => {
            for (d, s) in dst
                .as_chunks_mut::<4>()
                .0
                .iter_mut()
                .zip(px.as_chunks::<4>().0.iter())
            {
                over_nonpremul16(
                    d,
                    u64::from(s[0]),
                    u64::from(s[1]),
                    u64::from(s[2]),
                    u64::from(s[3]),
                );
            }
        }
    }
}

pub fn premul_palette(rgba: &[[u8; 4]; 256]) -> [[u8; 4]; 256] {
    let mut out = [[0u8; 4]; 256];
    for (o, c) in out.iter_mut().zip(rgba.iter()) {
        *o = premul8(
            u32::from(c[0]),
            u32::from(c[1]),
            u32::from(c[2]),
            u32::from(c[3]),
        );
    }
    out
}

pub fn premultiply_bgra(pixels: &mut [u8]) {
    for p in pixels.as_chunks_mut::<4>().0.iter_mut() {
        let out = premul8(
            u32::from(p[2]),
            u32::from(p[1]),
            u32::from(p[0]),
            u32::from(p[3]),
        );
        p.copy_from_slice(&out);
    }
}
