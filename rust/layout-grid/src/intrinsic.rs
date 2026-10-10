//! Southstar — the max-content and column-flow min-content widths of grid containers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_css::{AUTO_REPEAT_NONE, TRACK_PX};
use southstar_layout::{BoxRef, Edges, Style, children};
use southstar_style::{Kind, PropId, StyleRef, ValueRef};

use crate::ffi::{self, style_of};
use crate::tracks::{MAX, count};

pub(crate) fn is_length_or_calc(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc))
}

pub(crate) fn is_absolute_or_fixed(s: Option<StyleRef<'_>>) -> bool {
    s.and_then(|s| s.get(PropId::Position))
        .is_some_and(|v| v.is_keyword(c"absolute") || v.is_keyword(c"fixed"))
}

pub(crate) fn gap_px(
    specific: Option<ValueRef<'_>>,
    shorthand: Option<ValueRef<'_>>,
    basis: f64,
) -> f64 {
    let v = specific.or(shorthand);
    if !is_length_or_calc(v) {
        return 0.0;
    }
    let resolved = ffi::resolve_length(v, basis, 0.0);
    if resolved < 0.0 { 0.0 } else { resolved }
}

pub(crate) fn horizontal_extras(m: &Edges, p: &Edges, b: &Edges) -> f64 {
    m.left + m.right + p.left + p.right + b.left + b.right
}

fn outer_width(c: BoxRef<'_>, w: f64) -> f64 {
    match style_of(c) {
        Some(s) => {
            let (m, p, b) = ffi::edges(s.as_ptr(), 0.0);
            w + horizontal_extras(&m, &p, &b)
        }
        None => w,
    }
}

pub(crate) fn flows_by_column(s: Option<StyleRef<'_>>) -> bool {
    s.and_then(|s| s.get(PropId::GridAutoFlow))
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| kw.to_bytes().windows(6).any(|w| w == b"column"))
}

fn explicit_row_count(s: Option<StyleRef<'_>>) -> usize {
    match s
        .and_then(|s| s.get(PropId::GridTemplateRows))
        .and_then(ValueRef::tracks)
    {
        Some(t) if t.subgrid == 0 && t.auto_repeat == AUTO_REPEAT_NONE && t.n > 1 => t.n as usize,
        _ => 1,
    }
}

pub(crate) fn column_flow_width(
    b: BoxRef<'_>,
    child_style: *const Style,
    min_content: bool,
) -> f64 {
    let style = style_of(b);
    let rows = explicit_row_count(style);
    let mut sum = 0.0;
    let mut column = 0.0;
    let mut in_column = 0;
    let mut columns = 0;
    for c in children(b) {
        if is_absolute_or_fixed(style_of(c)) {
            continue;
        }
        let w = if min_content {
            ffi::min_width(c, child_style)
        } else {
            ffi::natural_width(c, child_style)
        };
        let w = outer_width(c, w);
        if w > column {
            column = w;
        }
        in_column += 1;
        if in_column == rows {
            sum += column;
            column = 0.0;
            in_column = 0;
            columns += 1;
        }
    }
    if in_column > 0 {
        sum += column;
        columns += 1;
    }
    if columns == 0 {
        return -1.0;
    }
    let gap = style.map_or(0.0, |s| {
        gap_px(s.get(PropId::ColumnGap), s.get(PropId::Gap), 0.0)
    });
    sum + gap * f64::from(columns - 1)
}

pub(crate) fn natural_width(b: BoxRef<'_>, child_style: *const Style) -> f64 {
    let style = style_of(b);
    if flows_by_column(style) {
        return column_flow_width(b, child_style, false);
    }
    let Some(style) = style else {
        return -1.0;
    };
    let Some(tk) = style
        .get(PropId::GridTemplateColumns)
        .and_then(ValueRef::tracks)
        .filter(|t| t.n > 0 && t.subgrid == 0 && t.auto_repeat == AUTO_REPEAT_NONE)
    else {
        return -1.0;
    };
    let n = count(tk);
    let mut col = [0.0f64; MAX];
    let mut slot = 0usize;
    for c in children(b) {
        if is_absolute_or_fixed(style_of(c)) {
            continue;
        }
        let w = outer_width(c, ffi::natural_width(c, child_style));
        let t = slot % n;
        if w > col[t] {
            col[t] = w;
        }
        slot += 1;
    }
    if slot == 0 {
        return -1.0;
    }
    let mut sum = 0.0;
    for (i, &width) in col[..n].iter().enumerate() {
        let t = &tk.tracks[i];
        let mut track = width;
        if t.kind == TRACK_PX {
            track = t.v;
        } else if t.has_min != 0 && t.min_kind == TRACK_PX && t.min_v > track {
            track = t.min_v;
        }
        sum += track;
    }
    if n > 1 {
        let mut gv = style.get(PropId::ColumnGap);
        if !is_length_or_calc(gv) {
            gv = style.get(PropId::Gap);
        }
        if is_length_or_calc(gv) {
            let gap = ffi::resolve_length(gv, 0.0, 0.0);
            if gap > 0.0 {
                sum += gap * (n - 1) as f64;
            }
        }
    }
    sum
}
