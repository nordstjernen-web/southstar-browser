//! Southstar — laying out a table box: its columns, rows, cells and captions, and its min- and max-content widths.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ops::Range;

use southstar_layout::{BoxKind, BoxRef, Edges, Style, children};
use southstar_layout_geom::{
    edges_from_style, edges_into_box, length_resolve, resolve_used_height, value_is_percent,
};
use southstar_style::{Kind, PropId, UNIT_PERCENT, ValueRef, box_style};

use crate::ffi::{
    layout_block, layout_box, legacy_align_block_child, measure_natural_width,
    min_content_width_of, min_width_of, shift_box_tree, style_of, translate_subtree,
};
use crate::grid::{self, Placement, Span};

fn value(s: *const Style, prop: PropId) -> Option<ValueRef<'static>> {
    style_of(s).and_then(|s| s.get(prop))
}

fn is_length_or_calc(v: ValueRef<'_>) -> bool {
    matches!(v.kind(), Kind::Length | Kind::Calc)
}

fn definite(v: Option<ValueRef<'_>>) -> Option<ValueRef<'_>> {
    v.filter(|v| is_length_or_calc(*v))
}

fn keyword_is(v: Option<ValueRef<'_>>, kw: &[u8]) -> bool {
    v.and_then(ValueRef::keyword_text)
        .is_some_and(|k| k.to_bytes() == kw)
}

fn keyword_is_ignore_case(v: Option<ValueRef<'_>>, kw: &[u8]) -> bool {
    v.and_then(ValueRef::keyword_text)
        .is_some_and(|k| k.to_bytes().eq_ignore_ascii_case(kw))
}

fn rows<'a>(b: BoxRef<'a>) -> impl Iterator<Item = BoxRef<'a>> {
    children(b).filter(|r| r.kind() == BoxKind::TableRow)
}

fn colspan(cell: BoxRef<'_>) -> usize {
    if cell.colspan() > 0 {
        cell.colspan() as usize
    } else {
        1
    }
}

fn rowspan(cell: BoxRef<'_>) -> usize {
    if cell.rowspan() > 0 {
        cell.rowspan() as usize
    } else {
        1
    }
}

fn hint_span(span: i32) -> usize {
    if span > 0 { span as usize } else { 1 }
}

fn margin_padding_border_h(m: &Edges, p: &Edges, b: &Edges) -> f64 {
    m.left + m.right + p.left + p.right + b.left + b.right
}

fn padding_border_margin_h(m: &Edges, p: &Edges, b: &Edges) -> f64 {
    p.left + p.right + b.left + b.right + m.left + m.right
}

fn outer_v(b: BoxRef<'_>) -> f64 {
    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    m.top + m.bottom + p.top + p.bottom + bd.top + bd.bottom
}

fn plus_outer_v(x: f64, b: BoxRef<'_>) -> f64 {
    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    x + m.top + m.bottom + p.top + p.bottom + bd.top + bd.bottom
}

fn minus_outer_v(x: f64, b: BoxRef<'_>) -> f64 {
    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    x - m.top - m.bottom - p.top - p.bottom - bd.top - bd.bottom
}

fn styled_or(b: BoxRef<'_>, fallback: *const Style) -> *const Style {
    if b.style().is_null() {
        fallback
    } else {
        b.style()
    }
}

#[derive(Clone, Copy)]
struct GridCell<'a> {
    cell: BoxRef<'a>,
    at: Placement,
}

fn grid_cells(b: BoxRef<'_>) -> (Vec<GridCell<'_>>, usize) {
    let row_count = rows(b).count();
    let spans = rows(b).map(|row| {
        children(row).map(|cell| Span {
            cols: colspan(cell),
            rows: rowspan(cell),
        })
    });
    let (placements, max_cols) = grid::place(spans, row_count);
    let cells = rows(b)
        .flat_map(children)
        .zip(placements)
        .map(|(cell, at)| GridCell { cell, at })
        .collect();
    (cells, max_cols)
}

fn narrowest_first<'c, 'a>(cells: &'c [GridCell<'a>]) -> Vec<&'c GridCell<'a>> {
    let mut ordered: Vec<&GridCell<'a>> = cells.iter().filter(|c| c.at.cols > 0).collect();
    ordered.sort_by_key(|c| c.at.cols);
    ordered
}

fn widen_columns(cols: &mut [f64], range: Range<usize>, outer: f64, hsp: f64) {
    let range = range.start.min(cols.len())..range.end.min(cols.len());
    let span = range.len();
    if span == 0 {
        return;
    }
    let spanned = &mut cols[range];
    let base: f64 = spanned.iter().sum();
    let excess = outer - base - hsp * (span - 1) as f64;
    if excess <= 0.0 {
        return;
    }
    if base > 0.0 {
        for c in spanned.iter_mut() {
            *c += excess * *c / base;
        }
    } else {
        let per = excess / span as f64;
        for c in spanned.iter_mut() {
            *c += per;
        }
    }
}

fn cell_definite_width(s: *const Style, prop: PropId, h_extra: f64) -> f64 {
    let Some(v) = definite(value(s, prop)) else {
        return -1.0;
    };
    if value_is_percent(Some(v)) {
        return -1.0;
    }
    let w = length_resolve(Some(v), 0.0, -1.0);
    if w >= 0.0 { w + h_extra } else { -1.0 }
}

fn cell_clamp(cell: BoxRef<'_>, h_extra: f64, mut w: f64) -> f64 {
    let max_w = cell_definite_width(cell.style(), PropId::MaxWidth, h_extra);
    let min_w = cell_definite_width(cell.style(), PropId::MinWidth, h_extra);
    if max_w >= 0.0 && w > max_w {
        w = max_w;
    }
    if min_w >= 0.0 && w < min_w {
        w = min_w;
    }
    w
}

fn border_spacing(s: *const Style) -> (f64, f64) {
    let Some(s) = style_of(s) else {
        return (0.0, 0.0);
    };
    if keyword_is_ignore_case(s.get(PropId::BorderCollapse), b"collapse") {
        return (0.0, 0.0);
    }
    match s.get(PropId::BorderSpacing).and_then(ValueRef::size) {
        Some(size) => (size.w, size.h),
        None => (0.0, 0.0),
    }
}

fn is_collapse(s: *const Style) -> bool {
    keyword_is_ignore_case(value(s, PropId::BorderCollapse), b"collapse")
}

pub fn intrinsic_width(b: BoxRef<'_>, inherited: *const Style, min: bool) -> f64 {
    let mut captions = 0.0;
    for c in children(b).filter(|c| c.kind() == BoxKind::TableCaption) {
        let cs = styled_or(c, inherited);
        let mut w = if min {
            min_width_of(c, cs)
        } else {
            measure_natural_width(c, cs)
        };
        let (m, p, bd) = edges_from_style(box_style(c), 0.0);
        w += margin_padding_border_h(&m, &p, &bd);
        if w > captions {
            captions = w;
        }
    }
    let (cells, max_cols) = grid_cells(b);
    if max_cols == 0 {
        return captions;
    }
    let (hsp, _) = border_spacing(b.style());
    let mut cols = vec![0.0; max_cols];
    for &GridCell { cell, at } in narrowest_first(&cells) {
        {
            let cs = styled_or(cell, inherited);
            let (m, p, bd) = edges_from_style(box_style(cell), 0.0);
            let extra = margin_padding_border_h(&m, &p, &bd);
            let content_min = min_content_width_of(cell, cs);
            let mut w = if min {
                content_min
            } else {
                measure_natural_width(cell, cs)
            };
            if let Some(wv) = definite(value(cell.style(), PropId::Width))
                && !value_is_percent(Some(wv))
            {
                let e = length_resolve(Some(wv), 0.0, -1.0);
                if e >= 0.0 {
                    w = if e > content_min { e } else { content_min };
                }
            }
            widen_columns(
                &mut cols,
                at.columns(),
                cell_clamp(cell, extra, w + extra),
                hsp,
            );
        }
    }
    let mut col = 0;
    for hint in &b.table_col_hints() {
        if col >= max_cols {
            break;
        }
        let span = hint_span(hint.span());
        if let Some(wv) = value(hint.style(), PropId::Width)
            && let Some((_, unit)) = wv.length()
            && unit != UNIT_PERCENT
        {
            let w = length_resolve(Some(wv), 0.0, -1.0);
            if w >= 0.0 {
                widen_columns(&mut cols, col..col + span, w, hsp);
            }
        }
        col += span;
    }
    let mut sum = (max_cols + 1) as f64 * hsp;
    for w in &cols {
        sum += w;
    }
    if sum > captions { sum } else { captions }
}

fn width_from_style(s: *const Style, basis: f64) -> Option<f64> {
    let wv = definite(value(s, PropId::Width))?;
    let w = length_resolve(Some(wv), basis, -1.0);
    (w >= 0.0).then_some(w)
}

fn apply_fixed_width(widths: &mut [f64], fixed: &mut [bool], col: usize, span: usize, w: f64) {
    if col >= widths.len() {
        return;
    }
    let mut per = w / span as f64;
    if per < 0.0 {
        per = 0.0;
    }
    let end = (col + span).min(widths.len());
    for idx in col..end {
        if per > widths[idx] {
            widths[idx] = per;
        }
        fixed[idx] = true;
    }
}

fn fixed_columns(
    b: BoxRef<'_>,
    cells: &[GridCell<'_>],
    cw: f64,
    widths: &mut [f64],
    fixed: &mut [bool],
) -> f64 {
    let max_cols = widths.len();
    let mut col = 0;
    for hint in &b.table_col_hints() {
        if col >= max_cols {
            break;
        }
        if let Some(w) = width_from_style(hint.style(), cw) {
            apply_fixed_width(widths, fixed, col, hint_span(hint.span()), w);
        }
        col += hint_span(hint.span());
    }

    for &GridCell { cell, at } in cells.iter().take_while(|c| c.at.row == 0) {
        if let Some(mut w) = width_from_style(cell.style(), cw) {
            let (m, p, bd) = edges_from_style(box_style(cell), cw);
            w += margin_padding_border_h(&m, &p, &bd);
            apply_fixed_width(widths, fixed, at.col, at.cols, w);
        }
    }

    let mut used = 0.0;
    let mut unset = 0;
    for i in 0..max_cols {
        used += widths[i];
        if !fixed[i] {
            unset += 1;
        }
    }

    let mut remaining = cw - used;
    if remaining < 0.0 {
        remaining = 0.0;
    }
    if unset > 0 {
        let per = remaining / unset as f64;
        for i in 0..max_cols {
            if !fixed[i] {
                widths[i] = per;
            }
        }
    } else if remaining > 0.0 && max_cols > 0 {
        let per = remaining / max_cols as f64;
        for w in widths.iter_mut() {
            *w += per;
        }
    }

    let mut sum = 0.0;
    for w in widths.iter() {
        sum += w;
    }
    sum
}

fn caption_bottom(caption: BoxRef<'_>) -> bool {
    style_of(caption.style())
        .and_then(|s| s.keyword_of(PropId::CaptionSide))
        .is_some_and(|side| side == c"bottom" || side == c"block-end")
}

fn outer_height(b: BoxRef<'_>) -> f64 {
    plus_outer_v(b.content_height(), b)
}

fn layout_captions(
    b: BoxRef<'_>,
    bottom: bool,
    inner_x: f64,
    cw: f64,
    child_inherited: *const Style,
    cursor_y: &mut f64,
) {
    for caption in children(b) {
        if caption.kind() != BoxKind::TableCaption || caption_bottom(caption) != bottom {
            continue;
        }
        caption.set_x(inner_x);
        caption.set_y(*cursor_y);
        layout_box(caption, cw, child_inherited);
        *cursor_y += outer_height(caption);
    }
}

fn collapse_borders(cells: &[GridCell<'_>], max_cols: usize, row_count: usize) {
    if max_cols == 0 || row_count == 0 {
        return;
    }
    let Some(slots) = row_count.checked_mul(max_cols) else {
        return;
    };
    let mut taken = vec![false; slots];
    for GridCell { at, .. } in cells {
        for r in at.row..=at.last_row().min(row_count - 1) {
            for c in at.columns().take_while(|&c| c < max_cols) {
                taken[r * max_cols + c] = true;
            }
        }
    }
    for &GridCell { cell, at } in cells {
        let last_row = at.last_row().min(row_count - 1);
        let right = at.col + at.cols;
        let right_nb = right < max_cols && (at.row..=last_row).any(|r| taken[r * max_cols + right]);
        let below = last_row + 1;
        let below_nb = below < row_count
            && at
                .columns()
                .take_while(|&c| c < max_cols)
                .any(|c| taken[below * max_cols + c]);
        if right_nb || below_nb {
            let mut border = cell.border();
            if right_nb {
                border.right = 0.0;
            }
            if below_nb {
                border.bottom = 0.0;
            }
            cell.set_border(border);
        }
    }
}

fn vertical_align_factor(cell: BoxRef<'_>) -> f64 {
    let va = value(cell.style(), PropId::VerticalAlign);
    if keyword_is_ignore_case(va, b"middle") {
        0.5
    } else if keyword_is_ignore_case(va, b"bottom") {
        1.0
    } else {
        0.0
    }
}

fn is_border_box(s: *const Style) -> bool {
    keyword_is(value(s, PropId::BoxSizing), b"border-box")
}

fn content_box_width(
    b: BoxRef<'_>,
    parent_content_width: f64,
    explicit: Option<ValueRef<'_>>,
) -> f64 {
    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    let horiz_total = m.left + m.right + p.left + p.right + bd.left + bd.right;
    let sizing_extras = if is_border_box(b.style()) {
        p.left + p.right + bd.left + bd.right
    } else {
        0.0
    };
    let mut cw = match explicit {
        Some(wv) => length_resolve(Some(wv), parent_content_width, 0.0) - sizing_extras,
        None => parent_content_width - horiz_total,
    };
    if let Some(mxw) = definite(value(b.style(), PropId::MaxWidth)) {
        let m = length_resolve(Some(mxw), parent_content_width, -1.0);
        if m >= 0.0 && cw > m - sizing_extras {
            cw = m - sizing_extras;
        }
    }
    if let Some(mnw) = definite(value(b.style(), PropId::MinWidth)) {
        let m = length_resolve(Some(mnw), parent_content_width, -1.0);
        if m >= 0.0 && cw < m - sizing_extras {
            cw = m - sizing_extras;
        }
    }
    if cw < 0.0 { 0.0 } else { cw }
}

struct AutoColumns {
    widths: Vec<f64>,
    min: Vec<f64>,
    fixed: Vec<bool>,
    explicit: Vec<f64>,
}

#[allow(clippy::too_many_arguments)]
fn auto_columns(
    b: BoxRef<'_>,
    cells: &[GridCell<'_>],
    explicit_width: bool,
    measure_inherited: *const Style,
    cols: &mut AutoColumns,
    col_avail: &mut f64,
    cw: &mut f64,
    hsp: f64,
) {
    let max_cols = cols.widths.len();
    let total_hsp = (max_cols + 1) as f64 * hsp;
    let mut has_explicit_cols = false;
    let ordered = narrowest_first(cells);
    for &&GridCell { cell, at } in &ordered {
        {
            let span = at.cols;
            let cs = styled_or(cell, measure_inherited);
            let natural = measure_natural_width(cell, cs);
            edges_into_box(cell, if *cw > 0.0 { *cw } else { 1000.0 });
            let h_extra = padding_border_margin_h(&cell.margin(), &cell.padding(), &cell.border());
            let cell_outer = cell_clamp(cell, h_extra, natural + h_extra);
            let mut cell_fixed = false;
            let mut cell_explicit = -1.0;
            if let Some(cwv) = value(cell.style(), PropId::Width)
                && is_length_or_calc(cwv)
            {
                cell_fixed = true;
                let w = length_resolve(
                    Some(cwv),
                    if *col_avail > 0.0 { *col_avail } else { 0.0 },
                    -1.0,
                );
                if w >= 0.0 {
                    cell_explicit = cell_clamp(cell, h_extra, w + h_extra);
                }
            }
            widen_columns(&mut cols.widths, at.columns(), cell_outer, hsp);
            for i in at.columns().take_while(|&i| i < max_cols) {
                if cell_fixed && span == 1 {
                    cols.fixed[i] = true;
                    has_explicit_cols = true;
                }
                if cell_explicit >= 0.0 && span == 1 && cell_explicit > cols.explicit[i] {
                    cols.explicit[i] = cell_explicit;
                    has_explicit_cols = true;
                }
            }
        }
    }
    let mut col = 0;
    for hint in &b.table_col_hints() {
        if col >= max_cols {
            break;
        }
        let hspan = hint_span(hint.span());
        if let Some(w) = width_from_style(hint.style(), *col_avail) {
            has_explicit_cols = true;
            let per = w / hspan as f64;
            for e in cols.explicit.iter_mut().skip(col).take(hspan) {
                if per > *e {
                    *e = per;
                }
            }
        }
        col += hspan;
    }
    let mut natural_sum_pre = 0.0;
    for w in &cols.widths {
        natural_sum_pre += w;
    }
    if has_explicit_cols || natural_sum_pre > *col_avail {
        for &&GridCell { cell, at } in &ordered {
            let cs = styled_or(cell, measure_inherited);
            let (m, p, bd) =
                edges_from_style(box_style(cell), if *cw > 0.0 { *cw } else { 1000.0 });
            let h_extra = padding_border_margin_h(&m, &p, &bd);
            let cell_min = cell_clamp(cell, h_extra, min_content_width_of(cell, cs) + h_extra);
            widen_columns(&mut cols.min, at.columns(), cell_min, hsp);
        }
    }
    for i in 0..max_cols {
        if cols.explicit[i] >= 0.0 {
            let mut e = cols.explicit[i];
            if e < cols.min[i] {
                e = cols.min[i];
            }
            cols.widths[i] = e;
            cols.fixed[i] = true;
        }
    }
    let mut natural_sum = 0.0;
    for w in &cols.widths {
        natural_sum += w;
    }
    let widths = &mut cols.widths;
    if natural_sum > *col_avail {
        let mut min_sum = 0.0;
        for mn in &cols.min {
            min_sum += mn;
        }
        if min_sum >= *col_avail {
            if min_sum > 0.0 {
                widths.copy_from_slice(&cols.min);
                *col_avail = min_sum;
                *cw = *col_avail + total_hsp;
                b.set_content_width(*cw);
            } else {
                let evenly = *col_avail / max_cols as f64;
                widths.fill(evenly);
            }
        } else {
            let slack_sum = natural_sum - min_sum;
            let avail_extra = *col_avail - min_sum;
            for (w, mn) in widths.iter_mut().zip(&cols.min) {
                let slack = *w - *mn;
                *w = *mn
                    + if slack_sum > 0.0 {
                        slack * (avail_extra / slack_sum)
                    } else {
                        0.0
                    };
            }
        }
    } else if natural_sum == 0.0 {
        let evenly = *col_avail / max_cols as f64;
        widths.fill(evenly);
    } else if explicit_width {
        let extra = *col_avail - natural_sum;
        if extra > 0.0 && max_cols > 0 {
            let mut elastic_natural = 0.0;
            let mut elastic_count = 0;
            for (w, fixed) in widths.iter().zip(&cols.fixed) {
                if !fixed {
                    elastic_natural += w;
                    elastic_count += 1;
                }
            }
            if elastic_natural > 0.0 {
                for (w, fixed) in widths.iter_mut().zip(&cols.fixed) {
                    if !fixed {
                        *w += extra * *w / elastic_natural;
                    }
                }
            } else if elastic_count > 0 {
                let per = extra / elastic_count as f64;
                for (w, fixed) in widths.iter_mut().zip(&cols.fixed) {
                    if !fixed {
                        *w += per;
                    }
                }
            } else if natural_sum > 0.0 {
                for w in widths.iter_mut() {
                    *w += extra * *w / natural_sum;
                }
            } else {
                let per = extra / max_cols as f64;
                for w in widths.iter_mut() {
                    *w += per;
                }
            }
        }
    } else {
        *cw = natural_sum + total_hsp;
        b.set_content_width(*cw);
    }
}

fn center_with_auto_margins(b: BoxRef<'_>, cw: f64, parent_content_width: f64) {
    let ml_auto = keyword_is(value(b.style(), PropId::MarginLeft), b"auto");
    let mr_auto = keyword_is(value(b.style(), PropId::MarginRight), b"auto");
    if !ml_auto && !mr_auto {
        return;
    }
    let (p, bd) = (b.padding(), b.border());
    let outer = cw + p.left + p.right + bd.left + bd.right;
    let mut available = parent_content_width - outer;
    if available < 0.0 {
        available = 0.0;
    }
    let mut m = b.margin();
    if ml_auto && mr_auto {
        m.left = available / 2.0;
        m.right = available / 2.0;
    } else if ml_auto {
        m.left = available - m.right;
        if m.left < 0.0 {
            m.left = 0.0;
        }
    } else {
        m.right = available - m.left;
        if m.right < 0.0 {
            m.right = 0.0;
        }
    }
    b.set_margin(m);
}

fn flow_height(b: BoxRef<'_>) -> f64 {
    let mut dh = b.content_height();
    if matches!(b.kind(), BoxKind::Block | BoxKind::Table) {
        dh += outer_v(b);
    }
    dh
}

fn layout_cell_content(
    cell: BoxRef<'_>,
    cell_outer_w: f64,
    child_inherited: *const Style,
) -> (f64, f64) {
    let cs = styled_or(cell, child_inherited);
    edges_into_box(cell, cell_outer_w);
    let (m, p, bd) = (cell.margin(), cell.padding(), cell.border());
    let mut cell_inner_w = cell_outer_w - p.left - p.right - bd.left - bd.right - m.left - m.right;
    if cell_inner_w < 0.0 {
        cell_inner_w = 0.0;
    }
    cell.set_content_width(cell_inner_w);
    let ix = cell.x() + m.left + bd.left + p.left;
    let iy = cell.y() + m.top + bd.top + p.top;
    if cell.style().is_null() {
        layout_block(cell, cell_outer_w, cs);
        return (cell.content_width(), cell.content_height());
    }
    let mut sub_y = iy;
    for child in children(cell) {
        child.set_x(ix);
        child.set_y(sub_y);
        layout_box(child, cell_inner_w, cs);
        legacy_align_block_child(child, ix, cell_inner_w, cs);
        sub_y += flow_height(child);
    }
    (cell_inner_w, sub_y - iy)
}

fn cell_height(cell: BoxRef<'_>, cell_inner_w: f64, mut cell_h: f64) -> f64 {
    let s = cell.style();
    if let Some(hv) = definite(value(s, PropId::Height)) {
        let eh = resolve_used_height(cell, Some(hv), cell_inner_w, -1.0);
        if eh > cell_h {
            cell_h = eh;
        }
    }
    if let Some(mnh) = definite(value(s, PropId::MinHeight)) {
        let mh = resolve_used_height(cell, Some(mnh), cell_inner_w, -1.0);
        if mh > cell_h {
            cell_h = mh;
        }
    }
    cell_h
}

struct Columns<'c> {
    widths: &'c [f64],
    x: &'c [f64],
    hsp: f64,
}

impl Columns<'_> {
    fn outer_width(&self, at: Placement) -> f64 {
        let n = self.widths.len();
        let covered = &self.widths[at.col.min(n)..(at.col + at.cols).min(n)];
        let w: f64 = covered.iter().sum();
        if covered.len() > 1 {
            w + self.hsp * (covered.len() - 1) as f64
        } else {
            w
        }
    }

    fn x_of(&self, at: Placement) -> f64 {
        self.x.get(at.col).copied().unwrap_or(0.0)
    }
}

fn fill_cell_height(cell: BoxRef<'_>, avail: f64) {
    let natural: f64 = children(cell).map(flow_height).sum();
    let extra = avail - natural;
    if extra > 0.0 {
        let factor = vertical_align_factor(cell);
        if factor > 0.0 {
            for ch in children(cell) {
                shift_box_tree(ch, 0.0, extra * factor);
            }
        }
    }
    if avail > cell.content_height() {
        cell.set_content_height(avail);
    }
}

#[allow(clippy::too_many_arguments)]
fn layout_row<'a>(
    row: BoxRef<'a>,
    row_cells: &[GridCell<'a>],
    ending: &[BoxRef<'a>],
    inner_x: f64,
    cursor_y: f64,
    cw: f64,
    cols: &Columns<'_>,
    child_inherited: *const Style,
) -> f64 {
    row.set_x(inner_x);
    row.set_y(cursor_y);
    row.set_content_width(cw);
    let mut row_height: f64 = 0.0;
    for &GridCell { cell, at } in row_cells {
        cell.set_x(inner_x + cols.x_of(at));
        cell.set_y(cursor_y);
        let (cell_inner_w, cell_h) =
            layout_cell_content(cell, cols.outer_width(at), child_inherited);
        let cell_h = cell_height(cell, cell_inner_w, cell_h);
        cell.set_content_height(cell_h);
        let cell_outer_h = plus_outer_v(cell_h, cell);
        if at.rows <= 1 && cell_outer_h > row_height {
            row_height = cell_outer_h;
        }
    }
    if let Some(rhv) = definite(value(row.style(), PropId::Height)) {
        let rh = length_resolve(Some(rhv), 0.0, -1.0);
        if rh > row_height {
            row_height = rh;
        }
    }
    for &rc in ending {
        let avail = minus_outer_v(cursor_y + row_height - rc.y(), rc);
        if rc.content_height() > avail {
            row_height += rc.content_height() - avail;
        }
    }
    for &GridCell { cell, at } in row_cells {
        if at.rows <= 1 {
            fill_cell_height(cell, minus_outer_v(row_height, cell));
        }
    }
    row.set_content_height(row_height);
    row_height
}

fn stretch_to_height(b: BoxRef<'_>, cw: f64) {
    let Some(thv) = definite(value(b.style(), PropId::Height)) else {
        return;
    };
    let mut target = resolve_used_height(b, Some(thv), cw, -1.0);
    if is_border_box(b.style()) {
        let (p, bd) = (b.padding(), b.border());
        target -= p.top + p.bottom + bd.top + bd.bottom;
    }
    let nrows = rows(b).count();
    if nrows == 0 || target <= b.content_height() + 0.5 {
        return;
    }
    let per = (target - b.content_height()) / nrows as f64;
    let mut shift = 0.0;
    for row in rows(b) {
        if shift > 0.0 {
            translate_subtree(row, 0.0, shift);
        }
        row.set_content_height(row.content_height() + per);
        for cell in children(row).filter(|c| c.kind() == BoxKind::TableCell) {
            let factor = vertical_align_factor(cell);
            if factor > 0.0 {
                for ch in children(cell) {
                    shift_box_tree(ch, 0.0, per * factor);
                }
            }
            cell.set_content_height(cell.content_height() + per);
        }
        shift += per;
    }
    b.set_content_height(target);
}

pub fn layout(b: BoxRef<'_>, parent_content_width: f64, inherited: *const Style) {
    edges_into_box(b, parent_content_width);
    let explicit = definite(value(b.style(), PropId::Width));
    let explicit_width = explicit.is_some();
    let mut cw = content_box_width(b, parent_content_width, explicit);
    b.set_content_width(cw);
    let child_inherited = styled_or(b, inherited);

    let (cells, max_cols) = grid_cells(b);
    if max_cols == 0 {
        let (m, p, bd) = (b.margin(), b.padding(), b.border());
        let inner_x = b.x() + m.left + bd.left + p.left;
        let inner_y = b.y() + m.top + bd.top + p.top;
        let mut cursor_y = inner_y;
        layout_captions(b, false, inner_x, cw, child_inherited, &mut cursor_y);
        layout_captions(b, true, inner_x, cw, child_inherited, &mut cursor_y);
        b.set_content_height(cursor_y - inner_y);
        return;
    }

    let (hsp, vsp) = border_spacing(b.style());
    let total_hsp = (max_cols + 1) as f64 * hsp;
    let mut col_avail = cw - total_hsp;
    if col_avail < 0.0 {
        col_avail = 0.0;
    }

    let fixed_layout =
        explicit_width && keyword_is(value(b.style(), PropId::TableLayout), b"fixed");
    let mut cols = AutoColumns {
        widths: vec![0.0; max_cols],
        min: Vec::new(),
        fixed: vec![false; max_cols],
        explicit: Vec::new(),
    };
    if fixed_layout {
        let fixed_sum = fixed_columns(b, &cells, col_avail, &mut cols.widths, &mut cols.fixed);
        if fixed_sum > col_avail {
            col_avail = fixed_sum;
            cw = col_avail + total_hsp;
            b.set_content_width(cw);
        }
    } else {
        cols.min = vec![0.0; max_cols];
        cols.explicit = vec![-1.0; max_cols];
        auto_columns(
            b,
            &cells,
            explicit_width,
            child_inherited,
            &mut cols,
            &mut col_avail,
            &mut cw,
            hsp,
        );
    }
    let col_widths = cols.widths;
    let mut col_x = vec![0.0; max_cols];
    let mut cx = hsp;
    for (x, w) in col_x.iter_mut().zip(&col_widths) {
        *x = cx;
        cx += w + hsp;
    }

    center_with_auto_margins(b, cw, parent_content_width);

    let (m, p, bd) = (b.margin(), b.padding(), b.border());
    let inner_x = b.x() + m.left + bd.left + p.left;
    let inner_y = b.y() + m.top + bd.top + p.top;
    let mut cursor_y = inner_y;

    layout_captions(b, false, inner_x, cw, child_inherited, &mut cursor_y);
    cursor_y += vsp;

    let columns = Columns {
        widths: &col_widths,
        x: &col_x,
        hsp,
    };
    let mut row_count = 0;
    let mut next = 0;
    let mut open: Vec<GridCell<'_>> = Vec::new();
    for (r, row) in rows(b).enumerate() {
        row_count = r + 1;
        let start = next;
        while next < cells.len() && cells[next].at.row == r {
            next += 1;
        }
        let row_cells = &cells[start..next];
        let ending: Vec<BoxRef<'_>> = open
            .iter()
            .filter(|c| c.at.last_row() == r)
            .map(|c| c.cell)
            .collect();
        let row_height = layout_row(
            row,
            row_cells,
            &ending,
            inner_x,
            cursor_y,
            cw,
            &columns,
            child_inherited,
        );
        cursor_y += row_height + vsp;
        open.extend(row_cells.iter().filter(|c| c.at.rows > 1).copied());
        open.retain(|c| {
            if c.at.last_row() > r {
                return true;
            }
            fill_cell_height(c.cell, minus_outer_v(cursor_y - vsp - c.cell.y(), c.cell));
            false
        });
    }
    layout_captions(b, true, inner_x, cw, child_inherited, &mut cursor_y);
    if is_collapse(b.style()) {
        collapse_borders(&cells, max_cols, row_count);
    }
    b.set_content_height(cursor_y - inner_y);
    stretch_to_height(b, cw);
}
