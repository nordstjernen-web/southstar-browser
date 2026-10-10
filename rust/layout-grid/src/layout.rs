//! Southstar — laying out a grid container: placing its items, sizing columns and rows, aligning the items in their areas and handing subgrids their parent's tracks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::CStr;

use southstar_css::{GridTracks, TRACK_AUTO, TRACK_FR, TRACK_PERCENT, TRACK_PX};
use southstar_layout::{BoxKind, BoxRef, Style, children};
use southstar_style::{PropId, StyleRef, ValueRef, display_of};

use crate::abs::{keyword_or, self_alignment, track_edge};
use crate::ffi::{self, style_of};
use crate::intrinsic::{gap_px, horizontal_extras, is_absolute_or_fixed, is_length_or_calc};
use crate::lines::{COLUMN_PROPS, Lines, ROW_PROPS, resolve_pos};
use crate::place::{Area, Occupancy, Slot};
use crate::text::{ROWS_MAX, fmax, fmin};
use crate::tracks::{
    self, ColumnContent, Contribution, MAX, RowSpan, RowTracks, distribute_span,
    expand_auto_repeat, expand_flexible_rows, extend_with_auto_tracks, is_intrinsic, resolve_sizes,
    single_auto, span_accommodate, track_is_fixed, track_min_px, track_px,
};

#[derive(Clone, Copy)]
struct SubgridCols {
    n: i32,
    x: [f64; MAX + 1],
    sizes: [f64; MAX],
    gap: f64,
}

#[derive(Clone, Copy)]
struct SubgridRows {
    n: i32,
    y: [f64; MAX + 1],
    sizes: [f64; MAX],
    gap: f64,
}

thread_local! {
    static PENDING_COLS: Cell<Option<SubgridCols>> = const { Cell::new(None) };
    static PENDING_ROWS: Cell<Option<SubgridRows>> = const { Cell::new(None) };
}

struct Item<'a> {
    b: BoxRef<'a>,
    col_start: i32,
    col_span: i32,
    row_start: i32,
    row_span: i32,
    col: i32,
    row: i32,
    outer_h: f64,
}

struct Row {
    top: f64,
    height: f64,
}

fn template_tracks(v: Option<ValueRef<'_>>) -> Option<&GridTracks> {
    v.and_then(ValueRef::tracks)
}

fn pattern_tracks(v: Option<ValueRef<'_>>) -> Option<&GridTracks> {
    template_tracks(v).filter(|t| t.n > 0 && t.subgrid == 0)
}

fn keyword_is(v: Option<ValueRef<'_>>, kw: &CStr) -> bool {
    v.is_some_and(|v| v.is_keyword(kw))
}

fn style_has(s: Option<StyleRef<'_>>, prop: PropId, kw: &CStr) -> bool {
    keyword_is(s.and_then(|s| s.get(prop)), kw)
}

fn box_inset_definite_height(b: BoxRef<'_>) -> f64 {
    let Some(s) = style_of(b) else {
        return -1.0;
    };
    if !is_absolute_or_fixed(Some(s)) || b.content_height() <= 0.0 {
        return -1.0;
    }
    let (top, bottom) = (s.get(PropId::Top), s.get(PropId::Bottom));
    if top.is_none() || keyword_is(top, c"auto") || bottom.is_none() || keyword_is(bottom, c"auto")
    {
        return -1.0;
    }
    b.content_height()
}

fn row_basis(b: BoxRef<'_>, s: StyleRef<'_>, cw: f64) -> f64 {
    let hv = s.get(PropId::Height);
    if is_length_or_calc(hv) {
        return ffi::clamp_height(Some(s), ffi::used_height(b, hv, cw, -1.0));
    }
    box_inset_definite_height(b)
}

fn auto_repeat_height(b: BoxRef<'_>, s: StyleRef<'_>, basis: f64, cw: f64) -> f64 {
    if basis > 0.0 {
        return basis;
    }
    for prop in [PropId::MaxHeight, PropId::MinHeight] {
        let v = s.get(prop);
        if is_length_or_calc(v) {
            let h = ffi::height_to_content(b, ffi::used_height(b, v, cw, -1.0));
            if h > 0.0 {
                return h;
            }
        }
    }
    0.0
}

fn min_block_contribution(c: BoxRef<'_>, item_outer: f64, cb_height: f64) -> f64 {
    let Some(s) = style_of(c).filter(|&s| ffi::establishes_bfc(s)) else {
        return item_outer;
    };
    let mnh = s.get(PropId::MinHeight);
    let mut min_h = if is_length_or_calc(mnh) {
        ffi::resolve_length(mnh, if cb_height > 0.0 { cb_height } else { 0.0 }, 0.0)
    } else {
        0.0
    };
    if min_h < 0.0 {
        min_h = 0.0;
    }
    let (m, p, b) = (c.margin(), c.padding(), c.border());
    let extras = p.top + p.bottom + b.top + b.bottom + m.top + m.bottom;
    fmin(item_outer, min_h + extras)
}

fn stretched_item_max_height(c: BoxRef<'_>) -> f64 {
    let mx = style_of(c).and_then(|s| s.get(PropId::MaxHeight));
    if !is_length_or_calc(mx) || ffi::is_percent(mx) {
        return -1.0;
    }
    let h = ffi::resolve_length(mx, 0.0, -1.0);
    if h < 0.0 {
        -1.0
    } else {
        ffi::height_to_content(c, h)
    }
}

fn outer_height(c: BoxRef<'_>) -> f64 {
    let (m, p, b) = (c.margin(), c.padding(), c.border());
    c.content_height() + p.top + p.bottom + b.top + b.bottom + m.top + m.bottom
}

fn columns_are_subgrid(s: Option<StyleRef<'_>>) -> bool {
    template_tracks(s.and_then(|s| s.get(PropId::GridTemplateColumns)))
        .is_some_and(|t| t.subgrid != 0)
}

fn rows_are_subgrid(s: Option<StyleRef<'_>>) -> bool {
    template_tracks(s.and_then(|s| s.get(PropId::GridTemplateRows))).is_some_and(|t| t.subgrid != 0)
}

fn has_word(kw: Option<&CStr>, word: &[u8]) -> bool {
    kw.is_some_and(|k| k.to_bytes().windows(word.len()).any(|w| w == word))
}

fn place_by_column(
    items: &mut [Item<'_>],
    cols_buf: &mut GridTracks,
    flow_rows: i32,
    tmpl_cols: i32,
    dense: bool,
    auto_cols: Option<&GridTracks>,
) -> i32 {
    let max = MAX as i32;
    let flow_rows = flow_rows.clamp(1, max);
    let mut occ = [[false; MAX]; MAX];
    let (mut cur_col, mut cur_row) = (0, 0);
    let mut used_cols = tmpl_cols;
    for it in items.iter_mut() {
        let s = it.col_start;
        let sp = it.col_span.clamp(1, max);
        let rs_start = it.row_start;
        let rs = it.row_span.max(1).min(flow_rows);
        let fixed_col = s >= 0 && s + sp <= max;
        let fixed_row = rs_start >= 0 && rs_start + rs <= flow_rows;
        let (mut pc, mut pr) = (-1, -1);
        let c0 = if fixed_col {
            s
        } else if dense {
            0
        } else {
            cur_col
        };
        let mut c = c0;
        while c + sp <= max && pc < 0 {
            if fixed_col && c != s {
                break;
            }
            let r0 = if fixed_row {
                rs_start
            } else if !fixed_col && !dense && c == c0 {
                cur_row
            } else {
                0
            };
            let mut r = r0;
            while r + rs <= flow_rows {
                if fixed_row && r != rs_start {
                    break;
                }
                let free =
                    (r..r + rs).all(|rr| (c..c + sp).all(|cc| !occ[rr as usize][cc as usize]));
                if free {
                    pc = c;
                    pr = r;
                    break;
                }
                r += 1;
            }
            c += 1;
        }
        if pc < 0 {
            pc = max - sp;
            pr = 0;
        }
        for row in &mut occ[pr as usize..(pr + rs) as usize] {
            for cell in &mut row[pc as usize..(pc + sp) as usize] {
                *cell = true;
            }
        }
        it.col = pc;
        it.row = pr;
        if pc + sp > used_cols {
            used_cols = pc + sp;
        }
        if !fixed_col || !fixed_row {
            cur_col = pc;
            cur_row = pr + rs;
            if cur_row >= flow_rows {
                cur_col = pc + sp;
                cur_row = 0;
            }
        }
    }
    let used_cols = used_cols.clamp(1, max);
    let mut flow_cols = GridTracks {
        n: used_cols,
        ..GridTracks::default()
    };
    for c in 0..used_cols {
        flow_cols.tracks[c as usize] = if c < tmpl_cols {
            cols_buf.tracks[c as usize]
        } else if let Some(a) = auto_cols {
            a.tracks[((c - tmpl_cols) % a.n) as usize]
        } else {
            tracks::auto_track()
        };
    }
    *cols_buf = flow_cols;
    used_cols
}

fn place_by_row(items: &mut [Item<'_>], n_cols: i32, dense: bool, n_rows: &mut i32) {
    let len = items.len();
    let mut occupied = Occupancy::default();
    let mut cursor = Slot { row: 0, col: 0 };
    for step in 0..3 * len {
        let i = step % len;
        let phase = step / len;
        let it = &items[i];
        let s = it.col_start;
        let sp = it.col_span.max(1).min(n_cols);
        let rs_start = it.row_start;
        let rs = it.row_span.clamp(1, ROWS_MAX);
        let fixed_col = s >= 0 && s + sp <= n_cols;
        let fixed_row = (0..ROWS_MAX).contains(&rs_start);
        let item_phase = if fixed_row {
            if fixed_col { 0 } else { 1 }
        } else {
            2
        };
        if phase != item_phase {
            continue;
        }
        let start = Slot {
            row: if fixed_row {
                rs_start
            } else if dense {
                0
            } else {
                cursor.row
            },
            col: if fixed_col {
                s
            } else if fixed_row || dense {
                0
            } else {
                cursor.col
            },
        };
        let area = Area {
            col_span: sp,
            row_span: rs,
            n_cols,
        };
        let slot = match occupied.find_slot(&area, &start, fixed_row, fixed_col) {
            Some(slot) => slot,
            None => {
                let mut row = start.row.max(0);
                let mut col = if fixed_col { s } else { 0 }.max(0);
                if col + sp > n_cols {
                    col = n_cols - sp;
                }
                if row + rs > ROWS_MAX {
                    row = ROWS_MAX - rs;
                }
                Slot {
                    row: row.max(0),
                    col,
                }
            }
        };
        occupied.mark(slot.row, slot.col, &area);
        items[i].row = slot.row;
        items[i].col = slot.col;
        if slot.row + rs > *n_rows {
            *n_rows = slot.row + rs;
        }
        if phase < 2 {
            continue;
        }
        cursor = Slot {
            row: slot.row,
            col: slot.col + sp,
        };
        occupied.advance_cursor(&mut cursor, n_cols);
    }
}

fn mark_used(items: &[Item<'_>], columns: bool, limit: i32) -> [bool; MAX] {
    let mut used = [false; MAX];
    for it in items {
        let (start, span) = if columns {
            (it.col, it.col_span)
        } else {
            (it.row, it.row_span)
        };
        let mut j = 0;
        while j < span && start + j < limit {
            if start + j >= 0 {
                used[(start + j) as usize] = true;
            }
            j += 1;
        }
    }
    used
}

fn measure_contribution(c: BoxRef<'_>, inherited: *const Style, avail: f64) -> Contribution {
    let mut nw = ffi::natural_width(c, inherited);
    let mut mw = ffi::min_width(c, inherited);
    if let Some(s) = style_of(c) {
        let (m, p, b) = ffi::edges(s.as_ptr(), mw);
        let extra = horizontal_extras(&m, &p, &b);
        mw += extra;
        nw += extra;
    }
    if nw > avail {
        nw = avail;
    }
    Contribution { min: mw, max: nw }
}

fn justify_shift(j_kw: &[u8], free_w: f64, grid_rtl: bool, item_far: bool) -> f64 {
    let mut dx = 0.0;
    let at_right = match j_kw {
        b"center" => {
            if free_w > 0.0 {
                dx = free_w / 2.0;
            }
            false
        }
        b"end" | b"flex-end" => !grid_rtl,
        b"start" | b"flex-start" => grid_rtl,
        b"self-end" => !item_far,
        b"self-start" => item_far,
        b"right" => true,
        _ => false,
    };
    if at_right && free_w != 0.0 {
        dx = free_w;
    }
    dx
}

fn content_distribution(kw: &[u8], free: f64, n: usize) -> (f64, f64) {
    match kw {
        b"space-between" if n > 1 => (0.0, free / (n - 1) as f64),
        b"space-around" if n > 0 => {
            let gap = free / n as f64;
            (gap / 2.0, gap)
        }
        b"space-evenly" if n > 0 => {
            let gap = free / (n + 1) as f64;
            (gap, gap)
        }
        _ => (0.0, 0.0),
    }
}

pub(crate) fn layout_grid(
    b: BoxRef<'_>,
    cw: f64,
    inner_x: f64,
    inner_y: f64,
    child_inherited: *const Style,
) -> f64 {
    let sg = PENDING_COLS.take();
    let sgr = PENDING_ROWS.take();
    let Some(style) = style_of(b) else {
        return inner_y;
    };

    let areas = style
        .get(PropId::GridTemplateAreas)
        .and_then(ValueRef::areas);
    let cols_v = template_tracks(style.get(PropId::GridTemplateColumns));
    let cols_sg = sg.filter(|s| cols_v.is_some_and(|t| t.subgrid != 0) && s.n > 0);
    let cols_subgrid = cols_sg.is_some();
    let rows_v = template_tracks(style.get(PropId::GridTemplateRows));
    let rows_sg = sgr.filter(|s| rows_v.is_some_and(|t| t.subgrid != 0) && s.n > 0);
    let default_cols = single_auto();
    let cols_template = cols_v.filter(|t| t.subgrid == 0);
    let cols_src = cols_template.unwrap_or(&default_cols);
    let auto_cols_v = style.get(PropId::GridAutoColumns);

    let mut col_gap = gap_px(style.get(PropId::ColumnGap), style.get(PropId::Gap), cw);
    let basis = row_basis(b, style, cw);
    let mut row_gap = gap_px(
        style.get(PropId::RowGap),
        style.get(PropId::Gap),
        if basis > 0.0 { basis } else { 0.0 },
    );
    if let Some(s) = &rows_sg {
        row_gap = s.gap;
    }

    let expanded = expand_auto_repeat(cols_src, cw, col_gap);
    let (fit_start, fit_count) = (expanded.fit_start, expanded.fit_count);
    let mut cols_buf = expanded.tracks;
    if let Some(a) = areas
        && !cols_subgrid
    {
        let templated = if cols_template.is_none() {
            0
        } else {
            cols_buf.n
        };
        if a.n_cols > templated {
            extend_with_auto_tracks(
                &mut cols_buf,
                templated,
                a.n_cols,
                template_tracks(auto_cols_v),
            );
        }
    }
    let mut n_cols = if cols_buf.n > 0 { cols_buf.n } else { 1 };
    let explicit_cols = n_cols;
    let mut rows_buf = GridTracks::default();
    let mut has_rows_template = false;
    let (mut row_fit_start, mut row_fit_count) = (0, 0);
    if rows_sg.is_none()
        && let Some(rt) = rows_v.filter(|t| t.subgrid == 0)
    {
        let e = expand_auto_repeat(rt, auto_repeat_height(b, style, basis, cw), row_gap);
        rows_buf = e.tracks;
        row_fit_start = e.fit_start;
        row_fit_count = e.fit_count;
        has_rows_template = true;
    }
    let mut row_line_tracks = match (&rows_sg, has_rows_template) {
        (Some(s), _) => s.n,
        (None, true) => rows_buf.n,
        (None, false) => 1,
    };
    if let Some(a) = areas
        && rows_sg.is_none()
        && a.n_rows > row_line_tracks
    {
        row_line_tracks = a.n_rows;
    }
    if row_line_tracks < 1 {
        row_line_tracks = 1;
    }

    let mut col_sizes = [0.0f64; MAX];

    let mut items: Vec<Item<'_>> = Vec::new();
    for c in children(b) {
        let (mut s, mut sp, mut rs_start, mut rs) = (-1, 1, -1, 1);
        if let Some(st) = style_of(c) {
            let col_lines = Lines {
                tracks: Some(&cols_buf),
                areas,
                row_axis: false,
            };
            let row_lines = Lines {
                tracks: has_rows_template.then_some(&rows_buf),
                areas,
                row_axis: true,
            };
            if !resolve_pos(&col_lines, st, &COLUMN_PROPS, n_cols, &mut s, &mut sp) {
                s = -1;
            }
            if sp > MAX as i32 {
                sp = MAX as i32;
            }
            if !resolve_pos(
                &row_lines,
                st,
                &ROW_PROPS,
                row_line_tracks,
                &mut rs_start,
                &mut rs,
            ) {
                rs_start = -1;
            }
            if rs < 1 {
                rs = 1;
            }
        }
        items.push(Item {
            b: c,
            col_start: s,
            col_span: sp,
            row_start: rs_start,
            row_span: rs,
            col: 0,
            row: 0,
            outer_h: 0.0,
        });
    }

    if !cols_subgrid {
        let mut max_end = 0;
        for it in &items {
            let e = it.col_start.max(0) + it.col_span.max(1);
            if e > max_end {
                max_end = e;
            }
        }
        if max_end > MAX as i32 {
            max_end = MAX as i32;
        }
        if max_end > n_cols {
            extend_with_auto_tracks(&mut cols_buf, n_cols, max_end, template_tracks(auto_cols_v));
            n_cols = max_end;
        }
    }

    let auto_flow = style.keyword_of(PropId::GridAutoFlow);
    let dense = has_word(auto_flow, b"dense");
    let col_flow = has_word(auto_flow, b"column") && !cols_subgrid && rows_sg.is_none();
    if col_flow {
        let flow_rows = if has_rows_template { rows_buf.n } else { 0 };
        let tmpl_cols = if cols_template.is_some() {
            cols_buf.n
        } else {
            0
        };
        n_cols = place_by_column(
            &mut items,
            &mut cols_buf,
            flow_rows,
            tmpl_cols,
            dense,
            pattern_tracks(auto_cols_v),
        );
    }

    let auto_rows_tracks = if rows_sg.is_none() {
        pattern_tracks(style.get(PropId::GridAutoRows))
    } else {
        None
    };
    let explicit_rows = match (&rows_sg, has_rows_template) {
        (Some(s), _) => s.n,
        (None, true) => rows_buf.n,
        (None, false) => 0,
    };
    let mut n_rows = explicit_rows;
    if let Some(a) = areas
        && rows_sg.is_none()
        && a.n_rows > n_rows
    {
        n_rows = a.n_rows.min(ROWS_MAX);
    }
    if col_flow {
        for it in &items {
            let rs = it.row_span.max(1);
            if it.row + rs > n_rows {
                n_rows = it.row + rs;
            }
        }
        if n_rows < 1 {
            n_rows = 1;
        }
    } else if !items.is_empty() {
        place_by_row(&mut items, n_cols, dense, &mut n_rows);
    }
    if n_rows > ROWS_MAX {
        n_rows = ROWS_MAX;
    }
    if let Some(s) = &rows_sg
        && n_rows > s.n
    {
        n_rows = s.n;
    }

    if row_fit_count > 0 && has_rows_template {
        let used = mark_used(&items, false, rows_buf.n);
        let end = (row_fit_start + row_fit_count).min(rows_buf.n);
        for t in row_fit_start..end {
            if used[t as usize] {
                continue;
            }
            let tk = &mut rows_buf.tracks[t as usize];
            tk.kind = TRACK_PX;
            tk.v = 0.0;
            tk.pct = 0.0;
            tk.has_min = 0;
        }
    }
    let mut col_collapsed = [false; MAX];
    let mut col_gap_after = [0.0f64; MAX + 1];
    if fit_count > 0 && !cols_subgrid {
        let used = mark_used(&items, true, n_cols);
        let end = (fit_start + fit_count).min(n_cols);
        for t in fit_start..end {
            if used[t as usize] {
                continue;
            }
            col_collapsed[t as usize] = true;
            let tk = &mut cols_buf.tracks[t as usize];
            tk.kind = TRACK_PX;
            tk.v = 0.0;
            tk.has_min = 0;
        }
    }
    let avail = {
        let n = n_cols as usize;
        let mut gaps_total = 0.0;
        for t in 0..n {
            let later = col_collapsed[t + 1..n].iter().any(|&c| !c);
            col_gap_after[t] = if !col_collapsed[t] && later {
                col_gap
            } else {
                0.0
            };
            gaps_total += col_gap_after[t];
        }
        let avail = cw - gaps_total;
        if avail < 0.0 { 0.0 } else { avail }
    };

    let rows_tracks = has_rows_template.then_some(&rows_buf);
    let cols = &cols_buf;
    let mut content = ColumnContent {
        min: [0.0; MAX],
        max: [0.0; MAX],
    };
    let mut any_auto_content = false;
    for t in 0..n_cols {
        let tk = &cols.tracks[t as usize];
        if !is_intrinsic(tk.kind) && !(tk.has_min != 0 && is_intrinsic(tk.min_kind)) {
            continue;
        }
        for it in &items {
            if it.col != t || it.col_span != 1 {
                continue;
            }
            let c = measure_contribution(it.b, child_inherited, avail);
            let t = t as usize;
            if c.max > content.max[t] {
                content.max[t] = c.max;
            }
            if c.min > content.min[t] {
                content.min[t] = c.min;
            }
            any_auto_content = true;
        }
    }
    for span in 2..=n_cols {
        for it in &items {
            let c0 = it.col;
            if c0 < 0 || it.col_span != span || c0 + span > n_cols {
                continue;
            }
            let contribution = measure_contribution(it.b, child_inherited, avail);
            if span_accommodate(
                cols,
                c0 as usize,
                span as usize,
                &col_gap_after,
                avail,
                contribution,
                &mut content,
            ) {
                any_auto_content = true;
            }
        }
    }
    let justify_content = keyword_or(Some(style), PropId::JustifyContent, c"normal").to_bytes();
    resolve_sizes(
        cols,
        avail,
        any_auto_content.then_some((&content.min, &content.max)),
        &mut col_sizes,
        justify_content == b"normal" || justify_content == b"stretch",
    );

    let mut col_x = [0.0f64; MAX + 1];
    col_x[0] = inner_x;
    for i in 0..n_cols as usize {
        col_x[i + 1] = col_x[i] + col_sizes[i] + col_gap_after[i];
    }

    if let Some(s) = &cols_sg {
        n_cols = s.n;
        col_gap = s.gap;
        let n = n_cols as usize;
        col_sizes[..n].copy_from_slice(&s.sizes[..n]);
        col_x[..=n].copy_from_slice(&s.x[..=n]);
    } else {
        let n = n_cols as usize;
        let mut used_w = 0.0;
        for &size in &col_sizes[..n] {
            used_w += size;
        }
        for &gap in &col_gap_after[..n] {
            used_w += gap;
        }
        let free_w = cw - used_w;
        if free_w > 0.5 {
            let jc = keyword_or(Some(style), PropId::JustifyContent, c"start").to_bytes();
            let (off, extra_gap) = match jc {
                b"center" => (free_w / 2.0, 0.0),
                b"end" | b"flex-end" | b"right" => (free_w, 0.0),
                _ => content_distribution(jc, free_w, n),
            };
            if off != 0.0 || extra_gap != 0.0 {
                col_x[0] = inner_x + off;
                for t in 0..n {
                    col_x[t + 1] = col_x[t] + col_sizes[t] + col_gap_after[t] + extra_gap;
                }
            }
        }
    }
    let grid_rtl =
        !cols_subgrid && keyword_or(Some(style), PropId::Direction, c"ltr").to_bytes() == b"rtl";
    if grid_rtl {
        let right = inner_x + cw;
        for t in 0..n_cols as usize {
            col_x[t] = right - (col_x[t] - inner_x) - col_sizes[t];
        }
    }

    let row_tracks = RowTracks {
        template: rows_tracks,
        auto: auto_rows_tracks,
        explicit: explicit_rows,
    };
    let nr = n_rows.max(0) as usize;
    let mut base_row_height = vec![0.0f64; nr + 1];
    for (r, h) in base_row_height[..nr].iter_mut().enumerate() {
        *h = match &rows_sg {
            Some(s) => s.sizes[r],
            None => track_px(row_tracks.track(r as i32), basis),
        };
    }
    let mut base_row_y = vec![0.0f64; nr + 1];
    base_row_y[0] = rows_sg.as_ref().map_or(inner_y, |s| s.y[0]);
    let base_gap = rows_sg.as_ref().map_or(row_gap, |s| s.gap);
    for r in 0..nr {
        base_row_y[r + 1] = base_row_y[r] + base_row_height[r] + base_gap;
    }

    let justify_items = keyword_or(Some(style), PropId::JustifyItems, c"stretch");
    for it in items.iter_mut() {
        let c = it.b;
        let cs = style_of(c);
        let sp = it.col_span.max(1).min(n_cols);
        let mut rs = it.row_span.max(1);
        let placed_row = it.row.max(0);
        if placed_row + rs > n_rows {
            rs = n_rows - placed_row;
        }
        let mut chosen = it.col.max(0);
        if chosen + sp > n_cols {
            chosen = n_cols - sp;
        }
        let ch = chosen as usize;

        let mut w = 0.0;
        for k in 0..sp as usize {
            w += col_sizes[ch + k]
                + if k > 0 {
                    col_gap_after[ch + k - 1]
                } else {
                    0.0
                };
        }
        ffi::edges_into(c, w);
        let (m, p, bd) = (c.margin(), c.padding(), c.border());
        let mut cw_for_item = w - m.left - m.right;
        if cw_for_item < 0.0 {
            cw_for_item = 0.0;
        }
        c.set_x(if grid_rtl {
            col_x[ch + sp as usize - 1]
        } else {
            col_x[ch]
        });
        c.set_y(inner_y);

        let j_eff = match cs.and_then(|s| s.keyword_of(PropId::JustifySelf)) {
            Some(kw) if kw != c"auto" => kw,
            _ => justify_items,
        }
        .to_bytes();
        let j_stretch = matches!(j_eff, b"stretch" | b"normal" | b"legacy");
        let i_has_w = is_length_or_calc(cs.and_then(|s| s.get(PropId::Width)));
        let mut item_w = cw_for_item;
        if !j_stretch && !i_has_w {
            let nat = ffi::natural_width(c, child_inherited);
            if nat >= 0.0 && nat < item_w {
                item_w = nat;
            }
            if item_w < 0.0 {
                item_w = 0.0;
            }
        }
        let child_is_grid = display_of(cs).is_grid_container();
        if sp >= 1 && child_is_grid && columns_are_subgrid(cs) {
            let mut sub = SubgridCols {
                n: sp,
                x: [0.0; MAX + 1],
                sizes: [0.0; MAX],
                gap: col_gap,
            };
            let n = sp as usize;
            sub.x[..=n].copy_from_slice(&col_x[ch..=ch + n]);
            sub.sizes[..n].copy_from_slice(&col_sizes[ch..ch + n]);
            PENDING_COLS.set(Some(sub));
        }
        let sub_rs = rs.min(MAX as i32);
        let pr = placed_row as usize;
        if sub_rs >= 1 && placed_row + sub_rs <= n_rows && child_is_grid && rows_are_subgrid(cs) {
            let n = sub_rs as usize;
            if base_row_height[pr..pr + n].iter().all(|&h| h > 0.0) {
                let child_inner_y = c.y() + m.top + bd.top + p.top;
                let parent_row_y = base_row_y[pr];
                let mut sub = SubgridRows {
                    n: sub_rs,
                    y: [0.0; MAX + 1],
                    sizes: [0.0; MAX],
                    gap: base_gap,
                };
                for k in 0..=n {
                    sub.y[k] = child_inner_y + (base_row_y[pr + k] - parent_row_y);
                }
                sub.sizes[..n].copy_from_slice(&base_row_height[pr..pr + n]);
                PENDING_ROWS.set(Some(sub));
            }
        }
        let mut area_h = 0.0;
        let mut k = 0;
        while k < rs && placed_row + k < n_rows {
            let h = base_row_height[pr + k as usize];
            if h <= 0.0 {
                area_h = 0.0;
                break;
            }
            area_h += h + if k > 0 { base_gap } else { 0.0 };
            k += 1;
        }
        c.set_cb_height_override(area_h);
        ffi::layout_child(c, item_w + m.left + m.right, child_inherited);
        PENDING_COLS.set(None);
        PENDING_ROWS.set(None);
        let auto_h_margin = style_has(cs, PropId::MarginLeft, c"auto")
            || style_has(cs, PropId::MarginRight, c"auto");
        if (!j_stretch || i_has_w) && !auto_h_margin {
            let (p, bd) = (c.padding(), c.border());
            let used_w = c.content_width() + p.left + p.right + bd.left + bd.right;
            let free_w = cw_for_item - used_w;
            let j_safe = cs
                .and_then(|s| s.get(PropId::JustifySelf))
                .and_then(ValueRef::keyword_text)
                .is_some_and(|kw| kw.to_bytes().starts_with(b"safe "));
            let j_kw: &[u8] = if j_stretch || (j_safe && free_w < 0.0) {
                b"start"
            } else {
                j_eff
            };
            let item_far = ffi::start_is_far_side(cs, true);
            let dx = justify_shift(j_kw, free_w, grid_rtl, item_far);
            if dx != 0.0 {
                ffi::shift(c, dx, 0.0);
            }
        }
        it.outer_h = outer_height(c);
    }

    let definite_rows = rows_sg.is_none() && basis > 0.0;
    let mut row_height = vec![0.0f64; nr + 1];
    let mut row_fixed = vec![false; nr + 1];
    let mut row_flex = vec![false; nr + 1];
    let mut row_fr = vec![0.0f64; nr + 1];
    let mut row_flex_factor = vec![0.0f64; nr + 1];
    let mut row_limit = vec![-1.0f64; nr + 1];
    let mut row_min_intrinsic = vec![false; nr + 1];
    let mut row_max_intrinsic = vec![false; nr + 1];
    let pos_basis = if basis > 0.0 { basis } else { 0.0 };
    for r in 0..nr {
        row_flex_factor[r] = -1.0;
        let mut fixed = 0.0;
        let tk = match &rows_sg {
            Some(s) => {
                fixed = s.sizes[r];
                None
            }
            None => row_tracks.track(r as i32),
        };
        row_min_intrinsic[r] = true;
        row_max_intrinsic[r] = true;
        if let Some(tk) = tk {
            let flex = tk.kind == TRACK_FR;
            let fit = tk.fit_content != 0;
            let has_min = tk.has_min != 0;
            fixed = if flex {
                track_min_px(tk, pos_basis)
            } else if fit {
                0.0
            } else {
                track_px(Some(tk), basis)
            };
            if !flex && !fit && has_min && !is_intrinsic(tk.min_kind) {
                row_min_intrinsic[r] = false;
                fixed = fmax(fixed, track_min_px(tk, pos_basis));
            }
            if !flex && !fit && !is_intrinsic(tk.kind) && has_min && is_intrinsic(tk.min_kind) {
                row_max_intrinsic[r] = false;
                row_limit[r] = track_px(Some(tk), basis);
            }
            row_fixed[r] = track_is_fixed(tk, basis)
                || (definite_rows && flex && has_min && !is_intrinsic(tk.min_kind));
            row_flex[r] = definite_rows && flex;
            if flex {
                row_fr[r] = if tk.v > 0.0 { tk.v } else { 1.0 };
                row_flex_factor[r] = if tk.v > 0.0 { tk.v } else { 0.0 };
            }
        }
        if fixed > row_height[r] {
            row_height[r] = fixed;
        }
    }
    let mut by_span: Vec<usize> = (0..items.len()).collect();
    by_span.sort_by_key(|&i| (items[i].row_span.max(1), i));
    let mut target = Vec::new();
    for &i in &by_span {
        let it = &items[i];
        let row = it.row;
        if row < 0 || row >= n_rows {
            continue;
        }
        let rs = it.row_span.max(1).min(n_rows - row) as usize;
        let row = row as usize;
        let mut item_outer = it.outer_h;
        if rs == 1 && !definite_rows && row_flex_factor[row] >= 0.0 && row_flex_factor[row] < 1.0 {
            item_outer = fmax(
                item_outer * row_flex_factor[row],
                min_block_contribution(it.b, item_outer, basis),
            );
        }
        let mut used = row_gap * (rs as f64 - 1.0);
        let mut growable = 0;
        let mut crosses_flex = false;
        for k in row..row + rs {
            used += row_height[k];
            if !row_fixed[k] {
                growable += 1;
            }
            if row_flex[k] || row_fr[k] > 0.0 {
                crosses_flex = true;
            }
        }
        if crosses_flex && rs > 1 {
            continue;
        }
        if item_outer > used && growable > 0 {
            if rs == 1 {
                if !row_fixed[row] {
                    row_height[row] += item_outer - used;
                }
            } else {
                let span = RowSpan {
                    fixed: &row_fixed[row..row + rs],
                    limit: &row_limit[row..row + rs],
                    min_intrinsic: &row_min_intrinsic[row..row + rs],
                    max_intrinsic: &row_max_intrinsic[row..row + rs],
                };
                distribute_span(
                    &mut row_height[row..row + rs],
                    &span,
                    &mut target,
                    item_outer - used,
                );
            }
        }
        if rs == 1 && !row_fixed[row] && item_outer > row_limit[row] {
            row_limit[row] = item_outer;
        }
    }
    if !definite_rows {
        for it in &items {
            let row = it.row;
            if row < 0 || row >= n_rows || it.row_span < 2 {
                continue;
            }
            let rs = it.row_span.min(n_rows - row) as usize;
            let row = row as usize;
            let mut used = row_gap * (rs as f64 - 1.0);
            let mut fr_total = 0.0;
            for k in row..row + rs {
                used += row_height[k];
                fr_total += row_fr[k];
            }
            if fr_total <= 0.0 || it.outer_h <= used {
                continue;
            }
            for k in row..row + rs {
                row_height[k] += (it.outer_h - used) * row_fr[k] / fr_total;
            }
        }
    }

    let gaps_between = if n_rows > 1 {
        row_gap * f64::from(n_rows - 1)
    } else {
        0.0
    };
    if rows_sg.is_none() && basis > 0.0 && n_rows > 0 {
        let mut over = gaps_between - basis;
        let mut shrinkable = vec![0.0f64; nr + 1];
        let mut shrink_total = 0.0;
        for r in 0..nr {
            over += row_height[r];
            if let Some(tk) = row_tracks.track(r as i32)
                && tk.has_min != 0
                && (tk.kind == TRACK_PX || tk.kind == TRACK_PERCENT)
            {
                let mn = track_min_px(tk, basis);
                if row_height[r] > mn {
                    shrinkable[r] = row_height[r] - mn;
                    shrink_total += shrinkable[r];
                }
            }
        }
        if over > 0.0 && shrink_total > 0.0 {
            let take = fmin(over, shrink_total);
            for r in 0..nr {
                if shrinkable[r] > 0.0 {
                    row_height[r] -= take * shrinkable[r] / shrink_total;
                }
            }
        }
        expand_flexible_rows(&mut row_height[..nr], &row_tracks, basis - gaps_between);
    } else if rows_sg.is_none() && n_rows > 0 {
        let mnv = style.get(PropId::MinHeight);
        let min_h = if is_length_or_calc(mnv) {
            ffi::height_to_content(b, ffi::used_height(b, mnv, cw, -1.0))
        } else {
            -1.0
        };
        let mut used = gaps_between;
        for &h in &row_height[..nr] {
            used += h;
        }
        if min_h > used {
            expand_flexible_rows(&mut row_height[..nr], &row_tracks, min_h - gaps_between);
        }
    }

    let mut cursor_y = rows_sg.as_ref().map_or(inner_y, |s| s.y[0]);
    let mut grid_rows: Vec<Row> = Vec::with_capacity(nr);
    for &h in &row_height[..nr] {
        grid_rows.push(Row {
            top: cursor_y,
            height: h,
        });
        cursor_y += h + row_gap;
    }
    if !grid_rows.is_empty() {
        cursor_y -= row_gap;
    }

    let measured = cursor_y - inner_y;
    if rows_sg.is_none()
        && basis <= 0.0
        && measured > 0.0
        && let Some(rt) = rows_tracks
    {
        let mut changed = false;
        let n = n_rows.min(rt.n).max(0) as usize;
        for (h, tk) in row_height[..n].iter_mut().zip(&rt.tracks[..n]) {
            if tk.kind != TRACK_PERCENT {
                continue;
            }
            let resolved = tk.v * measured / 100.0;
            if (resolved - *h).abs() > 0.01 {
                *h = resolved;
                changed = true;
            }
        }
        if changed {
            let mut y = inner_y;
            for (r, gr) in grid_rows.iter_mut().enumerate() {
                gr.top = y;
                gr.height = row_height[r];
                y += row_height[r] + row_gap;
            }
        }
    }
    let mut total_extra = 0.0;
    let hv = style.get(PropId::Height);
    if is_length_or_calc(hv) && !grid_rows.is_empty() {
        let mut eh = ffi::clamp_height(Some(style), ffi::used_height(b, hv, cw, -1.0));
        if style_has(Some(style), PropId::BoxSizing, c"border-box") {
            let (p, bd) = (b.padding(), b.border());
            eh -= p.top + p.bottom + bd.top + bd.bottom;
        }
        if eh > measured {
            total_extra = eh - measured;
        }
    }
    let acont = keyword_or(Some(style), PropId::AlignContent, c"stretch").to_bytes();
    let ac_stretch = acont == b"stretch" || acont == b"normal";
    let n_grid_rows = grid_rows.len();
    let mut row_extra = vec![0.0f64; n_grid_rows + 1];
    let mut row_extra_before = vec![0.0f64; n_grid_rows + 1];
    if ac_stretch && n_grid_rows > 0 && total_extra > 0.0 && rows_sg.is_none() {
        let row_auto: Vec<bool> = (0..n_grid_rows)
            .map(|r| {
                row_tracks
                    .track(r as i32)
                    .is_none_or(|tk| tk.kind == TRACK_AUTO)
            })
            .collect();
        let stretchable = row_auto.iter().filter(|&&a| a).count();
        if stretchable > 0 {
            let share = total_extra / stretchable as f64;
            let mut before = 0.0;
            for r in 0..n_grid_rows {
                row_extra_before[r] = before;
                row_extra[r] = if row_auto[r] { share } else { 0.0 };
                before += row_extra[r];
            }
        }
    }
    let (mut group_off, mut row_between) = (0.0, 0.0);
    if !ac_stretch && total_extra > 0.0 {
        match acont {
            b"center" => group_off = total_extra / 2.0,
            b"end" | b"flex-end" => group_off = total_extra,
            _ => (group_off, row_between) = content_distribution(acont, total_extra, n_grid_rows),
        }
    }
    let align_items = keyword_or(Some(style), PropId::AlignItems, c"stretch");
    for it in &items {
        let c = it.b;
        let r = it.row;
        if r < 0 || r as usize >= n_grid_rows {
            continue;
        }
        let r = r as usize;
        let span = it.row_span.max(1) as usize;
        let mut row_h = 0.0;
        for k in 0..span.min(n_grid_rows - r) {
            row_h += grid_rows[r + k].height + row_extra[r + k];
            if k > 0 {
                row_h += row_gap + row_between;
            }
        }
        let cs = style_of(c);
        let a_eff =
            self_alignment(cs, None, PropId::AlignSelf, PropId::AlignItems, align_items).to_bytes();
        let a_stretch = a_eff == b"stretch" || a_eff == b"normal";
        let item_outer = outer_height(c);
        let free_h = row_h - item_outer;
        let mut dy_align = 0.0;
        let mt_auto = style_has(cs, PropId::MarginTop, c"auto");
        let mb_auto = style_has(cs, PropId::MarginBottom, c"auto");
        if (mt_auto || mb_auto) && free_h > 0.5 {
            dy_align = if mt_auto && mb_auto {
                free_h / 2.0
            } else if mt_auto {
                free_h
            } else {
                0.0
            };
        } else if a_stretch {
            let i_has_h = is_length_or_calc(cs.and_then(|s| s.get(PropId::Height)));
            let row_definite = rows_sg.is_none()
                && rows_tracks.is_some_and(|rt| {
                    (r as i32) < rt.n && {
                        let rk = rt.tracks[r].kind;
                        rk == TRACK_PX || rk == TRACK_PERCENT || (rk == TRACK_FR && basis > 0.0)
                    }
                });
            let shrinks_to_row = span == 1
                && free_h < -0.5
                && (row_definite || min_block_contribution(c, item_outer, basis) < item_outer);
            if !i_has_h && c.kind() == BoxKind::Block && (free_h > 0.5 || shrinks_to_row) {
                let mut h = c.content_height() + free_h;
                c.set_content_height(h);
                let max_h = stretched_item_max_height(c);
                if max_h >= 0.0 && h > max_h {
                    h = max_h;
                    c.set_content_height(h);
                }
            }
        } else if free_h > 0.5 {
            match a_eff {
                b"center" => dy_align = free_h / 2.0,
                b"end" | b"flex-end" | b"last baseline" => dy_align = free_h,
                b"self-end" | b"self-start" => {
                    let far = ffi::start_is_far_side(cs, false);
                    if far == (a_eff == b"self-start") {
                        dy_align = free_h;
                    }
                }
                _ => {}
            }
        }
        let row_top = grid_rows[r].top + row_extra_before[r] + group_off + row_between * r as f64;
        let dy = row_top + dy_align - c.y();
        if dy != 0.0 {
            ffi::shift(c, 0.0, dy);
        }
    }
    if total_extra > 0.0 {
        cursor_y += total_extra;
    }

    ffi::free_track_array(b.grid_tracks(true));
    ffi::free_track_array(b.grid_tracks(false));
    let col_tracks = ffi::new_track_array();
    let row_tracks_out = ffi::new_track_array();
    for t in 0..n_cols.max(0) as usize {
        ffi::append_track(col_tracks, track_edge(col_x[t], col_x[t] + col_sizes[t]));
    }
    for (r, gr) in grid_rows.iter().enumerate() {
        let top = gr.top + row_extra_before[r] + group_off + row_between * r as f64;
        ffi::append_track(
            row_tracks_out,
            track_edge(top, top + gr.height + row_extra[r]),
        );
    }
    b.set_grid_tracks(col_tracks, row_tracks_out);
    b.set_grid_explicit(explicit_cols, row_line_tracks);
    cursor_y
}
