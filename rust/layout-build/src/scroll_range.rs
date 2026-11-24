//! Southstar — extends the scroll range of scrolling boxes to cover the absolutely positioned boxes inside them.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxKind, BoxRef, children};
use southstar_layout_float::{
    box_clips_children, kw_clips, kw_scroll_container, kw_scrolls, overflow_axis_keyword,
};
use southstar_layout_geom::{
    box_is_fixed, flex_direction_of, keyword_or, self_start_is_far_side,
    style_is_absolute_or_fixed, style_is_flex_container,
};
use southstar_style::{PropId, StyleRef, box_style};

const AXIS_X: u8 = 1;
const AXIS_Y: u8 = 2;
const MAX_DEPTH: u32 = 512;

#[derive(Default)]
struct Reach {
    right: f64,
    bottom: f64,
    pad_right: f64,
    pad_bottom: f64,
    flip_x: bool,
    flip_y: bool,
}

fn hides_overflow_contribution(s: Option<StyleRef<'_>>) -> bool {
    s.and_then(|s| s.get(PropId::ContentVisibility))
        .is_some_and(|v| v.is_keyword(c"hidden") || v.is_keyword(c"auto"))
}

fn has_transform_style(s: Option<StyleRef<'_>>) -> bool {
    let Some(s) = s else { return false };
    s.get(PropId::Transform)
        .and_then(|v| v.transform())
        .is_some_and(|t| t.n_ops > 0)
        || s.get(PropId::Translate).is_some()
        || s.get(PropId::Rotate).is_some()
        || s.get(PropId::Scale).is_some()
        || s.get(PropId::Perspective).is_some()
}

fn overflow_clip_axes(c: BoxRef<'_>) -> u8 {
    let s = box_style(c);
    if s.is_none() || matches!(c.kind(), BoxKind::Inline | BoxKind::Text) {
        return 0;
    }
    let ox = overflow_axis_keyword(s, PropId::OverflowX);
    let oy = overflow_axis_keyword(s, PropId::OverflowY);
    if kw_scroll_container(ox) || kw_scroll_container(oy) {
        return AXIS_X | AXIS_Y;
    }
    (if kw_clips(ox) { AXIS_X } else { 0 }) | (if kw_clips(oy) { AXIS_Y } else { 0 })
}

fn unreachable_from_start(acc: &Reach, left: f64, top: f64) -> bool {
    (acc.flip_x && left >= acc.pad_right) || (acc.flip_y && top >= acc.pad_bottom)
}

fn add_reach(c: BoxRef<'_>, acc: &mut Reach, axes: u8) {
    let (p, bd, m) = (c.padding(), c.border(), c.margin());
    let w = c.content_width() + p.left + p.right + bd.left + bd.right;
    let h = c.content_height() + p.top + p.bottom + bd.top + bd.bottom;
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let left = c.x() + m.left;
    let top = c.y() + m.top;
    if unreachable_from_start(acc, left, top) {
        return;
    }
    if axes & AXIS_X != 0 && left + w > acc.right {
        acc.right = left + w;
    }
    if axes & AXIS_Y != 0 && top + h > acc.bottom {
        acc.bottom = top + h;
    }
}

fn visit(c: BoxRef<'_>, in_abs: bool, axes: u8, acc: &mut Reach, depth: u32) {
    if box_is_fixed(c) || has_transform_style(box_style(c)) {
        return;
    }
    let c_abs = in_abs || style_is_absolute_or_fixed(box_style(c));
    if c_abs {
        add_reach(c, acc, axes);
    }
    if hides_overflow_contribution(box_style(c)) {
        return;
    }
    let inner = axes & !overflow_clip_axes(c);
    if inner != 0 {
        walk(c, c_abs, inner, acc, depth + 1);
    }
}

fn walk(b: BoxRef<'_>, in_abs: bool, axes: u8, acc: &mut Reach, depth: u32) {
    if depth > MAX_DEPTH {
        return;
    }
    for c in children(b) {
        visit(c, in_abs, axes, acc, depth);
    }
    for ab in b.inline_atomic_boxes() {
        visit(ab, in_abs, axes, acc, depth);
    }
}

fn scroll_axis_flips(b: BoxRef<'_>) -> (bool, bool) {
    let s = box_style(b);
    let mut fx = self_start_is_far_side(s, true);
    let mut fy = self_start_is_far_side(s, false);
    if !style_is_flex_container(s) {
        return (fx, fy);
    }
    let dir = flex_direction_of(s).to_bytes();
    let column = dir.starts_with(b"column");
    let wrap_reverse = keyword_or(s, PropId::FlexWrap, c"nowrap").to_bytes() == b"wrap-reverse";
    if dir.windows(8).any(|w| w == b"-reverse") {
        if column {
            fy = !fy;
        } else {
            fx = !fx;
        }
    }
    if wrap_reverse {
        if column {
            fx = !fx;
        } else {
            fy = !fy;
        }
    }
    (fx, fy)
}

fn owned_by_viewport(b: BoxRef<'_>) -> bool {
    let Some(name) = b.dom().and_then(|d| d.name()) else {
        return false;
    };
    match name.to_bytes() {
        b"html" => true,
        b"body" => b.parent().and_then(box_style).is_none_or(|ps| {
            let ps = Some(ps);
            !kw_clips(overflow_axis_keyword(ps, PropId::OverflowX))
                && !kw_clips(overflow_axis_keyword(ps, PropId::OverflowY))
        }),
        _ => false,
    }
}

fn grow(max: f64, extent: f64) -> Option<f64> {
    let whole = (extent + 0.5).floor();
    (whole >= 1.0 && whole > max.max(0.0)).then_some(whole)
}

fn candidate_axes(b: BoxRef<'_>) -> Option<(bool, bool)> {
    let s = box_style(b);
    if b.parent().is_none() || s.is_none() {
        return None;
    }
    let sx = kw_scrolls(overflow_axis_keyword(s, PropId::OverflowX));
    let sy = kw_scrolls(overflow_axis_keyword(s, PropId::OverflowY));
    ((sx || sy)
        && box_clips_children(b)
        && !owned_by_viewport(b)
        && !hides_overflow_contribution(s))
    .then_some((sx, sy))
}

fn extend_from_abs(b: BoxRef<'_>) {
    let Some((sx, sy)) = candidate_axes(b) else {
        return;
    };
    let (flip_x, flip_y) = scroll_axis_flips(b);
    let (m, bd, p) = (b.margin(), b.border(), b.padding());
    let pad_right = b.x() + m.left + bd.left + p.left + b.content_width() + p.right;
    let pad_bottom = b.y() + m.top + bd.top + p.top + b.content_height() + p.bottom;
    let mut acc = Reach {
        right: pad_right,
        bottom: pad_bottom,
        pad_right,
        pad_bottom,
        flip_x,
        flip_y,
    };
    walk(b, false, AXIS_X | AXIS_Y, &mut acc, 0);
    let x = (sx && !flip_x)
        .then(|| grow(b.scroll_max_x(), acc.right - pad_right))
        .flatten();
    let y = (sy && !flip_y)
        .then(|| grow(b.scroll_max_y(), acc.bottom - pad_bottom))
        .flatten();
    if x.is_some() || y.is_some() {
        b.set_scrolls(true);
        b.set_scroll_max(x, y);
    }
}

pub fn extend_scroll_ranges_from_abs(b: BoxRef<'_>) {
    for c in children(b) {
        extend_scroll_ranges_from_abs(c);
    }
    for ab in b.inline_atomic_boxes() {
        extend_scroll_ranges_from_abs(ab);
    }
    extend_from_abs(b);
}
