//! Southstar — flex item sizing shared by row and column containers: base sizes, min and max main sizes, resolving flexible lengths, justify-content, align-content and stretching.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_layout::{BoxKind, BoxRef, Style};
use southstar_style::{Kind, PropId, ValueRef};

use crate::ffi::{
    box_is_scroll_container, containing_block_definite_height, dom_of, flex_box_is_border_box,
    flex_grow_of, flex_shrink_of, height_keyword_stretches, intrinsic_keyword_width, layout_box,
    length_resolve, measure_natural_width, min_content_width_of, overflow_scrolls,
    resolve_height_with_basis, resolve_used_height, shift_box_tree, size_keyword_is_intrinsic,
    style_is_absolute_or_fixed, style_of, value_is_percent,
};

#[derive(Clone, Copy, Default)]
pub struct FlexLen {
    pub basis: f64,
    pub min: f64,
    pub max: f64,
    pub grow: f64,
    pub shrink: f64,
    pub target: f64,
    pub violation: f64,
    pub frozen: bool,
}

impl FlexLen {
    pub fn hypothetical(&self) -> f64 {
        clamp_main(self.basis, self.min, self.max)
    }
}

#[derive(Clone, Copy)]
pub struct Item<'a> {
    pub b: BoxRef<'a>,
    pub len: FlexLen,
    pub extra: f64,
    pub main: f64,
}

pub fn collect<'a>(container: BoxRef<'a>, out: &mut Vec<Item<'a>>) {
    out.extend(
        southstar_layout::children(container)
            .filter(|c| !style_is_absolute_or_fixed(c.style()))
            .map(|b| Item {
                b,
                len: FlexLen::default(),
                extra: 0.0,
                main: 0.0,
            }),
    );
}

pub fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub fn value(b: BoxRef<'_>, prop: PropId) -> Option<ValueRef<'_>> {
    style_of(b).and_then(|s| s.get(prop))
}

pub fn is_length(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc))
}

pub fn is_keyword(v: Option<ValueRef<'_>>, kw: &CStr) -> bool {
    v.is_some_and(|v| v.is_keyword(kw))
}

pub fn has_auto(b: BoxRef<'_>, prop: PropId) -> bool {
    is_keyword(value(b, prop), c"auto")
}

pub fn outer_main_extras(c: BoxRef<'_>) -> f64 {
    let (m, p, b) = (c.margin(), c.padding(), c.border());
    m.left + m.right + p.left + p.right + b.left + b.right
}

pub fn vertical_extras(c: BoxRef<'_>) -> f64 {
    let (p, b) = (c.padding(), c.border());
    p.top + p.bottom + b.top + b.bottom
}

pub fn outer_height(c: BoxRef<'_>) -> f64 {
    let (m, p, b) = (c.margin(), c.padding(), c.border());
    c.content_height() + p.top + p.bottom + b.top + b.bottom + m.top + m.bottom
}

pub fn outer_width(c: BoxRef<'_>) -> f64 {
    let (m, p, b) = (c.margin(), c.padding(), c.border());
    c.content_width() + p.left + p.right + b.left + b.right + m.left + m.right
}

pub fn or_style(inherited: *const Style, c: BoxRef<'_>) -> *const Style {
    if inherited.is_null() {
        c.style()
    } else {
        inherited
    }
}

fn main_axis_extras(c: BoxRef<'_>) -> f64 {
    let (p, b) = (c.padding(), c.border());
    p.left + p.right + b.left + b.right
}

fn border_box_to_content(c: BoxRef<'_>, mut v: f64) -> f64 {
    if flex_box_is_border_box(c) {
        v -= main_axis_extras(c);
        if v < 0.0 {
            v = 0.0;
        }
    }
    v
}

fn keyword_width(c: BoxRef<'_>, v: Option<ValueRef<'_>>, cw: f64, inherited: *const Style) -> f64 {
    let Some(kw) = v.and_then(ValueRef::keyword_text) else {
        return -1.0;
    };
    let (m, p, b) = (c.margin(), c.padding(), c.border());
    let inner = cw - m.left - m.right - p.left - p.right - b.left - b.right;
    intrinsic_keyword_width(c, kw, or_style(inherited, c), inner)
}

fn main_basis_explicit(c: BoxRef<'_>, cw: f64, inherited: *const Style) -> Option<f64> {
    let s = style_of(c)?;
    let basis = s.get(PropId::FlexBasis);
    if is_length(basis) {
        return Some(border_box_to_content(c, length_resolve(basis, cw, 0.0)));
    }
    let keyword_basis = keyword_width(c, basis, cw, inherited);
    if keyword_basis >= 0.0 {
        return Some(keyword_basis);
    }
    let w = s.get(PropId::Width);
    if is_length(w) {
        return Some(border_box_to_content(c, length_resolve(w, cw, 0.0)));
    }
    let keyword_w = keyword_width(c, w, cw, inherited);
    (keyword_w >= 0.0).then_some(keyword_w)
}

fn content_basis_from_natural(b: BoxRef<'_>, inherited: *const Style) -> f64 {
    let w = measure_natural_width(b, or_style(inherited, b));
    if w > 0.0 { w } else { 0.0 }
}

fn max_main(c: BoxRef<'_>, cw: f64, inherited: *const Style) -> f64 {
    let mxw = value(c, PropId::MaxWidth);
    if mxw.is_some_and(|v| v.kind() == Kind::Keyword) {
        return keyword_width(c, mxw, cw, inherited);
    }
    if !is_length(mxw) {
        return -1.0;
    }
    let mx = length_resolve(mxw, cw, -1.0);
    if mx < 0.0 {
        -1.0
    } else {
        border_box_to_content(c, mx)
    }
}

pub fn is_replaced_like(c: BoxRef<'_>) -> bool {
    if matches!(c.kind(), BoxKind::Image | BoxKind::Video | BoxKind::Svg) {
        return true;
    }
    let Some(name) = dom_of(c).and_then(|n| n.element_name().map(|name| (n, name))) else {
        return false;
    };
    let (n, name) = name;
    if name == b"input" {
        return n.attr(c"type").is_none_or(|t| {
            let t = t.to_bytes();
            !t.eq_ignore_ascii_case(b"button")
                && !t.eq_ignore_ascii_case(b"submit")
                && !t.eq_ignore_ascii_case(b"reset")
        });
    }
    matches!(name, b"select" | b"textarea" | b"meter" | b"progress")
}

fn min_main(c: BoxRef<'_>, cw: f64, inherited: *const Style) -> f64 {
    let mnw = value(c, PropId::MinWidth);
    if is_length(mnw) {
        let mn = border_box_to_content(c, length_resolve(mnw, cw, -1.0));
        return if mn > 0.0 { mn } else { 0.0 };
    }
    if mnw.is_some_and(|v| v.kind() == Kind::Keyword) && !is_keyword(mnw, c"auto") {
        let mn = keyword_width(c, mnw, cw, inherited);
        return if mn > 0.0 { mn } else { 0.0 };
    }
    if box_is_scroll_container(c) {
        return 0.0;
    }
    let mut mn = min_content_width_of(c, or_style(inherited, c));
    if mn < 0.0 {
        mn = 0.0;
    }
    let wv = value(c, PropId::Width);
    let specified = if is_length(wv) {
        if value_is_percent(wv) && is_replaced_like(c) {
            border_box_to_content(c, length_resolve(wv, 0.0, -1.0))
        } else {
            border_box_to_content(c, length_resolve(wv, cw, -1.0))
        }
    } else {
        keyword_width(c, wv, cw, inherited)
    };
    if specified >= 0.0 && specified < mn {
        mn = specified;
    }
    let mx = max_main(c, cw, inherited);
    if mx >= 0.0 && mn > mx {
        mn = mx;
    }
    mn
}

pub fn row_len(c: BoxRef<'_>, cw: f64, inherited: *const Style) -> FlexLen {
    let basis = main_basis_explicit(c, cw, inherited)
        .unwrap_or_else(|| content_basis_from_natural(c, inherited));
    FlexLen {
        basis,
        min: min_main(c, cw, inherited),
        max: max_main(c, cw, inherited),
        grow: flex_grow_of(c),
        shrink: flex_shrink_of(c),
        ..FlexLen::default()
    }
}

pub fn clamp_main(mut v: f64, mn: f64, mx: f64) -> f64 {
    if mx >= 0.0 && v > mx {
        v = mx;
    }
    if v < mn {
        v = mn;
    }
    if v < 0.0 { 0.0 } else { v }
}

pub fn resolve_lengths(items: &mut [Item<'_>], available: f64) {
    let n = items.len();
    let mut sum_hyp = 0.0;
    for it in items.iter() {
        sum_hyp += it.len.hypothetical();
    }
    let growing = sum_hyp < available;
    let mut initial_free = available;
    for it in items.iter_mut() {
        let l = &mut it.len;
        let hyp = l.hypothetical();
        let factor = if growing { l.grow } else { l.shrink };
        l.frozen = factor <= 0.0 || (growing && l.basis > hyp) || (!growing && l.basis < hyp);
        l.target = hyp;
        initial_free -= if l.frozen { hyp } else { l.basis };
    }
    for _ in 0..=n {
        let mut sum_factor = 0.0;
        let mut sum_scaled = 0.0;
        let mut remaining = available;
        let mut any = false;
        for it in items.iter() {
            let l = &it.len;
            if l.frozen {
                remaining -= l.target;
                continue;
            }
            any = true;
            remaining -= l.basis;
            sum_factor += if growing { l.grow } else { l.shrink };
            sum_scaled += l.shrink * l.basis;
        }
        if !any {
            break;
        }
        if sum_factor < 1.0 {
            let product = initial_free * sum_factor;
            if product.abs() < remaining.abs() {
                remaining = product;
            }
        }
        let mut total_violation = 0.0;
        for it in items.iter_mut() {
            let l = &mut it.len;
            if l.frozen {
                continue;
            }
            let mut t = l.basis;
            if growing && remaining > 0.0 && sum_factor > 0.0 {
                t += remaining * (l.grow / sum_factor);
            } else if !growing && remaining < 0.0 && sum_scaled > 0.0 {
                t -= remaining.abs() * (l.shrink * l.basis / sum_scaled);
            }
            let clamped = clamp_main(t, l.min, l.max);
            l.violation = clamped - t;
            l.target = clamped;
            total_violation += clamped - t;
        }
        for it in items.iter_mut() {
            let l = &mut it.len;
            if l.frozen {
                continue;
            }
            let freeze = if total_violation > 0.0001 {
                l.violation > 0.0
            } else if total_violation < -0.0001 {
                l.violation < 0.0
            } else {
                true
            };
            if freeze {
                l.frozen = true;
            }
        }
    }
}

pub fn container_scrolls(b: BoxRef<'_>) -> bool {
    let s = b.style();
    if s.is_null() {
        return false;
    }
    overflow_scrolls(s, PropId::OverflowX) || overflow_scrolls(s, PropId::OverflowY)
}

pub fn justify_offsets(
    b: BoxRef<'_>,
    justify: &CStr,
    free_main: f64,
    count: usize,
    reverse: bool,
) -> (f64, f64) {
    if count == 0 {
        return (0.0, 0.0);
    }
    let justify = justify.to_bytes();
    if free_main < 0.0 {
        if container_scrolls(b) {
            return (if reverse { free_main } else { 0.0 }, 0.0);
        }
        if matches!(
            justify,
            b"space-between" | b"space-around" | b"space-evenly"
        ) {
            return (0.0, 0.0);
        }
    }
    let n = count as f64;
    match justify {
        b"flex-end" | b"end" | b"right" => (free_main, 0.0),
        b"center" => (free_main / 2.0, 0.0),
        b"space-between" => (
            0.0,
            if count > 1 {
                free_main / (n - 1.0)
            } else {
                0.0
            },
        ),
        b"space-around" => {
            let between = free_main / n;
            (between / 2.0, between)
        }
        b"space-evenly" => {
            let between = free_main / (n + 1.0);
            (between, between)
        }
        _ => (0.0, 0.0),
    }
}

pub struct ContentOffsets {
    pub lead: f64,
    pub between: f64,
    pub per_line: f64,
}

pub fn align_content_offsets(b: BoxRef<'_>, free_cross: f64, n: usize) -> ContentOffsets {
    let s = b.style();
    let mut acont = crate::ffi::keyword_or(s, PropId::AlignContent, c"stretch").to_bytes();
    let wrap_reverse = is_keyword(value(b, PropId::FlexWrap), c"wrap-reverse");
    if matches!(acont, b"start" | b"left" | b"self-start") {
        acont = if wrap_reverse {
            b"flex-end"
        } else {
            b"flex-start"
        };
    } else if matches!(acont, b"end" | b"right" | b"self-end") {
        acont = if wrap_reverse {
            b"flex-start"
        } else {
            b"flex-end"
        };
    }
    let mut out = ContentOffsets {
        lead: 0.0,
        between: 0.0,
        per_line: 0.0,
    };
    if n == 0 {
        return out;
    }
    if free_cross < 0.0 {
        if container_scrolls(b) {
            out.lead = if wrap_reverse { free_cross } else { 0.0 };
            return out;
        }
        if matches!(
            acont,
            b"space-between" | b"space-around" | b"space-evenly" | b"stretch" | b"normal"
        ) {
            return out;
        }
    }
    let lines = n as f64;
    match acont {
        b"stretch" | b"normal" => out.per_line = free_cross / lines,
        b"center" => out.lead = free_cross / 2.0,
        b"flex-end" | b"end" => out.lead = free_cross,
        b"space-between" => {
            out.between = if n > 1 {
                free_cross / (lines - 1.0)
            } else {
                0.0
            }
        }
        b"space-around" => {
            out.between = free_cross / lines;
            out.lead = out.between / 2.0;
        }
        b"space-evenly" => {
            out.between = free_cross / (lines + 1.0);
            out.lead = out.between;
        }
        _ => {}
    }
    out
}

pub fn main_height_outer(
    c: BoxRef<'_>,
    v: Option<ValueRef<'_>>,
    cross_size: f64,
    container_main_size: f64,
) -> f64 {
    let mut out = if value_is_percent(v) {
        resolve_height_with_basis(v, cross_size, container_main_size, 0.0)
    } else {
        length_resolve(v, cross_size, 0.0)
    };
    if !flex_box_is_border_box(c) {
        out += vertical_extras(c);
    }
    out
}

pub fn basis_main_height(c: BoxRef<'_>, cross_size: f64, container_main_size: f64) -> Option<f64> {
    let s = style_of(c)?;
    for prop in [PropId::FlexBasis, PropId::Height] {
        let v = s.get(prop);
        if is_length(v) {
            if value_is_percent(v) && container_main_size < 0.0 {
                return None;
            }
            return Some(main_height_outer(c, v, cross_size, container_main_size));
        }
    }
    None
}

pub fn relayout_after_cross_resize(
    c: BoxRef<'_>,
    layout_width: f64,
    main_size: f64,
    pre_h: f64,
    child_inherited: *const Style,
) {
    if c.first_child().is_none() {
        return;
    }
    let target_h = c.content_height();
    if (target_h - pre_h).abs() < 0.01 {
        return;
    }
    c.set_definite_height(target_h);
    c.set_flex_main(main_size);
    let (sx, sy) = (c.x(), c.y());
    layout_box(c, layout_width, child_inherited);
    if c.x() != sx || c.y() != sy {
        shift_box_tree(c, sx - c.x(), sy - c.y());
    }
    c.set_content_height(target_h);
}

pub fn align_stretches(align: &CStr) -> bool {
    matches!(align.to_bytes(), b"stretch" | b"normal")
}

pub fn cross_size_auto(c: BoxRef<'_>) -> bool {
    let h = value(c, PropId::Height);
    match h {
        None => !size_keyword_is_intrinsic(h),
        Some(v) if v.kind() == Kind::Keyword => !size_keyword_is_intrinsic(h),
        Some(_) => value_is_percent(h) && containing_block_definite_height(c) < 0.0,
    }
}

pub fn stretched_height(c: BoxRef<'_>, line_cross_size: f64, width_basis: f64) -> f64 {
    let vex = vertical_extras(c);
    let m = c.margin();
    let mut h = line_cross_size - m.top - m.bottom - vex;
    if h < 0.0 {
        h = 0.0;
    }
    let Some(s) = style_of(c) else { return h };
    let sizing_extras = if flex_box_is_border_box(c) { vex } else { 0.0 };
    let mx = resolve_used_height(c, s.get(PropId::MaxHeight), width_basis, -1.0);
    if mx >= 0.0 && h > mx - sizing_extras {
        h = if mx > sizing_extras {
            mx - sizing_extras
        } else {
            0.0
        };
    }
    let mn = resolve_used_height(c, s.get(PropId::MinHeight), width_basis, -1.0);
    if mn >= 0.0 && h < mn - sizing_extras {
        h = mn - sizing_extras;
    }
    h
}

pub fn fit_line_keyword_limits(c: BoxRef<'_>, line_cross_size: f64, width_basis: f64) {
    let Some(s) = style_of(c) else { return };
    let mxh = s.get(PropId::MaxHeight);
    let mnh = s.get(PropId::MinHeight);
    let max_stretches = height_keyword_stretches(mxh);
    let min_stretches = height_keyword_stretches(mnh);
    if !max_stretches && !min_stretches {
        return;
    }
    let vex = vertical_extras(c);
    let m = c.margin();
    let mut line_inner = line_cross_size - m.top - m.bottom - vex;
    if line_inner < 0.0 {
        line_inner = 0.0;
    }
    let mut min_h = line_inner;
    if !min_stretches {
        min_h = resolve_used_height(c, mnh, width_basis, -1.0);
        if min_h >= 0.0 && flex_box_is_border_box(c) {
            min_h -= vex;
        }
    }
    if max_stretches && c.content_height() > line_inner {
        c.set_content_height(line_inner);
    }
    if min_h >= 0.0 && c.content_height() < min_h {
        c.set_content_height(min_h);
    }
}

pub fn preset_cross_size(c: BoxRef<'_>, line_cross_size: f64, width_basis: f64) -> bool {
    if c.first_child().is_none() {
        return false;
    }
    let stretched = stretched_height(c, line_cross_size, width_basis);
    if stretched <= 0.0 {
        return false;
    }
    c.set_definite_height(stretched);
    true
}

pub fn is_rtl(b: BoxRef<'_>) -> bool {
    crate::ffi::keyword_or(b.style(), PropId::Direction, c"ltr") == c"rtl"
}

pub fn wrap_clamp_height(b: BoxRef<'_>, mut h: f64, width_basis: f64) -> f64 {
    let s = style_of(b);
    let mut mn = resolve_used_height(
        b,
        s.and_then(|s| s.get(PropId::MinHeight)),
        width_basis,
        -1.0,
    );
    let mut mx = resolve_used_height(
        b,
        s.and_then(|s| s.get(PropId::MaxHeight)),
        width_basis,
        -1.0,
    );
    if is_keyword(s.and_then(|s| s.get(PropId::BoxSizing)), c"border-box") {
        let vex = b.border().top + b.border().bottom + b.padding().top + b.padding().bottom;
        if mn > 0.0 {
            mn = max(mn - vex, 0.0);
        }
        if mx >= 0.0 {
            mx = max(mx - vex, 0.0);
        }
    }
    if mx >= 0.0 && h > mx {
        h = mx;
    }
    if mn > 0.0 && h < mn {
        h = mn;
    }
    h
}
