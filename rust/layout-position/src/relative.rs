//! Southstar — relative positioning: offsetting position: relative boxes, and inline-level atomic boxes on their lines, by their insets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::BoxRef;
use southstar_style::{Kind, PropId, StyleRef, UNIT_PERCENT, ValueRef};

use crate::ffi::{
    containing_block_definite_height, dom_of, height_keyword_stretches, prop, shift_box_tree,
    style_of, value_is_percent, viewport_h,
};

const UNIT_EM: u32 = 1;

pub fn length_is_auto(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| v.is_keyword(c"auto"))
}

fn style_is_relative(s: Option<StyleRef<'_>>) -> bool {
    s.and_then(|s| s.get(PropId::Position))
        .is_some_and(|v| v.is_keyword(c"relative"))
}

fn length_or_zero(v: Option<ValueRef<'_>>, basis: f64) -> f64 {
    match v.and_then(ValueRef::length) {
        Some((v, UNIT_PERCENT)) => v * basis / 100.0,
        Some((v, UNIT_EM)) => v * 16.0,
        Some((v, _)) => v,
        None => 0.0,
    }
}

pub fn translate_subtree(b: BoxRef<'_>, dx: f64, dy: f64) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    shift_box_tree(b, dx, dy);
}

fn relative_pct_cb_height(b: BoxRef<'_>) -> f64 {
    let mut p = b.parent();
    while let Some(pb) = p {
        if !pb.style().is_null() {
            break;
        }
        p = pb.parent();
    }
    let Some(p) = p else {
        return -1.0;
    };
    let h = prop(p, PropId::Height);
    if let Some(hv) = h
        && hv.kind() == Kind::Keyword
    {
        return if height_keyword_stretches(h) {
            containing_block_definite_height(b)
        } else {
            -1.0
        };
    }
    let Some(hv) = h else {
        return -1.0;
    };
    if value_is_percent(h) {
        let is_html = dom_of(p).and_then(|d| d.name()) == Some(c"html");
        let base = if is_html {
            viewport_h()
        } else {
            relative_pct_cb_height(p)
        };
        if base < 0.0 {
            return -1.0;
        }
        if let Some((pct, px)) = hv.calc() {
            return pct / 100.0 * base + px;
        }
        return hv.length().map_or(0.0, |(v, _)| v) * base / 100.0;
    }
    if p.content_height() > 0.0 {
        return p.content_height();
    }
    -1.0
}

fn relative_offset_y(b: BoxRef<'_>, parent_h: f64, pct_of: BoxRef<'_>) -> f64 {
    let tv = prop(b, PropId::Top);
    let bv = prop(b, PropId::Bottom);
    let from_top = tv.is_some() && !length_is_auto(tv);
    let v = if from_top { tv } else { bv };
    let sign = if from_top { 1.0 } else { -1.0 };
    if v.is_none() || length_is_auto(v) {
        return 0.0;
    }
    if !value_is_percent(v) {
        return sign * length_or_zero(v, parent_h);
    }
    let cb_h = relative_pct_cb_height(pct_of);
    if cb_h < 0.0 {
        0.0
    } else {
        sign * length_or_zero(v, cb_h)
    }
}

fn relative_offset_x(b: BoxRef<'_>, parent_w: f64) -> f64 {
    let lv = prop(b, PropId::Left);
    let rv = prop(b, PropId::Right);
    if lv.is_some() && !length_is_auto(lv) {
        return length_or_zero(lv, parent_w);
    }
    if rv.is_some() && !length_is_auto(rv) {
        return -length_or_zero(rv, parent_w);
    }
    0.0
}

fn apply_relative_offset(b: BoxRef<'_>, parent_w: f64, parent_h: f64) {
    translate_subtree(
        b,
        relative_offset_x(b, parent_w),
        relative_offset_y(b, parent_h, b),
    );
}

fn apply_atomic_position_offsets(b: BoxRef<'_>, cb_w: f64, cb_h: f64) {
    for ab in b.inline_atomic_boxes() {
        if style_is_relative(style_of(ab)) {
            let dx = relative_offset_x(ab, cb_w);
            let dy = relative_offset_y(ab, cb_h, b);
            ab.set_rel_offset(dx, dy);
            translate_subtree(ab, dx, dy);
        }
        let mut c = ab.first_child();
        while let Some(child) = c {
            apply_position_offsets(child, ab.content_width(), ab.content_height());
            c = child.next_sibling();
        }
        if ab.inline_atomics().is_some() {
            apply_atomic_position_offsets(ab, ab.content_width(), ab.content_height());
        }
    }
}

pub fn apply_position_offsets(b: BoxRef<'_>, parent_w: f64, parent_h: f64) {
    let child_w = b.content_width();
    let child_h = b.content_height();
    if style_is_relative(style_of(b)) {
        apply_relative_offset(b, parent_w, parent_h);
    }
    if b.inline_atomics().is_some() {
        apply_atomic_position_offsets(b, parent_w, parent_h);
    }
    let mut c = b.first_child();
    while let Some(child) = c {
        apply_position_offsets(child, child_w, child_h);
        c = child.next_sibling();
    }
}
