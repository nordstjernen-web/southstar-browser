//! Southstar — a block's used height from its content, height, aspect-ratio, min/max and stretch keywords, then align-content, centred control content, scroll extents and content-visibility.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxRef, children};
use southstar_layout_float::{kw_scrolls, overflow_axis_keyword};
use southstar_layout_geom::{
    aspect_ratio_number, containing_block_definite_height, height_keyword_stretches,
    resolve_used_height, style_is_absolute_or_fixed,
};
use southstar_style::{
    CALC_SIZE_ANY, CALC_SIZE_AUTO, CALC_SIZE_PERCENT, CALC_SIZE_STRETCH, Kind, PropId, ValueRef,
};

use crate::align::block_align_content_shift;
use crate::dispatch::shift_box_tree;
use crate::ffi::{dom_of, size_keyword_is_intrinsic, style_of};
use crate::fieldset::{c_max, c_min};

pub struct HeightInput {
    pub pcw: f64,
    pub inner_x: f64,
    pub inner_y: f64,
    pub cursor_y: f64,
    pub legend_extra: f64,
    pub border_box: bool,
    pub vert_extras: f64,
}

fn is_length_or_calc(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc))
}

fn keyword_is(v: Option<ValueRef<'_>>, kw: &core::ffi::CStr) -> bool {
    v.is_some_and(|v| v.is_keyword(kw))
}

fn shift_in_flow_children(b: BoxRef<'_>, dy: f64) {
    for c in children(b) {
        if !style_is_absolute_or_fixed(style_of(c)) {
            shift_box_tree(c, 0.0, dy);
        }
    }
}

fn element_named(b: BoxRef<'_>, name: &[u8]) -> bool {
    dom_of(b).is_some_and(|d| d.is_element() && d.name().is_some_and(|n| n.to_bytes() == name))
}

fn horizontal_scroll_extent(b: BoxRef<'_>, inner_x: f64) -> Option<f64> {
    let content_right = inner_x + b.content_width();
    let padding_right = content_right + b.padding().right;
    let mut max_right = padding_right;
    for c in children(b) {
        if style_is_absolute_or_fixed(style_of(c)) {
            continue;
        }
        let (m, p, bd) = (c.margin(), c.padding(), c.border());
        let right = c.x() + m.left + c.content_width() + p.left + p.right + bd.left + bd.right;
        let flow_right = c_min(right + m.right, c_max(right, content_right)) + b.padding().right;
        if right > max_right {
            max_right = right;
        }
        if flow_right > max_right {
            max_right = flow_right;
        }
    }
    (max_right > padding_right + 1.0).then_some(max_right - padding_right)
}

pub fn finish_height(b: BoxRef<'_>, input: &HeightInput) {
    let style = style_of(b);
    let pcw = input.pcw;
    let border_box = input.border_box;
    let vert_extras = input.vert_extras;
    let hv = style.and_then(|s| s.get(PropId::Height));
    let mxh = style.and_then(|s| s.get(PropId::MaxHeight));
    let mnh = style.and_then(|s| s.get(PropId::MinHeight));
    let overflow_scrolls =
        style.is_some() && kw_scrolls(overflow_axis_keyword(style_of(b), PropId::OverflowY));
    let overflow_scrolls_x =
        style.is_some() && kw_scrolls(overflow_axis_keyword(style_of(b), PropId::OverflowX));
    let measured = input.cursor_y - input.inner_y + input.legend_extra;
    b.set_measured_content_height(measured);
    let calc_size_h = |v: Option<ValueRef<'_>>| {
        let mut cs = v.and_then(|v| v.calc_size())?;
        let cb_h = containing_block_definite_height(b);
        if cs.basis == CALC_SIZE_PERCENT && cb_h < 0.0 {
            cs.basis = CALC_SIZE_AUTO;
            cs.pct = 0.0;
        }
        let keyword_basis = !matches!(cs.basis, CALC_SIZE_ANY | CALC_SIZE_PERCENT);
        let basis = match cs.basis {
            CALC_SIZE_ANY | CALC_SIZE_PERCENT => 0.0,
            CALC_SIZE_STRETCH if cb_h >= 0.0 => {
                let (m, p, bd) = (b.margin(), b.padding(), b.border());
                (cb_h - m.top - m.bottom - bd.top - bd.bottom - p.top - p.bottom).max(0.0)
            }
            _ => measured,
        };
        let mut h = cs.resolve(basis, cb_h.max(0.0)).max(0.0);
        if border_box && keyword_basis {
            h += vert_extras;
        }
        Some(h)
    };
    let mut explicit_h = -1.0;
    if let Some(h) = calc_size_h(hv) {
        explicit_h = h;
    } else if is_length_or_calc(hv) {
        explicit_h = resolve_used_height(b, hv, pcw, -1.0);
    }
    let mut stretch_h = -1.0;
    if height_keyword_stretches(hv)
        || height_keyword_stretches(mnh)
        || height_keyword_stretches(mxh)
    {
        let cb_h = containing_block_definite_height(b);
        if cb_h >= 0.0 {
            let (m, p, bd) = (b.margin(), b.padding(), b.border());
            stretch_h = cb_h - m.top - m.bottom - bd.top - bd.bottom - p.top - p.bottom;
            if stretch_h < 0.0 {
                stretch_h = 0.0;
            }
        }
    }
    if explicit_h < 0.0 && stretch_h >= 0.0 && height_keyword_stretches(hv) {
        explicit_h = stretch_h;
        if border_box {
            explicit_h += vert_extras;
        }
    }
    if explicit_h >= 0.0 {
        if border_box {
            explicit_h -= vert_extras;
            if explicit_h < 0.0 {
                explicit_h = 0.0;
            }
        }
        b.set_content_height(explicit_h);
    } else {
        let ratio = style.map_or(-1.0, |s| aspect_ratio_number(s.get(PropId::AspectRatio)).0);
        if ratio > 0.0 && b.content_width() > 0.0 {
            let aspect_h = b.content_width() / ratio;
            let content_floor = !is_length_or_calc(mnh)
                && !overflow_scrolls
                && !keyword_is(style.and_then(|s| s.get(PropId::OverflowY)), c"hidden");
            b.set_content_height(if content_floor && measured > aspect_h {
                measured
            } else {
                aspect_h
            });
        } else {
            b.set_content_height(measured);
        }
    }
    let mut max_h = calc_size_h(mxh).unwrap_or_else(|| resolve_used_height(b, mxh, pcw, -1.0));
    if border_box && max_h >= 0.0 {
        max_h -= vert_extras;
        if max_h < 0.0 {
            max_h = 0.0;
        }
    }
    if max_h < 0.0 && stretch_h >= 0.0 && height_keyword_stretches(mxh) {
        max_h = stretch_h;
    }
    if max_h < 0.0 && size_keyword_is_intrinsic(mxh) {
        max_h = measured;
    }
    if max_h >= 0.0 && b.content_height() > max_h {
        b.set_content_height(max_h);
    }
    let mut min_h = calc_size_h(mnh).unwrap_or_else(|| resolve_used_height(b, mnh, pcw, -1.0));
    if min_h < 0.0 && stretch_h >= 0.0 && height_keyword_stretches(mnh) {
        min_h = stretch_h + if border_box { vert_extras } else { 0.0 };
    }
    if border_box && min_h >= 0.0 {
        min_h -= vert_extras;
        if min_h < 0.0 {
            min_h = 0.0;
        }
    }
    if min_h < 0.0 && size_keyword_is_intrinsic(mnh) {
        min_h = measured;
    }
    if min_h >= 0.0 && b.content_height() < min_h {
        b.set_content_height(min_h);
    }
    let align_shift = block_align_content_shift(b, b.content_height() - measured);
    if align_shift != 0.0 {
        shift_in_flow_children(b, align_shift);
    }
    if align_shift == 0.0 && element_named(b, b"input") && b.content_height() > measured + 0.5 {
        let shift = (b.content_height() - measured) * 0.5;
        for c in children(b) {
            c.set_y(c.y() + shift);
        }
    }
    if align_shift == 0.0
        && element_named(b, b"button")
        && !keyword_is(style.and_then(|s| s.get(PropId::Appearance)), c"none")
        && b.content_height() > measured + 0.5
    {
        shift_in_flow_children(b, (b.content_height() - measured) * 0.5);
    }
    if overflow_scrolls {
        b.set_scrolls(true);
        if measured > b.content_height() {
            b.set_scroll_max(None, Some(measured - b.content_height()));
        }
    }
    if overflow_scrolls_x && let Some(extent) = horizontal_scroll_extent(b, input.inner_x) {
        b.set_scrolls(true);
        b.set_scroll_max(Some(extent), None);
    }
    let content_visibility_hidden = style
        .and_then(|s| s.get(PropId::ContentVisibility))
        .is_some_and(|v| v.is_keyword(c"hidden"));
    if content_visibility_hidden {
        let mut contained_height = if explicit_h >= 0.0 { explicit_h } else { 0.0 };
        if max_h >= 0.0 && contained_height > max_h {
            contained_height = max_h;
        }
        if min_h >= 0.0 && contained_height < min_h {
            contained_height = min_h;
        }
        b.set_content_height(contained_height);
        b.set_scrolls(false);
        b.set_scroll_max(Some(0.0), Some(0.0));
    }
}
