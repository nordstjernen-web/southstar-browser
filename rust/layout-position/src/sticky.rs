//! Southstar — sticky positioning: how far a position: sticky box moves to stay inside its scrollport, and the scroll-driven model the painters reuse.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::{self as glib, GBoolean};
use southstar_layout::BoxRef;
use southstar_style::{Kind, PropId, ValueRef};

use crate::ffi::{length_resolve, prop};
use crate::relative::length_is_auto;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct StickyY {
    has_top: GBoolean,
    has_bottom: GBoolean,
    top_start: f64,
    top_cap: f64,
    bottom_start: f64,
    bottom_cap: f64,
}

fn is_sticky(b: BoxRef<'_>) -> bool {
    prop(b, PropId::Position).is_some_and(|v| v.is_keyword(c"sticky"))
}

fn inset(v: Option<ValueRef<'_>>, basis: f64) -> Option<f64> {
    let v = v?;
    if length_is_auto(Some(v)) || !matches!(v.kind(), Kind::Length | Kind::Calc) {
        return None;
    }
    let out = length_resolve(Some(v), basis, 0.0);
    out.is_finite().then_some(out)
}

fn outer_height(b: BoxRef<'_>) -> f64 {
    let (m, p, e) = (b.margin(), b.padding(), b.border());
    m.top + e.top + p.top + b.content_height() + p.bottom + e.bottom + m.bottom
}

fn outer_width(b: BoxRef<'_>) -> f64 {
    let (m, p, e) = (b.margin(), b.padding(), b.border());
    m.left + e.left + p.left + b.content_width() + p.right + e.right + m.right
}

fn content_origin(p: BoxRef<'_>) -> (f64, f64) {
    let (m, pd, e) = (p.margin(), p.padding(), p.border());
    (
        p.x() + m.left + e.left + pd.left,
        p.y() + m.top + e.top + pd.top,
    )
}

fn pull_toward(want: f64, cap: f64) -> f64 {
    let cap = if cap < 0.0 { 0.0 } else { cap };
    if want < cap { want } else { cap }
}

fn push_back(want: f64, cap: f64) -> f64 {
    let cap = if cap > 0.0 { 0.0 } else { cap };
    if want > cap { want } else { cap }
}

pub fn offset_in(b: BoxRef<'_>, sp_x0: f64, sp_y0: f64, sp_x1: f64, sp_y1: f64) -> (f64, f64) {
    if !is_sticky(b) {
        return (0.0, 0.0);
    }
    let box_top = b.y();
    let box_h = outer_height(b);
    let box_left = b.x();
    let box_w = outer_width(b);
    let (cb_left, cb_top, cb_right, cb_bot) = match b.parent() {
        Some(p) => {
            let (l, t) = content_origin(p);
            (l, t, l + p.content_width(), t + p.content_height())
        }
        None => (sp_x0, 0.0, sp_x1, f64::MAX / 2.0),
    };
    let sp_w = sp_x1 - sp_x0;
    let sp_h = sp_y1 - sp_y0;
    let top = inset(prop(b, PropId::Top), sp_h);
    let bottom = inset(prop(b, PropId::Bottom), sp_h);
    let left = inset(prop(b, PropId::Left), sp_w);
    let right = inset(prop(b, PropId::Right), sp_w);
    let mut dx = 0.0;
    let mut dy = 0.0;
    if let Some(t) = top {
        let target = sp_y0 + t;
        if box_top < target {
            dy = pull_toward(target - box_top, cb_bot - (box_top + box_h));
        }
    }
    if let Some(bv) = bottom
        && dy == 0.0
    {
        let target = sp_y1 - bv;
        let box_bot = box_top + box_h;
        if box_bot > target {
            dy = push_back(target - box_bot, cb_top - box_top);
        }
    }
    if let Some(l) = left {
        let target = sp_x0 + l;
        if box_left < target {
            dx = pull_toward(target - box_left, cb_right - (box_left + box_w));
        }
    }
    if let Some(r) = right
        && dx == 0.0
    {
        let target = sp_x1 - r;
        let box_right = box_left + box_w;
        if box_right > target {
            dx = push_back(target - box_right, cb_left - box_left);
        }
    }
    (
        if dx.is_finite() { dx } else { 0.0 },
        if dy.is_finite() { dy } else { 0.0 },
    )
}

pub fn scrollport_for(b: BoxRef<'_>) -> Option<(f64, f64, f64, f64)> {
    let mut a = b.parent();
    while let Some(anc) = a {
        if anc.scrolls() {
            let (m, p, e) = (anc.margin(), anc.padding(), anc.border());
            let x0 = anc.x() + m.left + e.left + anc.scroll_x();
            let y0 = anc.y() + m.top + e.top + anc.scroll_y();
            let x1 = x0 + p.left + anc.content_width() + p.right;
            let y1 = y0 + p.top + anc.content_height() + p.bottom;
            return Some((x0, y0, x1, y1));
        }
        a = anc.parent();
    }
    None
}

pub fn offset(b: BoxRef<'_>, vp_x0: f64, vp_y0: f64, vp_x1: f64, vp_y1: f64) -> (f64, f64) {
    let (x0, y0, x1, y1) = scrollport_for(b).unwrap_or((vp_x0, vp_y0, vp_x1, vp_y1));
    offset_in(b, x0, y0, x1, y1)
}

fn c_max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn c_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

pub fn y_model(b: BoxRef<'_>, viewport_h: f64) -> (StickyY, bool) {
    let mut out = StickyY::default();
    let Some(p) = b.parent() else {
        return (out, false);
    };
    if !is_sticky(b) {
        return (out, false);
    }
    let box_top = b.y();
    let box_h = outer_height(b);
    let cb_top = content_origin(p).1;
    let cb_bot = cb_top + p.content_height();
    let top = inset(prop(b, PropId::Top), viewport_h);
    let bottom = inset(prop(b, PropId::Bottom), viewport_h);
    out.has_top = glib::boolean(top.is_some());
    out.has_bottom = glib::boolean(bottom.is_some());
    if let Some(t) = top {
        out.top_start = box_top - t;
        out.top_cap = c_max(cb_bot - (box_top + box_h), 0.0);
    }
    if let Some(bv) = bottom {
        out.bottom_start = box_top + box_h + bv - viewport_h;
        out.bottom_cap = c_min(cb_top - box_top, 0.0);
    }
    let ok = out.top_start.is_finite()
        && out.top_cap.is_finite()
        && out.bottom_start.is_finite()
        && out.bottom_cap.is_finite();
    (out, ok)
}

pub fn y_offset(m: &StickyY, scroll_y: f64) -> f64 {
    let mut dy = 0.0;
    if m.has_top != 0 && scroll_y > m.top_start {
        dy = c_min(scroll_y - m.top_start, m.top_cap);
    }
    if m.has_bottom != 0 && dy == 0.0 && scroll_y < m.bottom_start {
        dy = c_max(scroll_y - m.bottom_start, m.bottom_cap);
    }
    dy
}
