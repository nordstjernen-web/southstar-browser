//! Southstar — the length, height and edge resolution the layout crates share: CSS lengths against a basis, used and definite heights, min/max clamping, box edges, and the style predicates and keywords layout reads.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

pub mod flex;
mod text;

pub use text::{utf8_skip, vertical_stack_text};

use core::ffi::CStr;

use southstar_layout::{BoxRef, Edges};
use southstar_style::{Display, Kind, PropId, StyleRef, ValueRef, box_style, display_of};

const UNIT_PX: u32 = 0;
const UNIT_EM: u32 = 1;
const UNIT_REM: u32 = 2;
const UNIT_PERCENT: u32 = 3;
const UNIT_NUMBER: u32 = 4;
const UNIT_VW: u32 = 5;
const UNIT_VH: u32 = 6;
const UNIT_VMIN: u32 = 7;
const UNIT_VMAX: u32 = 8;
const UNIT_CQW: u32 = 9;
const UNIT_CQH: u32 = 10;
const UNIT_CQMIN: u32 = 11;
const UNIT_CQMAX: u32 = 12;

pub const BOX_NORMAL: u8 = 0;
pub const BOX_NONE: u8 = 1;
pub const BOX_CONTENTS: u8 = 2;
pub const OUTER_INLINE: u8 = 0;
pub const OUTER_BLOCK: u8 = 1;
pub const INNER_FLOW: u8 = 0;
pub const INNER_TABLE: u8 = 2;
pub const INNER_FLEX: u8 = 3;
pub const INNER_GRID: u8 = 4;
pub const INNER_RUBY: u8 = 5;
pub const INTERNAL_NONE: u8 = 0;
pub const INTERNAL_TABLE_CAPTION: u8 = 8;

fn c_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn c_max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub fn keyword_is(v: Option<ValueRef<'_>>, kw: &CStr) -> bool {
    v.is_some_and(|v| v.is_keyword(kw))
}

pub fn length_or(v: Option<ValueRef<'_>>, fallback: f64) -> f64 {
    v.map_or(fallback, |v| v.length_or(fallback))
}

pub fn writing_mode(s: Option<StyleRef<'_>>) -> i32 {
    southstar_css::writing_mode_of(
        s.and_then(|s| s.get(PropId::WritingMode))
            .and_then(ValueRef::keyword_text),
    )
}

pub fn length_resolve(v: Option<ValueRef<'_>>, basis: f64, fallback: f64) -> f64 {
    let Some(v) = v else {
        return fallback;
    };
    if v.is_math_fn() {
        return v.math_fn_px(basis);
    }
    if let Some((pct, px)) = v.calc() {
        return pct / 100.0 * basis + px;
    }
    let Some((len, unit)) = v.length() else {
        return fallback;
    };
    match unit {
        UNIT_PX | UNIT_NUMBER => return len,
        UNIT_EM | UNIT_REM => return len * 16.0,
        UNIT_PERCENT => return len * basis / 100.0,
        _ => {}
    }
    let (vw, vh) = southstar_css::viewport();
    match unit {
        UNIT_VW => len * vw / 100.0,
        UNIT_VH => len * vh / 100.0,
        UNIT_VMIN => len * c_min(vw, vh) / 100.0,
        UNIT_VMAX => len * c_max(vw, vh) / 100.0,
        UNIT_CQW => {
            let cw = southstar_css::container_unit_dims().0;
            len * (if cw > 0.0 { cw } else { vw }) / 100.0
        }
        UNIT_CQH => {
            let ch = southstar_css::container_unit_dims().1;
            len * (if ch > 0.0 { ch } else { vh }) / 100.0
        }
        UNIT_CQMIN | UNIT_CQMAX => {
            let mut cw = southstar_css::container_unit_dims().0;
            if cw <= 0.0 {
                cw = vw;
            }
            let mut ch = southstar_css::container_unit_dims().1;
            if ch <= 0.0 {
                ch = vh;
            }
            let m = if unit == UNIT_CQMIN {
                c_min(cw, ch)
            } else {
                c_max(cw, ch)
            };
            len * m / 100.0
        }
        _ => fallback,
    }
}

fn length_resolve_nonnegative(v: Option<ValueRef<'_>>, basis: f64, fallback: f64) -> f64 {
    let resolved = length_resolve(v, basis, fallback);
    if resolved < 0.0 { 0.0 } else { resolved }
}

pub fn text_indent_px(s: Option<StyleRef<'_>>, basis: f64) -> f64 {
    match s.and_then(|s| s.get(PropId::TextIndent)) {
        Some(v) => length_resolve(Some(v), basis, 0.0),
        None => 0.0,
    }
}

pub fn inline_text_indent_px(run: BoxRef<'_>, s: Option<StyleRef<'_>>, basis: f64) -> f64 {
    if run.inline_split_tail() {
        return 0.0;
    }
    text_indent_px(s, basis)
}

fn length_is_auto(v: Option<ValueRef<'_>>) -> bool {
    keyword_is(v, c"auto")
}

pub fn aspect_ratio_number(v: Option<ValueRef<'_>>) -> (f64, bool) {
    let Some(sz) = v.and_then(ValueRef::size) else {
        return (-1.0, false);
    };
    if sz.w_unit != UNIT_NUMBER || sz.h_unit != UNIT_NUMBER {
        return (-1.0, false);
    }
    let ratio = if sz.w > 0.0 && sz.h > 0.0 {
        sz.w / sz.h
    } else {
        -1.0
    };
    (ratio, sz.w_auto)
}

pub fn value_is_percent(v: Option<ValueRef<'_>>) -> bool {
    let Some(v) = v else {
        return false;
    };
    if v.length().is_some_and(|(_, unit)| unit == UNIT_PERCENT) {
        return true;
    }
    if v.calc().is_some_and(|(pct, _)| pct != 0.0) {
        return true;
    }
    v.is_math_fn() && v.calc_arg_pcts().iter().any(|&p| p != 0.0)
}

fn box_is_doc_root(b: BoxRef<'_>) -> bool {
    b.dom()
        .and_then(|d| d.name())
        .is_some_and(|n| matches!(n.to_bytes(), b"html" | b"body"))
}

fn percent_of(hv: ValueRef<'_>, base: f64) -> f64 {
    match hv.calc() {
        Some((pct, px)) => pct / 100.0 * base + px,
        None => hv.length().map_or(0.0, |(v, _)| v) * base / 100.0,
    }
}

pub fn resolve_used_height(
    b: BoxRef<'_>,
    hv: Option<ValueRef<'_>>,
    width_basis: f64,
    fallback: f64,
) -> f64 {
    let Some(h) = hv else {
        return fallback;
    };
    if value_is_percent(hv) {
        let vh = if box_is_doc_root(b)
            || box_style(b).is_some_and(|s| keyword_is(s.get(PropId::Position), c"fixed"))
        {
            southstar_css::viewport().1
        } else {
            let base = containing_block_definite_height(b);
            if base < 0.0 {
                return fallback;
            }
            base
        };
        return percent_of(h, vh);
    }
    length_resolve(hv, width_basis, fallback)
}

fn non_percent_length(v: Option<ValueRef<'_>>) -> Option<ValueRef<'_>> {
    v.filter(|v| v.length().is_some_and(|(_, unit)| unit != UNIT_PERCENT))
}

pub fn clamp_height_minmax_px(s: Option<StyleRef<'_>>, mut h: f64) -> f64 {
    let Some(s) = s else {
        return h;
    };
    if h < 0.0 {
        return h;
    }
    if let Some(mx) = non_percent_length(s.get(PropId::MaxHeight)) {
        let m = length_resolve(Some(mx), 0.0, -1.0);
        if m >= 0.0 && h > m {
            h = m;
        }
    }
    if let Some(mn) = non_percent_length(s.get(PropId::MinHeight)) {
        let m = length_resolve(Some(mn), 0.0, -1.0);
        if m >= 0.0 && h < m {
            h = m;
        }
    }
    h
}

pub fn specified_height_to_content(b: BoxRef<'_>, h: f64) -> f64 {
    let border_box =
        box_style(b).is_some_and(|s| keyword_is(s.get(PropId::BoxSizing), c"border-box"));
    if h < 0.0 || !border_box {
        return h;
    }
    let (p, bd) = (b.padding(), b.border());
    let h = h - (p.top + p.bottom + bd.top + bd.bottom);
    if h < 0.0 { 0.0 } else { h }
}

pub fn containing_block_definite_height(b: BoxRef<'_>) -> f64 {
    if b.cb_height_override() > 0.0 {
        return b.cb_height_override();
    }
    let mut p = b.parent();
    while let Some(pb) = p.filter(|pb| pb.style().is_null()) {
        p = pb.parent();
    }
    let Some(p) = p else {
        return -1.0;
    };
    let Some(ps) = box_style(p) else {
        return -1.0;
    };
    if p.read_definite_height() > 0.0 {
        return p.definite_height();
    }
    let mut h = ps.get(PropId::Height);
    if height_keyword_stretches(h) {
        let base = containing_block_definite_height(p);
        if base < 0.0 {
            return -1.0;
        }
        let (m, bd, pd) = (p.margin(), p.border(), p.padding());
        let inner = base - m.top - m.bottom - bd.top - bd.bottom - pd.top - pd.bottom;
        return clamp_height_minmax_px(Some(ps), if inner > 0.0 { inner } else { 0.0 });
    }
    if h.is_some_and(|v| v.kind() == Kind::Keyword) {
        h = None;
    }
    let Some(hv) = h else {
        let top = ps.get(PropId::Top);
        let bottom = ps.get(PropId::Bottom);
        if style_is_absolute_or_fixed(Some(ps))
            && top.is_some()
            && !length_is_auto(top)
            && bottom.is_some()
            && !length_is_auto(bottom)
            && p.content_height() > 0.0
        {
            return p.content_height();
        }
        let (ratio, _) = aspect_ratio_number(ps.get(PropId::AspectRatio));
        if ratio > 0.0 && p.content_width() > 0.0 {
            return p.content_width() / ratio;
        }
        return -1.0;
    };
    if value_is_percent(h) {
        let base = if box_is_doc_root(p) {
            southstar_css::viewport().1
        } else {
            containing_block_definite_height(p)
        };
        if base < 0.0 {
            return -1.0;
        }
        return specified_height_to_content(
            p,
            clamp_height_minmax_px(Some(ps), percent_of(hv, base)),
        );
    }
    if p.content_height() > 0.0 {
        return p.content_height();
    }
    specified_height_to_content(
        p,
        clamp_height_minmax_px(Some(ps), length_resolve(h, 0.0, -1.0)),
    )
}

pub fn resolve_height_with_basis(
    hv: Option<ValueRef<'_>>,
    width_basis: f64,
    height_basis: f64,
    fallback: f64,
) -> f64 {
    let Some(v) = hv else {
        return fallback;
    };
    if let Some((pct, px)) = v.calc() {
        if v.is_math_fn() && height_basis >= 0.0 {
            return v.math_fn_px(height_basis);
        }
        if pct != 0.0 && height_basis >= 0.0 {
            return pct / 100.0 * height_basis + px;
        }
        return length_resolve(hv, width_basis, fallback);
    }
    if let Some((len, UNIT_PERCENT)) = v.length() {
        if height_basis >= 0.0 {
            return len * height_basis / 100.0;
        }
        return fallback;
    }
    length_resolve(hv, width_basis, fallback)
}

pub fn height_keyword_stretches(v: Option<ValueRef<'_>>) -> bool {
    v.and_then(ValueRef::keyword_text).is_some_and(|k| {
        matches!(
            k.to_bytes(),
            b"stretch" | b"-webkit-fill-available" | b"-moz-available"
        )
    })
}

pub fn is_block_level(d: Display) -> bool {
    d.box_ == BOX_NORMAL
        && d.outer == OUTER_BLOCK
        && d.inner != INNER_RUBY
        && (d.internal == INTERNAL_NONE || d.internal == INTERNAL_TABLE_CAPTION)
}

pub fn is_atomic_inline(d: Display) -> bool {
    d.box_ == BOX_NORMAL
        && d.internal == INTERNAL_NONE
        && d.outer == OUTER_INLINE
        && d.inner != INNER_FLOW
        && d.inner != INNER_RUBY
}

pub fn is_atomic_inline_container(d: Display) -> bool {
    is_atomic_inline(d) && d.inner != INNER_TABLE
}

pub fn generates_own_box(d: Display) -> bool {
    if d.box_ == BOX_NONE {
        return false;
    }
    if d.box_ == BOX_CONTENTS {
        return true;
    }
    is_block_level(d) || is_atomic_inline(d)
}

pub fn inner_is(d: Display, inner: u8) -> bool {
    d.box_ == BOX_NORMAL && d.internal == INTERNAL_NONE && d.inner == inner
}

pub fn is_table_internal(d: Display) -> bool {
    d.box_ == BOX_NORMAL && d.internal != INTERNAL_NONE && d.internal <= INTERNAL_TABLE_CAPTION
}

pub fn style_is_block(s: Option<StyleRef<'_>>) -> bool {
    generates_own_box(display_of(s))
}

pub fn style_is_block_level(s: Option<StyleRef<'_>>) -> bool {
    is_block_level(display_of(s))
}

pub fn style_is_flex_container(s: Option<StyleRef<'_>>) -> bool {
    inner_is(display_of(s), INNER_FLEX)
}

pub fn style_is_grid_container(s: Option<StyleRef<'_>>) -> bool {
    inner_is(display_of(s), INNER_GRID)
}

pub fn style_is_absolute_or_fixed(s: Option<StyleRef<'_>>) -> bool {
    let v = s.and_then(|s| s.get(PropId::Position));
    keyword_is(v, c"absolute") || keyword_is(v, c"fixed")
}

pub fn style_is_none(s: Option<StyleRef<'_>>) -> bool {
    display_of(s).box_ == BOX_NONE
}

pub fn style_is_contents(s: Option<StyleRef<'_>>) -> bool {
    display_of(s).box_ == BOX_CONTENTS
}

pub fn keyword_or<'a>(s: Option<StyleRef<'a>>, p: PropId, fallback: &'a CStr) -> &'a CStr {
    match s.and_then(|s| s.get(p)).and_then(ValueRef::keyword_text) {
        Some(kw) => southstar_css::alignment_base(kw),
        None => fallback,
    }
}

pub fn flex_direction_of<'a>(s: Option<StyleRef<'a>>) -> &'a CStr {
    let dir = keyword_or(s, PropId::FlexDirection, c"row");
    let writing_mode = writing_mode(s);
    if writing_mode == 0 {
        return dir;
    }
    let bytes = dir.to_bytes();
    let reverse = bytes.windows(8).any(|w| w == b"-reverse");
    if bytes.starts_with(b"row") {
        let rtl = keyword_is(s.and_then(|s| s.get(PropId::Direction)), c"rtl");
        return if reverse != rtl {
            c"column-reverse"
        } else {
            c"column"
        };
    }
    let right_to_left = writing_mode == 1;
    if reverse != right_to_left {
        c"row-reverse"
    } else {
        c"row"
    }
}

pub fn self_start_is_far_side(s: Option<StyleRef<'_>>, horizontal_axis: bool) -> bool {
    let writing_mode = writing_mode(s);
    let rtl = keyword_is(s.and_then(|s| s.get(PropId::Direction)), c"rtl");
    if horizontal_axis {
        return match writing_mode {
            1 => true,
            2 => false,
            _ => rtl,
        };
    }
    if writing_mode == 0 {
        return false;
    }
    let upward = keyword_is(s.and_then(|s| s.get(PropId::WritingMode)), c"sideways-lr");
    upward != rtl
}

fn border_side_width(s: StyleRef<'_>, width: PropId, style: PropId) -> f64 {
    let line = s.get(style);
    if line.is_none()
        || line
            .and_then(ValueRef::keyword_text)
            .is_some_and(|k| matches!(k.to_bytes(), b"none" | b"hidden"))
    {
        return 0.0;
    }
    length_or(s.get(width), 0.0)
}

pub fn edges_from_style(s: Option<StyleRef<'_>>, basis: f64) -> (Edges, Edges, Edges) {
    let Some(s) = s else {
        return (Edges::default(), Edges::default(), Edges::default());
    };
    let lr = |p| length_resolve(s.get(p), basis, 0.0);
    let ln = |p| length_resolve_nonnegative(s.get(p), basis, 0.0);
    let margin = Edges {
        top: lr(PropId::MarginTop),
        right: lr(PropId::MarginRight),
        bottom: lr(PropId::MarginBottom),
        left: lr(PropId::MarginLeft),
    };
    let padding = Edges {
        top: ln(PropId::PaddingTop),
        right: ln(PropId::PaddingRight),
        bottom: ln(PropId::PaddingBottom),
        left: ln(PropId::PaddingLeft),
    };
    let border = Edges {
        top: border_side_width(s, PropId::BorderTopWidth, PropId::BorderTopStyle),
        right: border_side_width(s, PropId::BorderRightWidth, PropId::BorderRightStyle),
        bottom: border_side_width(s, PropId::BorderBottomWidth, PropId::BorderBottomStyle),
        left: border_side_width(s, PropId::BorderLeftWidth, PropId::BorderLeftStyle),
    };
    (margin, padding, border)
}

pub fn edges_into_box(b: BoxRef<'_>, basis: f64) {
    let (margin, padding, border) = edges_from_style(box_style(b), basis);
    b.set_margin(margin);
    b.set_padding(padding);
    b.set_border(border);
}

pub fn style_creates_fixed_cb(s: Option<StyleRef<'_>>) -> bool {
    let Some(s) = s else {
        return false;
    };
    for p in [
        PropId::Transform,
        PropId::Translate,
        PropId::Rotate,
        PropId::Scale,
    ] {
        if s.get(p)
            .and_then(ValueRef::transform)
            .is_some_and(|t| t.n_ops > 0)
        {
            return true;
        }
    }
    s.get(PropId::Perspective)
        .and_then(ValueRef::length)
        .is_some_and(|(v, _)| v > 0.0)
}
