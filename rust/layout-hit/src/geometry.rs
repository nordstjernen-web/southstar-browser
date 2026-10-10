//! Southstar — moving a hit point into a box: the fixed and sticky offsets against the scrolled viewport, the inverse of the box's transform, scroll offsets and inline atomic placement.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::sync::atomic::{AtomicU64, Ordering};

use southstar_layout::{BoxRef, InlineAtomic};
use southstar_style::{PropId, StyleRef};

use crate::ffi;

static VIEWPORT_X: AtomicU64 = AtomicU64::new(0);
static VIEWPORT_Y: AtomicU64 = AtomicU64::new(0);

pub fn set_viewport(scroll_x: f64, scroll_y: f64) {
    let finite_or_zero = |v: f64| if v.is_finite() { v } else { 0.0 };
    VIEWPORT_X.store(finite_or_zero(scroll_x).to_bits(), Ordering::Relaxed);
    VIEWPORT_Y.store(finite_or_zero(scroll_y).to_bits(), Ordering::Relaxed);
}

fn viewport() -> (f64, f64) {
    (
        f64::from_bits(VIEWPORT_X.load(Ordering::Relaxed)),
        f64::from_bits(VIEWPORT_Y.load(Ordering::Relaxed)),
    )
}

pub fn position_keyword(s: Option<StyleRef<'_>>) -> Option<&core::ffi::CStr> {
    s?.get(PropId::Position)?.keyword_text()
}

pub fn is_fixed(b: BoxRef<'_>) -> bool {
    let Some(s) = ffi::style_of(b) else {
        return false;
    };
    if !s
        .get(PropId::Position)
        .is_some_and(|v| v.is_keyword(c"fixed"))
    {
        return false;
    }
    core::iter::successors(b.parent(), |p| p.parent())
        .all(|p| !ffi::style_creates_fixed_cb(p.style()))
}

pub fn hit_offset(b: BoxRef<'_>) -> (f64, f64) {
    let (vx, vy) = viewport();
    match position_keyword(ffi::style_of(b)).map(|k| k.to_bytes()) {
        Some(b"fixed") if is_fixed(b) => (vx, vy),
        Some(b"sticky") => {
            let (vw, vh) = ffi::viewport_size();
            ffi::sticky_offset(b, vx, vy, vx + vw, vy + vh)
        }
        _ => (0.0, 0.0),
    }
}

pub fn enter(b: BoxRef<'_>, x: f64, y: f64) -> (f64, f64) {
    let (dx, dy) = hit_offset(b);
    (x - dx, y - dy)
}

pub fn has_transform(s: StyleRef<'_>) -> bool {
    [
        PropId::Transform,
        PropId::Translate,
        PropId::Rotate,
        PropId::Scale,
    ]
    .into_iter()
    .any(|p| s.get(p).is_some())
}

fn border_box_size(b: BoxRef<'_>) -> (f64, f64) {
    let (p, br) = (b.padding(), b.border());
    (
        b.content_width() + p.left + p.right + br.left + br.right,
        b.content_height() + p.top + p.bottom + br.top + br.bottom,
    )
}

fn origin_offset(v: f64, is_percent: i32, basis: f64) -> f64 {
    if is_percent != 0 {
        v / 100.0 * basis
    } else {
        v
    }
}

pub fn untransform(b: BoxRef<'_>, x: f64, y: f64) -> Option<(f64, f64)> {
    let Some(s) = ffi::style_of(b).filter(|&s| has_transform(s)) else {
        return Some((x, y));
    };
    let eff = ffi::effective_transform(s);
    if eff.n_ops == 0 {
        return Some((x, y));
    }
    let bx = b.x() + b.margin().left;
    let by = b.y() + b.margin().top;
    let (bw, bh) = border_box_size(b);
    let (mut ox, mut oy) = (bx + bw / 2.0, by + bh / 2.0);
    if let Some(origin) = s
        .get(PropId::TransformOrigin)
        .and_then(|v| v.transform())
        .filter(|t| t.n_ops > 0)
    {
        let o = &origin.ops[0];
        ox = bx + origin_offset(o.a, o.a_is_percent, bw);
        oy = by + origin_offset(o.b, o.b_is_percent, bh);
    }
    let m = ffi::transform_to_mat4(&eff, bw, bh);
    if !m.is_affine2d() {
        return Some((x, y));
    }
    let m = &m.m;
    let [xx, yx, xy, yy, x0, y0] = ffi::invert_affine([m[0], m[4], m[1], m[5], m[3], m[7]])?;
    let (px, py) = (x - ox, y - oy);
    Some((xx * px + xy * py + x0 + ox, yx * px + yy * py + y0 + oy))
}

pub fn padding_contains(b: BoxRef<'_>, x: f64, y: f64) -> bool {
    let (m, br, p) = (b.margin(), b.border(), b.padding());
    let x0 = b.x() + m.left + br.left;
    let y0 = b.y() + m.top + br.top;
    let x1 = x0 + b.content_width() + p.left + p.right;
    let y1 = y0 + b.content_height() + p.top + p.bottom;
    x >= x0 && x <= x1 && y >= y0 && y <= y1
}

pub fn border_contains(b: BoxRef<'_>, x: f64, y: f64) -> bool {
    let x0 = b.x() + b.margin().left;
    let y0 = b.y() + b.margin().top;
    let (w, h) = border_box_size(b);
    x >= x0 && x <= x0 + w && y >= y0 && y <= y0 + h
}

pub fn outside_paint_span(b: BoxRef<'_>, y: f64) -> bool {
    b.paint_bottom() > b.paint_top() && (y < b.paint_top() - 1.0 || y > b.paint_bottom() + 1.0)
}

pub fn clips_out(b: BoxRef<'_>, x: f64, y: f64) -> bool {
    ffi::clips_children(b) && !padding_contains(b, x, y)
}

pub fn scrolled(b: BoxRef<'_>, x: f64, y: f64) -> (f64, f64) {
    (x + b.scroll_x(), y + b.scroll_y())
}

pub fn atomic_point<'a>(
    owner: BoxRef<'_>,
    atomic: &'a InlineAtomic,
    x: f64,
    y: f64,
) -> Option<(BoxRef<'a>, f64, f64)> {
    let ab = atomic.box_ref()?;
    let (ox, oy) = atomic.owner_offset();
    let dx = owner.x() + ox + ab.rel_dx() - ab.x();
    let dy = owner.y() + oy + ab.rel_dy() - ab.y();
    Some((ab, x - dx, y - dy))
}
