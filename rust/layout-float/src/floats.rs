//! Southstar — the float bands a block lays out against: escaping floats, offsets at and over a span, clearance, readable-width advancing, inherited bands and shifting placed floats.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::BoxRef;

use crate::ffi::{FloatRef, shift_box_tree, style_of};
use crate::overflow::{box_establishes_bfc, float_side_of};

const MAX_DEPTH: i32 = 512;

pub fn collect_escaping(b: BoxRef<'_>, out: &mut Vec<FloatRef>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    let side = float_side_of(style_of(b));
    if side >= 0 {
        let (m, p, bd) = (b.margin(), b.padding(), b.border());
        out.push(FloatRef::placed(
            b,
            side,
            b.y(),
            b.y() + m.top + b.content_height() + p.top + p.bottom + bd.top + bd.bottom + m.bottom,
            b.content_width() + p.left + p.right + bd.left + bd.right + m.left + m.right,
        ));
        return;
    }
    if box_establishes_bfc(b) {
        return;
    }
    let mut c = b.first_child();
    while let Some(child) = c {
        collect_escaping(child, out, depth + 1);
        c = child.next_sibling();
    }
}

pub fn offsets_at(floats: &[FloatRef], y: f64) -> (f64, f64) {
    let (mut l, mut r) = (0.0, 0.0);
    for f in floats {
        if y < f.top || y >= f.bottom {
            continue;
        }
        if f.side == 0 {
            l += f.outer_w;
        } else {
            r += f.outer_w;
        }
    }
    (l, r)
}

pub fn offsets_over(floats: &[FloatRef], top: f64, bottom: f64) -> (f64, f64) {
    let (mut l, mut r) = offsets_at(floats, top);
    if bottom > top {
        for f in floats {
            if f.top <= top || f.top >= bottom {
                continue;
            }
            let (bl, br) = offsets_at(floats, f.top);
            if bl > l {
                l = bl;
            }
            if br > r {
                r = br;
            }
        }
    }
    (l, r)
}

pub fn clear_y(floats: &[FloatRef], y: f64, clear: i32) -> f64 {
    if clear == 0 {
        return y;
    }
    let mut out = y;
    for f in floats {
        if clear == 1 && f.side != 0 {
            continue;
        }
        if clear == 2 && f.side != 1 {
            continue;
        }
        if f.bottom > out {
            out = f.bottom;
        }
    }
    out
}

pub fn max_bottom(floats: &[FloatRef]) -> f64 {
    let mut y = 0.0;
    for f in floats {
        if f.bottom > y {
            y = f.bottom;
        }
    }
    y
}

pub fn advance_to_readable_width(floats: &[FloatRef], cw: f64, y: &mut f64) -> (bool, f64, f64) {
    let min_w = (cw * 0.40).clamp(120.0, 260.0);
    let mut moved = false;
    loop {
        let (left, right) = offsets_at(floats, *y);
        let avail = cw - left - right;
        if avail >= min_w || cw <= min_w {
            return (moved, left, right);
        }
        let mut next_y = *y;
        let mut advanced = false;
        for f in floats {
            if f.bottom > *y && (!advanced || f.bottom < next_y) {
                next_y = f.bottom;
                advanced = true;
            }
        }
        if !advanced || next_y <= *y {
            return (moved, left, right);
        }
        *y = next_y;
        moved = true;
    }
}

fn sort_edges(edges: &mut [f64]) {
    for i in 1..edges.len() {
        let mut j = i;
        while j > 0 && edges[j] < edges[j - 1] {
            edges.swap(j, j - 1);
            j -= 1;
        }
    }
}

pub struct Outer {
    pub inner_x: f64,
    pub cw: f64,
}

pub fn inherit(
    src: &[FloatRef],
    from: &Outer,
    inner_x: f64,
    cw: f64,
    from_y: f64,
) -> Vec<FloatRef> {
    let mut bands = Vec::new();
    if src.is_empty() {
        return bands;
    }
    let mut edges = Vec::with_capacity(src.len() * 2);
    for f in src {
        if f.bottom <= from_y {
            continue;
        }
        edges.push(f.top);
        edges.push(f.bottom);
    }
    sort_edges(&mut edges);
    for pair in edges.windows(2) {
        let (top, bottom) = (pair[0], pair[1]);
        if bottom <= top || bottom <= from_y {
            continue;
        }
        let (l, r) = offsets_at(src, top);
        let left_in = from.inner_x + l - inner_x;
        let right_in = inner_x + cw - (from.inner_x + from.cw - r);
        if l > 0.0 && left_in > 0.0 {
            bands.push(FloatRef::band(0, top, bottom, left_in));
        }
        if r > 0.0 && right_in > 0.0 {
            bands.push(FloatRef::band(1, top, bottom, right_in));
        }
    }
    bands
}

pub fn shift_placed(floats: &mut [FloatRef], dy: f64) {
    if dy == 0.0 {
        return;
    }
    for f in floats {
        let Some(b) = f.placed_box() else {
            continue;
        };
        shift_box_tree(b, 0.0, dy);
        f.top += dy;
        f.bottom += dy;
    }
}

pub fn band_bottom(floats: &[FloatRef], y: f64) -> f64 {
    let mut bottom = -1.0;
    for f in floats {
        if y < f.top || y >= f.bottom {
            continue;
        }
        if bottom < 0.0 || f.bottom < bottom {
            bottom = f.bottom;
        }
    }
    bottom
}
