//! Southstar — the flex item properties, gaps and baseline alignment the block, flex, sizing, inline and positioning crates share.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_layout::{BoxKind, BoxRef};
use southstar_style::{Kind, PropId, StyleRef, ValueRef, box_style};

use crate::{length_resolve, style_is_absolute_or_fixed, writing_mode};

fn number_or(v: Option<ValueRef<'_>>, fallback: f64) -> f64 {
    v.and_then(ValueRef::length).map_or(fallback, |(n, _)| n)
}

pub fn box_is_border_box(c: BoxRef<'_>) -> bool {
    box_style(c)
        .and_then(|s| s.get(PropId::BoxSizing))
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| kw.to_bytes() == b"border-box")
}

pub fn grow_of(c: BoxRef<'_>) -> f64 {
    box_style(c).map_or(0.0, |s| number_or(s.get(PropId::FlexGrow), 0.0))
}

pub fn shrink_of(c: BoxRef<'_>) -> f64 {
    let Some(s) = box_style(c) else {
        return 1.0;
    };
    let shrink = number_or(s.get(PropId::FlexShrink), 1.0);
    if shrink < 0.0 { 0.0 } else { shrink }
}

pub fn gap_px(specific: Option<ValueRef<'_>>, shorthand: Option<ValueRef<'_>>, basis: f64) -> f64 {
    let Some(v) = specific.or(shorthand) else {
        return 0.0;
    };
    if !matches!(v.kind(), Kind::Length | Kind::Calc) {
        return 0.0;
    }
    let resolved = length_resolve(Some(v), basis, 0.0);
    if resolved < 0.0 { 0.0 } else { resolved }
}

pub fn gap_of(s: Option<StyleRef<'_>>, basis: f64) -> f64 {
    s.map_or(0.0, |s| {
        gap_px(s.get(PropId::ColumnGap), s.get(PropId::Gap), basis)
    })
}

pub fn wraps(s: Option<StyleRef<'_>>) -> bool {
    s.and_then(|s| s.get(PropId::FlexWrap))
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| matches!(kw.to_bytes(), b"wrap" | b"wrap-reverse"))
}

pub fn box_first_baseline(b: BoxRef<'_>) -> Option<f64> {
    if b.kind() == BoxKind::Inline {
        if b.first_baseline() <= 0.0 {
            return None;
        }
        return Some(b.margin().top + b.border().top + b.padding().top + b.first_baseline());
    }
    let mut c = b.first_child();
    while let Some(child) = c {
        c = child.next_sibling();
        if style_is_absolute_or_fixed(box_style(child)) || child.is_rendered_legend() {
            continue;
        }
        if let Some(cb) = box_first_baseline(child) {
            return Some((child.y() - b.y()) + cb);
        }
    }
    None
}

pub fn item_align<'a>(c: BoxRef<'a>, container_align: &'a CStr) -> &'a CStr {
    box_style(c)
        .and_then(|s| s.keyword_of(PropId::AlignSelf))
        .filter(|a| a.to_bytes() != b"auto")
        .unwrap_or(container_align)
}

pub fn align_is_baseline(align: &CStr) -> bool {
    matches!(align.to_bytes(), b"baseline" | b"first baseline")
}

pub fn item_baseline(c: BoxRef<'_>, fallback: f64) -> f64 {
    if let Some(p) = c.parent()
        && writing_mode(box_style(p)) != 0
        && writing_mode(box_style(c)) != 0
    {
        let (pd, bd) = (c.padding(), c.border());
        let border_h = c.content_height() + pd.top + pd.bottom + bd.top + bd.bottom;
        return c.margin().top + border_h / 2.0;
    }
    box_first_baseline(c).unwrap_or(fallback)
}
