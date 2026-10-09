//! Southstar — canvas raster helpers: the shadow box blur, compositing and fill-rule keywords, and conic gradient colours.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::f64::consts::PI;

const MAX_BLUR_RADIUS: i32 = 64;

pub(crate) const CONIC_SECTORS: i32 = 256;

pub(crate) const CONIC_RADIUS: f64 = 1e5;

const COMPOSITE_OPERATORS: [(&str, i32); 27] = [
    ("source-over", 2),
    ("source-in", 3),
    ("source-out", 4),
    ("source-atop", 5),
    ("destination-over", 7),
    ("destination-in", 8),
    ("destination-out", 9),
    ("destination-atop", 10),
    ("lighter", 12),
    ("copy", 1),
    ("xor", 11),
    ("clear", 0),
    ("multiply", 14),
    ("screen", 15),
    ("overlay", 16),
    ("darken", 17),
    ("lighten", 18),
    ("color-dodge", 19),
    ("color-burn", 20),
    ("hard-light", 21),
    ("soft-light", 22),
    ("difference", 23),
    ("exclusion", 24),
    ("hue", 25),
    ("saturation", 26),
    ("color", 27),
    ("luminosity", 28),
];

const OPERATOR_OVER: i32 = 2;

pub(crate) fn composite_operator(name: Option<&[u8]>) -> i32 {
    name.and_then(|name| {
        COMPOSITE_OPERATORS
            .iter()
            .find(|(known, _)| known.as_bytes() == name)
            .map(|&(_, op)| op)
    })
    .unwrap_or(OPERATOR_OVER)
}

pub(crate) fn fill_rule(name: Option<&[u8]>) -> i32 {
    i32::from(name == Some(b"evenodd"))
}

fn blur_pass(
    dst: &mut [u8],
    dst_stride: usize,
    src: &[u8],
    src_stride: usize,
    (w, h): (usize, usize),
    radius: i32,
    vertical: bool,
) {
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0i32; 4];
            let mut n = 0;
            for k in -radius..=radius {
                let (sx, sy) = if vertical {
                    (x as i64, y as i64 + i64::from(k))
                } else {
                    (x as i64 + i64::from(k), y as i64)
                };
                if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                    continue;
                }
                let p = sy as usize * src_stride + sx as usize * 4;
                for (c, s) in sum.iter_mut().enumerate() {
                    *s += i32::from(src[p + c]);
                }
                n += 1;
            }
            let n = if n == 0 { 1 } else { n };
            let q = y * dst_stride + x * 4;
            for (c, s) in sum.iter().enumerate() {
                dst[q + c] = (s / n) as u8;
            }
        }
    }
}

pub(crate) fn box_blur(data: &mut [u8], w: i32, h: i32, stride: i32, radius: i32) {
    if radius <= 0 || w <= 0 || h <= 0 || stride <= 0 {
        return;
    }
    let radius = radius.min(MAX_BLUR_RADIUS);
    let (w, h, stride) = (w as usize, h as usize, stride as usize);
    let mut tmp = vec![0u8; w * h * 4];
    blur_pass(&mut tmp, w * 4, data, stride, (w, h), radius, false);
    blur_pass(data, stride, &tmp, w * 4, (w, h), radius, true);
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ConicStop {
    pub pos: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

pub(crate) fn sort_stops(stops: &mut [ConicStop]) {
    stops.sort_by(|x, y| {
        x.pos
            .partial_cmp(&y.pos)
            .unwrap_or(core::cmp::Ordering::Equal)
    });
}

pub(crate) fn conic_color_at(stops: &[ConicStop], t: f64) -> [f64; 4] {
    let rgba = |s: &ConicStop| [s.r, s.g, s.b, s.a];
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else {
        return [0.0; 4];
    };
    if t <= first.pos {
        return rgba(first);
    }
    if t >= last.pos {
        return rgba(last);
    }
    for pair in stops.windows(2) {
        let (lo, hi) = (&pair[0], &pair[1]);
        if t <= hi.pos {
            let span = hi.pos - lo.pos;
            let f = if span > 0.0 { (t - lo.pos) / span } else { 0.0 };
            return [
                lo.r + (hi.r - lo.r) * f,
                lo.g + (hi.g - lo.g) * f,
                lo.b + (hi.b - lo.b) * f,
                lo.a + (hi.a - lo.a) * f,
            ];
        }
    }
    [0.0; 4]
}

pub(crate) struct Sector {
    pub edges: [(f64, f64); 2],
    pub colors: [[f64; 4]; 2],
}

pub(crate) fn conic_sectors(center: (f64, f64), angle: f64, stops: &[ConicStop]) -> Vec<Sector> {
    (0..CONIC_SECTORS)
        .map(|i| {
            let t0 = f64::from(i) / f64::from(CONIC_SECTORS);
            let t1 = f64::from(i + 1) / f64::from(CONIC_SECTORS);
            let a0 = angle + t0 * 2.0 * PI;
            let a1 = angle + t1 * 2.0 * PI;
            Sector {
                edges: [
                    (
                        center.0 + CONIC_RADIUS * a0.cos(),
                        center.1 + CONIC_RADIUS * a0.sin(),
                    ),
                    (
                        center.0 + CONIC_RADIUS * a1.cos(),
                        center.1 + CONIC_RADIUS * a1.sin(),
                    ),
                ],
                colors: [conic_color_at(stops, t0), conic_color_at(stops, t1)],
            }
        })
        .collect()
}
