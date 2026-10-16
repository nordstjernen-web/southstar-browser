//! Southstar — min-content and max-content widths of boxes and their contributions, clamped by definite and keyword min/max widths, and the sizing keywords resolved against them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use core::sync::atomic::{AtomicI32, Ordering};

use southstar_css::TRACK_PX;
use southstar_layout::{BoxKind, BoxRef, Style, children};
use southstar_layout_float::{float_side_of, overflow_establishes_bfc, style_is_grid_container};
use southstar_style::{Kind, PropId, StyleRef, ValueRef};

use crate::ffi::{
    edges_of, flex_box_is_border_box, flex_direction_of, flex_gap_of, flex_grow_of, flex_shrink_of,
    flex_wraps, grid_column_flow_width, grid_flows_by_column, grid_natural_width, inline_min_width,
    inline_natural_width, length_resolve, style_is_flex_container, style_of, table_intrinsic_width,
    value_is_percent,
};
use crate::replaced::{
    inline_atomic_measure_basis, is_border_box, is_length_or_calc, is_replaced,
    replaced_intrinsic_contribution, replaced_width_is_cyclic, replaced_width_is_percent,
    style_value,
};

const UNIT_PX: u32 = 0;
const UNIT_NUMBER: u32 = 4;

static MIN_MEASURE_DEPTH: AtomicI32 = AtomicI32::new(0);

pub fn size_keyword_is_intrinsic(v: Option<ValueRef<'_>>) -> bool {
    v.and_then(ValueRef::keyword_text).is_some_and(|kw| {
        matches!(
            kw.to_bytes(),
            b"min-content" | b"max-content" | b"fit-content"
        )
    })
}

fn keyword_is(v: Option<ValueRef<'_>>, kw: &CStr) -> bool {
    v.is_some_and(|v| v.is_keyword(kw))
}

pub fn style_contains_inline_size(s: Option<StyleRef<'_>>) -> bool {
    s.and_then(|s| s.get(PropId::ContainerType))
        .and_then(ValueRef::keyword_text)
        .is_some_and(|kw| {
            let kw = kw.to_bytes();
            kw.eq_ignore_ascii_case(b"size") || kw.eq_ignore_ascii_case(b"inline-size")
        })
}

fn starts_row(dir: &[u8]) -> bool {
    dir.starts_with(b"row")
}

fn horizontal_edges(s: *const Style) -> f64 {
    let (m, pd, bd) = edges_of(s, 0.0);
    m.left + m.right + pd.left + pd.right + bd.left + bd.right
}

fn subtract_inline_padding_border(b: BoxRef<'_>, w: f64) -> f64 {
    let (_, pd, bd) = edges_of(b.style(), 0.0);
    let w = w - (pd.left + pd.right + bd.left + bd.right);
    if w < 0.0 { 0.0 } else { w }
}

pub fn measure_natural_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    match b.kind() {
        BoxKind::Inline => return inline_natural_width(b, parent_style),
        BoxKind::Image | BoxKind::Video | BoxKind::Svg => {
            if replaced_width_is_cyclic(b) {
                return replaced_intrinsic_contribution(b, true);
            }
            let wv = style_value(b, PropId::Width);
            if is_length_or_calc(wv) {
                let styled = length_resolve(wv, inline_atomic_measure_basis(b), -1.0);
                if styled >= 0.0 {
                    return styled;
                }
            }
            return if b.content_width() > 0.0 {
                b.content_width()
            } else {
                200.0
            };
        }
        BoxKind::Text => {
            return if b.content_width() > 0.0 {
                b.content_width()
            } else {
                0.0
            };
        }
        _ => {}
    }
    let wv = style_value(b, PropId::Width);
    let mut w = -1.0;
    match wv {
        Some(v) if v.kind() == Kind::Length => {
            if let Some((len, unit)) = v.length()
                && (unit == UNIT_PX || unit == UNIT_NUMBER)
            {
                w = len;
            }
        }
        Some(v) if v.kind() == Kind::Calc && !value_is_percent(wv) => {
            w = length_resolve(Some(v), 0.0, -1.0);
        }
        _ => {}
    }
    if w >= 0.0 {
        if flex_box_is_border_box(b) {
            w = subtract_inline_padding_border(b, w);
        }
        return width_contribution_keyword_limits(b, w, parent_style, true);
    }
    if keyword_is(wv, c"min-content") {
        return width_contribution_keyword_limits(
            b,
            min_content_width_of(b, parent_style),
            parent_style,
            true,
        );
    }
    width_contribution_keyword_limits(
        b,
        measure_max_content_width(b, parent_style),
        parent_style,
        true,
    )
}

fn definite_width_limit(b: BoxRef<'_>, v: Option<ValueRef<'_>>) -> f64 {
    if !is_length_or_calc(v) || value_is_percent(v) {
        return -1.0;
    }
    let limit = length_resolve(v, 0.0, -1.0);
    if limit < 0.0 {
        return -1.0;
    }
    if flex_box_is_border_box(b) {
        return subtract_inline_padding_border(b, limit);
    }
    limit
}

fn width_contribution_keyword_limits(
    b: BoxRef<'_>,
    mut w: f64,
    parent_style: *const Style,
    max_content: bool,
) -> f64 {
    let Some(style) = style_of(b) else {
        return w;
    };
    if !matches!(b.kind(), BoxKind::Block | BoxKind::Table) {
        return w;
    }
    let mxw = style.get(PropId::MaxWidth);
    let mnw = style.get(PropId::MinWidth);
    let max_limit = definite_width_limit(b, mxw);
    if max_limit >= 0.0 && w > max_limit {
        w = max_limit;
    }
    let min_limit = definite_width_limit(b, mnw);
    if min_limit >= 0.0 && w < min_limit {
        w = min_limit;
    }
    let keyword_measure = |v: Option<ValueRef<'_>>| {
        let use_max =
            keyword_is(v, c"max-content") || (max_content && keyword_is(v, c"fit-content"));
        if use_max {
            measure_max_content_width(b, parent_style)
        } else {
            min_content_width_of(b, parent_style)
        }
    };
    if size_keyword_is_intrinsic(mxw) {
        let m = keyword_measure(mxw);
        if m >= 0.0 && w > m {
            w = m;
        }
    }
    if size_keyword_is_intrinsic(mnw) {
        let m = keyword_measure(mnw);
        if m >= 0.0 && w < m {
            w = m;
        }
    }
    w
}

pub fn measure_max_content_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    if matches!(b.kind(), BoxKind::Inline | BoxKind::Text) || is_replaced(b) {
        return measure_natural_width(b, parent_style);
    }
    let style = style_of(b);
    if style_contains_inline_size(style) {
        return 0.0;
    }
    let child_style = if b.style().is_null() {
        parent_style
    } else {
        b.style()
    };
    if b.kind() == BoxKind::Table {
        return table_intrinsic_width(b, child_style, false);
    }
    if style.is_some() && style_is_grid_container(style) {
        let gw = grid_natural_width(b, child_style);
        if gw > 0.0 {
            return gw;
        }
    }
    let flex_row = style_is_flex_container(b.style()) && starts_row(flex_direction_of(b.style()));
    let mut max_child = 0.0;
    let mut float_row = 0.0;
    let mut row_sum = 0.0;
    let mut flex_items = 0;
    for c in children(b) {
        let w = measure_natural_width(c, child_style);
        let fside = float_side_of(style_of(c));
        let mut outer = w;
        if !c.style().is_null() {
            outer += horizontal_edges(c.style());
        }
        if flex_row {
            row_sum += outer;
            flex_items += 1;
        } else if matches!(
            b.kind(),
            BoxKind::Block | BoxKind::TableCell | BoxKind::TableCaption
        ) {
            if fside >= 0 {
                float_row += outer;
                if float_row > max_child {
                    max_child = float_row;
                }
            } else {
                if outer > max_child {
                    max_child = outer;
                }
                if w > max_child {
                    max_child = w;
                }
                float_row = 0.0;
            }
        } else {
            max_child += w;
        }
    }
    if flex_row
        && flex_items > 1
        && let Some(style) = style
    {
        let mut g = style.get(PropId::ColumnGap);
        if !g.is_some_and(|g| g.kind() == Kind::Length) {
            g = style.get(PropId::Gap);
        }
        let gap = g.and_then(ValueRef::length).map_or(0.0, |(v, _)| v);
        row_sum += gap * f64::from(flex_items - 1);
    }
    if flex_row { row_sum } else { max_child }
}

pub fn min_width_of(b: Option<BoxRef<'_>>, parent_style: *const Style) -> f64 {
    MIN_MEASURE_DEPTH.fetch_add(1, Ordering::Relaxed);
    let w = b.map_or(0.0, |b| measure_min_width(b, parent_style));
    MIN_MEASURE_DEPTH.fetch_sub(1, Ordering::Relaxed);
    w
}

fn measure_min_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    match b.kind() {
        BoxKind::Inline => return inline_min_width(b, parent_style),
        BoxKind::Image | BoxKind::Video | BoxKind::Svg => {
            if style_value(b, PropId::MaxWidth)
                .and_then(ValueRef::length)
                .is_some_and(|(_, unit)| unit == southstar_style::UNIT_PERCENT)
            {
                return 0.0;
            }
            if replaced_width_is_cyclic(b)
                || (MIN_MEASURE_DEPTH.load(Ordering::Relaxed) > 1 && replaced_width_is_percent(b))
            {
                return replaced_intrinsic_contribution(b, false);
            }
            let wv = style_value(b, PropId::Width);
            if is_length_or_calc(wv) && !value_is_percent(wv) {
                let styled = length_resolve(wv, inline_atomic_measure_basis(b), -1.0);
                if styled >= 0.0 {
                    return styled;
                }
            }
            return if b.content_width() > 0.0 {
                b.content_width()
            } else {
                200.0
            };
        }
        BoxKind::Text => {
            return if b.content_width() > 0.0 {
                b.content_width()
            } else {
                0.0
            };
        }
        _ => {}
    }
    let style = style_of(b);
    if style_contains_inline_size(style) {
        return 0.0;
    }
    if let Some(style) = style {
        let wv = style.get(PropId::Width);
        if is_length_or_calc(wv) {
            let mut w = length_resolve(wv, 0.0, -1.0);
            if w >= 0.0 {
                if is_border_box(b) {
                    w = subtract_inline_padding_border(b, w);
                }
                let mxw = style.get(PropId::MaxWidth);
                if is_length_or_calc(mxw) {
                    let m = length_resolve(mxw, 0.0, -1.0);
                    if m >= 0.0 && w > m {
                        w = m;
                    }
                }
                let mnw = style.get(PropId::MinWidth);
                if is_length_or_calc(mnw) {
                    let m = length_resolve(mnw, 0.0, -1.0);
                    if m >= 0.0 && w < m {
                        w = m;
                    }
                }
                return width_contribution_keyword_limits(b, w, parent_style, false);
            }
        }
        if keyword_is(wv, c"max-content") {
            return width_contribution_keyword_limits(
                b,
                measure_max_content_width(b, parent_style),
                parent_style,
                false,
            );
        }
    }
    width_contribution_keyword_limits(
        b,
        min_content_width_of(b, parent_style),
        parent_style,
        false,
    )
}

fn flex_contribution_clamp_to_basis(c: BoxRef<'_>, contribution: f64, preferred: f64) -> f64 {
    let bv = style_value(c, PropId::FlexBasis);
    let mut base = definite_width_limit(c, bv);
    if base < 0.0 && (bv.is_none() || keyword_is(bv, c"auto")) {
        base = preferred;
    }
    if base < 0.0 {
        return contribution;
    }
    if flex_grow_of(c) <= 0.0 && contribution > base {
        return base;
    }
    if flex_shrink_of(c) <= 0.0 && contribution < base {
        return base;
    }
    contribution
}

fn c_max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn c_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn flex_row_item_min_contribution(c: BoxRef<'_>, child_style: *const Style) -> f64 {
    let Some(s) = style_of(c).filter(|_| !matches!(c.kind(), BoxKind::Inline | BoxKind::Text))
    else {
        return min_width_of(Some(c), child_style);
    };
    let min_content = min_content_width_of(c, child_style);
    let preferred = definite_width_limit(c, s.get(PropId::Width));
    let mut contribution =
        flex_contribution_clamp_to_basis(c, c_max(min_content, preferred), preferred);
    let max_main = definite_width_limit(c, s.get(PropId::MaxWidth));
    if max_main >= 0.0 && contribution > max_main {
        contribution = max_main;
    }
    let mut min_main = definite_width_limit(c, s.get(PropId::MinWidth));
    if min_main < 0.0 && !overflow_establishes_bfc(Some(s)) {
        min_main = if preferred >= 0.0 {
            c_min(min_content, preferred)
        } else {
            min_content
        };
    }
    if min_main >= 0.0 && contribution < min_main {
        contribution = min_main;
    }
    contribution
}

pub fn min_content_width_of(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    MIN_MEASURE_DEPTH.fetch_add(1, Ordering::Relaxed);
    let w = measure_min_content_width(b, parent_style);
    MIN_MEASURE_DEPTH.fetch_sub(1, Ordering::Relaxed);
    w
}

fn template_columns_min_width(style: StyleRef<'_>) -> Option<f64> {
    let tk = style.get(PropId::GridTemplateColumns)?.tracks()?;
    if tk.n <= 0 || tk.subgrid != 0 {
        return None;
    }
    let n = tk.n as usize;
    let mut sum = 0.0;
    for t in tk.tracks.iter().take(n) {
        if t.kind == TRACK_PX {
            sum += t.v;
        } else if t.has_min != 0 && t.min_kind == TRACK_PX {
            sum += t.min_v;
        } else {
            return None;
        }
    }
    if sum.is_nan() || sum <= 0.0 {
        return None;
    }
    let mut gv = style.get(PropId::ColumnGap);
    if !is_length_or_calc(gv) {
        gv = style.get(PropId::Gap);
    }
    if is_length_or_calc(gv) && n > 1 {
        let gap = length_resolve(gv, 0.0, 0.0);
        if gap > 0.0 {
            sum += gap * (n - 1) as f64;
        }
    }
    Some(sum)
}

fn measure_min_content_width(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    if matches!(b.kind(), BoxKind::Inline | BoxKind::Text) || is_replaced(b) {
        return min_width_of(Some(b), parent_style);
    }
    let style = style_of(b);
    if style_contains_inline_size(style) {
        return 0.0;
    }
    if b.kind() == BoxKind::Table {
        let inherited = if b.style().is_null() {
            parent_style
        } else {
            b.style()
        };
        return table_intrinsic_width(b, inherited, true);
    }
    if let Some(s) = style
        && style_is_grid_container(style)
    {
        if grid_flows_by_column(s.as_ptr()) {
            let gw = grid_column_flow_width(b, s.as_ptr(), true);
            if gw >= 0.0 {
                return gw;
            }
        }
        if let Some(sum) = template_columns_min_width(s) {
            return sum;
        }
    }
    let child_style = if b.style().is_null() {
        parent_style
    } else {
        b.style()
    };
    let single_line_row = style_is_flex_container(b.style())
        && starts_row(flex_direction_of(b.style()))
        && !flex_wraps(b.style());
    let mut max_child = 0.0;
    let mut row_sum = 0.0;
    let mut items = 0;
    for c in children(b) {
        let w = if single_line_row {
            flex_row_item_min_contribution(c, child_style)
        } else {
            min_width_of(Some(c), child_style)
        };
        let mut outer = w;
        if !c.style().is_null() {
            outer += horizontal_edges(c.style());
        }
        if outer > max_child {
            max_child = outer;
        }
        row_sum += outer;
        items += 1;
    }
    if !single_line_row {
        return max_child;
    }
    if items > 1 {
        row_sum += flex_gap_of(b.style(), 0.0) * f64::from(items - 1);
    }
    row_sum
}

pub fn intrinsic_keyword_width(b: BoxRef<'_>, kw: &CStr, mi: *const Style, mut avail: f64) -> f64 {
    if avail < 0.0 {
        avail = 0.0;
    }
    match kw.to_bytes() {
        b"min-content" => min_content_width_of(b, mi),
        b"max-content" => measure_max_content_width(b, mi),
        b"fit-content" => {
            let mn = min_content_width_of(b, mi);
            let mx = measure_max_content_width(b, mi);
            let w = if mx < avail { mx } else { avail };
            if w < mn { mn } else { w }
        }
        b"stretch" | b"-webkit-fill-available" | b"-moz-available" => avail,
        _ => -1.0,
    }
}
