//! Southstar — the quick natural-width estimate of a box before it is laid out, and the intrinsic width of a text input from its size attribute.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxKind, BoxRef, children};
use southstar_layout_geom::{
    edges_from_style, flex_direction_of, length_or, length_resolve, style_is_absolute_or_fixed,
    style_is_flex_container,
};
use southstar_style::{Kind, PropId, StyleRef, UNIT_PERCENT, ValueRef};

use crate::ffi::{dom_of, parse_int, style_of, viewport_w};
use crate::intrinsic::style_contains_inline_size;
use crate::replaced::{is_length_or_calc, is_replaced};

const UNIT_PX: u32 = 0;
const UNIT_EM: u32 = 1;
const UNIT_REM: u32 = 2;
const UNIT_NUMBER: u32 = 4;
const UNIT_VW: u32 = 5;

const TEXT_INPUT_TYPES: &[&[u8]] = &[
    b"text",
    b"password",
    b"search",
    b"email",
    b"url",
    b"tel",
    b"number",
];

fn px_font_size(s: StyleRef<'_>) -> Option<Option<f64>> {
    let fs = s.get(PropId::FontSize)?;
    Some(
        fs.length()
            .filter(|&(_, unit)| unit == UNIT_PX)
            .map(|(v, _)| v),
    )
}

fn font_size_of(b: BoxRef<'_>) -> f64 {
    if let Some(own) = style_of(b).and_then(px_font_size) {
        return own.unwrap_or(16.0);
    }
    let mut p = b.parent();
    while let Some(anc) = p {
        if let Some(Some(px)) = style_of(anc).and_then(px_font_size) {
            return px;
        }
        p = anc.parent();
    }
    16.0
}

fn style_border_box(s: StyleRef<'_>) -> bool {
    s.get(PropId::BoxSizing)
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| kw.to_bytes() == b"border-box")
}

fn specified_width(b: BoxRef<'_>, s: StyleRef<'_>, font_size: f64, cap: f64) -> Option<f64> {
    let (v, unit) = s.get(PropId::Width)?.length()?;
    if v.is_nan() || v <= 0.0 {
        return None;
    }
    let mut sw = match unit {
        UNIT_PX | UNIT_NUMBER => v,
        UNIT_EM => v * font_size,
        UNIT_REM => v * 16.0,
        UNIT_VW => v * viewport_w() / 100.0,
        _ => -1.0,
    };
    if sw.is_nan() || sw <= 0.0 {
        return None;
    }
    if !style_border_box(s) {
        let (p, bd) = (b.padding(), b.border());
        sw += p.left + p.right + bd.left + bd.right;
    }
    Some(if sw > cap { cap } else { sw })
}

fn count_non_object_chars(text: &[u8]) -> f64 {
    let mut chars = 0.0;
    let mut i = 0;
    while i < text.len() {
        let lead = text[i];
        let skip = match lead {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            0xF8..=0xFB => 5,
            0xFC..=0xFD => 6,
            _ => 1,
        };
        if text[i..].get(..3) != Some(&[0xEF, 0xBF, 0xBC][..]) {
            chars += 1.0;
        }
        i += skip;
    }
    chars
}

fn is_column_direction(dir: &[u8]) -> bool {
    dir == b"column" || dir == b"column-reverse"
}

fn inline_text_width(b: BoxRef<'_>, text: &[u8], font_size: f64, cap: f64) -> f64 {
    let mut w = count_non_object_chars(text) * font_size * 0.65 + font_size * 0.5;
    for ab in b.inline_atomic_boxes() {
        let (p, bd, m) = (ab.padding(), ab.border(), ab.margin());
        let aw = if ab.content_width() > 0.0 {
            ab.content_width() + p.left + p.right + bd.left + bd.right
        } else {
            estimate_natural_width(ab, cap)
        };
        w += aw + m.left + m.right;
    }
    w
}

fn flow_children_width(b: BoxRef<'_>, cap: f64) -> f64 {
    let style = style_of(b);
    let column_flex = style.is_some()
        && style_is_flex_container(style_of(b))
        && is_column_direction(flex_direction_of(style_of(b)).to_bytes());
    let mut w = 0.0;
    let mut flow_children = 0;
    for c in children(b) {
        if !c.style().is_null()
            && !core::ptr::eq(c.style(), b.style())
            && style_is_absolute_or_fixed(style_of(c))
        {
            continue;
        }
        let mut cw_child = estimate_natural_width(c, cap);
        if let Some(cs) = style_of(c)
            && (cs.get(PropId::MarginLeft).is_some() || cs.get(PropId::MarginRight).is_some())
        {
            let (m, _, _) = edges_from_style(style_of(c), 0.0);
            cw_child += m.left + m.right;
        }
        if column_flex {
            if cw_child > w {
                w = cw_child;
            }
        } else {
            w += cw_child;
        }
        flow_children += 1;
    }
    if flow_children > 1
        && style_is_flex_container(style_of(b))
        && !is_column_direction(flex_direction_of(style_of(b)).to_bytes())
        && let Some(s) = style
    {
        let cg = s
            .get(PropId::ColumnGap)
            .filter(|v| v.kind() == Kind::Length);
        let gg = s.get(PropId::Gap).filter(|v| v.kind() == Kind::Length);
        if let Some((v, unit)) = cg.or(gg).and_then(ValueRef::length)
            && unit != UNIT_PERCENT
        {
            w += f64::from(flow_children - 1) * v;
        }
    }
    w
}

pub fn estimate_natural_width(b: BoxRef<'_>, cap: f64) -> f64 {
    let font_size = font_size_of(b);
    let style = style_of(b);
    if let Some(s) = style
        && let Some(sw) = specified_width(b, s, font_size, cap)
    {
        return sw;
    }
    if style_contains_inline_size(style) {
        return 0.0;
    }
    let mut w = match (b.kind(), b.text()) {
        (BoxKind::Inline, Some(text)) => inline_text_width(b, text.to_bytes(), font_size, cap),
        _ if is_replaced(b) => {
            if b.content_width() > 0.0 {
                b.content_width()
            } else {
                0.0
            }
        }
        _ => flow_children_width(b, cap),
    };
    match style {
        Some(s) => {
            if s.get(PropId::PaddingLeft).is_some()
                || s.get(PropId::PaddingRight).is_some()
                || s.get(PropId::BorderLeftWidth).is_some()
                || s.get(PropId::BorderRightWidth).is_some()
            {
                let (_, bpd, bbd) = edges_from_style(style_of(b), cap);
                w += bpd.left + bpd.right + bbd.left + bbd.right;
            }
        }
        None => {
            let (p, bd) = (b.padding(), b.border());
            w += p.left + p.right + bd.left + bd.right;
        }
    }
    if let Some(s) = style {
        let (p, bd) = (b.padding(), b.border());
        let box_extras = if style_border_box(s) {
            0.0
        } else {
            p.left + p.right + bd.left + bd.right
        };
        let mxw = s.get(PropId::MaxWidth);
        if is_length_or_calc(mxw) {
            let mx = length_resolve(mxw, cap, -1.0);
            if mx >= 0.0 && w > mx + box_extras {
                w = mx + box_extras;
            }
        }
        let mnw = s.get(PropId::MinWidth);
        if is_length_or_calc(mnw) {
            let mn = length_resolve(mnw, cap, -1.0);
            if mn > 0.0 && w < mn + box_extras {
                w = mn + box_extras;
            }
        }
    }
    if w > cap {
        w = cap;
    }
    w
}

pub fn text_input_intrinsic_width(b: BoxRef<'_>) -> f64 {
    let Some(dom) = dom_of(b) else {
        return 0.0;
    };
    if dom.name().map(|n| n.to_bytes()) != Some(b"input".as_slice()) {
        return 0.0;
    }
    if let Some(ty) = dom.attr(c"type")
        && !ty.is_empty()
        && !TEXT_INPUT_TYPES
            .iter()
            .any(|t| ty.to_bytes().eq_ignore_ascii_case(t))
    {
        return 0.0;
    }
    let size = dom.attr(c"size").map_or(20, |s| parse_int(s, 20, 1, 80));
    let font_size = length_or(style_of(b).and_then(|s| s.get(PropId::FontSize)), 16.0);
    f64::from(size) * font_size * 0.75
}
