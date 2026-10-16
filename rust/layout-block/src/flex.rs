//! Southstar — the flex item and container helpers block, flex and grid layout share: box-sizing, grow and shrink factors, gaps and wrapping.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::BoxRef;
use southstar_layout_geom::length_resolve;
use southstar_style::{Kind, PropId, StyleRef, ValueRef};

use crate::ffi::style_of;

fn number_or(v: Option<ValueRef<'_>>, fallback: f64) -> f64 {
    v.and_then(ValueRef::length).map_or(fallback, |(n, _)| n)
}

pub fn box_is_border_box(c: BoxRef<'_>) -> bool {
    style_of(c)
        .and_then(|s| s.get(PropId::BoxSizing))
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| kw.to_bytes() == b"border-box")
}

pub fn grow_of(c: BoxRef<'_>) -> f64 {
    style_of(c).map_or(0.0, |s| number_or(s.get(PropId::FlexGrow), 0.0))
}

pub fn shrink_of(c: BoxRef<'_>) -> f64 {
    let Some(s) = style_of(c) else {
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
