//! Southstar — block flow: placing floats and clearing them, collapsing margins through and between children, wrapping lines past floats, aligning replaced and table children, and spreading the flow over multicol columns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};

use southstar_layout::{BoxKind, BoxRef, NsBox, Style, children};
use southstar_layout_float::floats::{self, Outer};
use southstar_layout_float::{
    FloatRef, box_establishes_bfc, clear_kind_of, float_side_of, multicol_column_host,
    multicol_distributable_children,
};
use southstar_style::{Display, Kind, PropId, ValueRef, display_of};

use crate::align::legacy_align_block_child;
use crate::dispatch::{layout_box, shift_box_tree};
use crate::ffi::{
    ContentBox, containing_block_definite_height, dom_of, edges_into_box, height_keyword_stretches,
    inline_box_can_fragment_multicol, inline_line_height, inline_run_break_before,
    inline_run_split, length_resolve, measure_natural_width, min_width_of, multicol_single_inline,
    resolve_used_height, style_is_absolute_or_fixed, style_of, style_ref, used_column_count,
};
use crate::fieldset::c_max;
use crate::flex::box_is_border_box;

struct Inherited {
    b: *const NsBox,
    floats: Vec<FloatRef>,
    inner_x: f64,
    cw: f64,
}

thread_local! {
    static MARGIN_ABOVE: Cell<(*const NsBox, f64)> = const { Cell::new((core::ptr::null(), 0.0)) };
    static INHERITED_FLOATS: RefCell<Option<Inherited>> = const { RefCell::new(None) };
}

const BOX_NORMAL: u8 = 0;
const OUTER_BLOCK: u8 = 1;
const INNER_RUBY: u8 = 5;
const INTERNAL_NONE: u8 = 0;
const INTERNAL_TABLE_CAPTION: u8 = 8;
const LAYOUT_UNIT: f64 = 1.0 / 64.0;

#[derive(Clone, Copy)]
pub struct FlowStart {
    pub collapse_top_with_parent: bool,
    pub collapse_bottom_with_parent: bool,
}

fn is_length_or_calc(v: Option<ValueRef<'_>>) -> bool {
    v.is_some_and(|v| matches!(v.kind(), Kind::Length | Kind::Calc))
}

fn style_value(b: BoxRef<'_>, p: PropId) -> Option<ValueRef<'_>> {
    style_of(b).and_then(|s| s.get(p))
}

pub fn block_height_is_auto(b: BoxRef<'_>, width_basis: f64) -> bool {
    let Some(style) = style_of(b) else {
        return true;
    };
    if resolve_used_height(b, style.get(PropId::MinHeight), width_basis, -1.0) > 0.0 {
        return false;
    }
    let hv = style.get(PropId::Height);
    if height_keyword_stretches(hv) {
        return containing_block_definite_height(b) < 0.0;
    }
    if is_length_or_calc(hv) {
        return resolve_used_height(b, hv, width_basis, -1.0) < 0.0;
    }
    true
}

fn collapsed_margin(a: f64, b: f64) -> f64 {
    let pa = if a > 0.0 { a } else { 0.0 };
    let pb = if b > 0.0 { b } else { 0.0 };
    let na = if a < 0.0 { a } else { 0.0 };
    let nb = if b < 0.0 { b } else { 0.0 };
    let positive = if pa > pb { pa } else { pb };
    let negative = if na < nb { na } else { nb };
    positive + negative
}

fn display_is_block_level(d: Display) -> bool {
    d.box_ == BOX_NORMAL
        && d.outer == OUTER_BLOCK
        && d.inner != INNER_RUBY
        && (d.internal == INTERNAL_NONE || d.internal == INTERNAL_TABLE_CAPTION)
}

fn is_replaced(c: BoxRef<'_>) -> bool {
    matches!(c.kind(), BoxKind::Image | BoxKind::Video | BoxKind::Svg)
}

fn is_block_level_replaced(c: BoxRef<'_>) -> bool {
    is_replaced(c) && display_is_block_level(display_of(style_of(c)))
}

fn empty_block_collapses_through(c: BoxRef<'_>, clear: i32) -> bool {
    let (p, bd) = (c.padding(), c.border());
    c.kind() == BoxKind::Block
        && clear == 0
        && c.content_height() <= 0.0
        && p.top <= 0.0
        && p.bottom <= 0.0
        && bd.top <= 0.0
        && bd.bottom <= 0.0
        && !box_establishes_bfc(c)
}

fn horizontal_outer(c: BoxRef<'_>, inner: f64) -> f64 {
    let (m, p, bd) = (c.margin(), c.padding(), c.border());
    inner + p.left + p.right + bd.left + bd.right + m.left + m.right
}

fn vertical_border_box(c: BoxRef<'_>) -> f64 {
    let (p, bd) = (c.padding(), c.border());
    c.content_height() + p.top + p.bottom + bd.top + bd.bottom
}

struct Flow<'a> {
    b: BoxRef<'a>,
    cw: f64,
    inner_x: f64,
    inner_y: f64,
    child_inherited: *const Style,
    floats: Vec<FloatRef>,
    cursor_y: f64,
    prev_margin_bottom: f64,
    top_through: f64,
    float_shift: f64,
    outer_margin: f64,
    collapse_top_with_parent: bool,
    inline_line_top: f64,
}

impl<'a> Flow<'a> {
    fn offsets_at(&self, y: f64) -> (f64, f64) {
        floats::offsets_at(&self.floats, y)
    }

    fn readable_offsets(&mut self) -> (f64, f64) {
        let (mut l, mut r) = self.offsets_at(self.cursor_y);
        if !self.floats.is_empty() {
            let (_, al, ar) =
                floats::advance_to_readable_width(&self.floats, self.cw, &mut self.cursor_y);
            l = al;
            r = ar;
        }
        (l, r)
    }

    fn place_float(&mut self, c: BoxRef<'a>, fside: i32, clr: i32) {
        let cw = self.cw;
        edges_into_box(c, cw);
        let float_max_w = cw;
        let mut float_floor_w = 0.0;
        let wv2 = style_value(c, PropId::Width);
        let mxw2 = style_value(c, PropId::MaxWidth);
        let mnw2 = style_value(c, PropId::MinWidth);
        let (m, p, bd) = (c.margin(), c.padding(), c.border());
        let float_sizing_extras = if box_is_border_box(c) {
            p.left + p.right + bd.left + bd.right
        } else {
            0.0
        };
        let mut cw_for_float;
        if is_length_or_calc(wv2) {
            cw_for_float = length_resolve(wv2, cw, 0.0) - float_sizing_extras;
            if cw_for_float < 0.0 {
                cw_for_float = 0.0;
            }
        } else {
            let mut cap = float_max_w - p.left - p.right - bd.left - bd.right - m.left - m.right;
            if cap < 0.0 {
                cap = 0.0;
            }
            cw_for_float = if height_keyword_stretches(wv2) {
                cap
            } else {
                measure_natural_width(c, self.child_inherited)
            };
            if cw_for_float > cap {
                cw_for_float = cap;
            }
            if !height_keyword_stretches(wv2) {
                float_floor_w = min_width_of(c, self.child_inherited);
                if cw_for_float < float_floor_w {
                    cw_for_float = float_floor_w;
                }
            }
            if cw_for_float < 0.0 {
                cw_for_float = 0.0;
            }
        }
        if is_length_or_calc(mxw2) {
            let mx = length_resolve(mxw2, cw, 0.0);
            if mx > 0.0 && cw_for_float > mx - float_sizing_extras {
                cw_for_float = c_max(mx - float_sizing_extras, 0.0);
            }
        }
        if is_length_or_calc(mnw2) {
            let mn = length_resolve(mnw2, cw, 0.0);
            if mn > 0.0 && cw_for_float < mn - float_sizing_extras {
                cw_for_float = mn - float_sizing_extras;
            }
        }
        let avail = horizontal_outer(c, cw_for_float);
        let mut float_y = self.cursor_y + self.prev_margin_bottom;
        if clr == 0 && self.inline_line_top >= 0.0 && self.inline_line_top < self.cursor_y {
            float_y = self.inline_line_top;
        }
        if clr != 0 {
            let y_after_clear = floats::clear_y(&self.floats, float_y, clr);
            if y_after_clear > float_y {
                float_y = y_after_clear;
            }
        }
        let (mut left_off, mut right_off) = self.offsets_at(float_y);
        while avail > cw - left_off - right_off + LAYOUT_UNIT && !self.floats.is_empty() {
            let mut next_y = float_y;
            let mut advanced = false;
            for f in &self.floats {
                if f.bottom > float_y && (!advanced || f.bottom < next_y) {
                    next_y = f.bottom;
                    advanced = true;
                }
            }
            if !advanced {
                break;
            }
            float_y = next_y;
            (left_off, right_off) = self.offsets_at(float_y);
        }
        let cw_capped =
            cw - left_off - right_off - m.left - m.right - p.left - p.right - bd.left - bd.right;
        if cw_for_float > cw_capped && cw_capped > 0.0 {
            cw_for_float = c_max(cw_capped, float_floor_w);
        }
        let tentative_outer = horizontal_outer(c, cw_for_float);
        if fside == 0 {
            c.set_x(self.inner_x + left_off);
        } else {
            c.set_x(self.inner_x + cw - right_off - tentative_outer);
        }
        c.set_y(float_y);
        c.set_content_width(cw_for_float);
        let layout_width = if is_length_or_calc(wv2) {
            cw
        } else {
            horizontal_outer(c, cw_for_float)
        };
        layout_box(c, layout_width, self.child_inherited);
        let actual_outer = horizontal_outer(c, c.content_width());
        if fside == 1 {
            let aligned_x = self.inner_x + cw - right_off - actual_outer;
            if aligned_x != c.x() {
                shift_box_tree(c, aligned_x - c.x(), 0.0);
            }
        }
        let (m, p, bd) = (c.margin(), c.padding(), c.border());
        let bottom =
            c.y() + m.top + c.content_height() + p.top + p.bottom + bd.top + bd.bottom + m.bottom;
        self.floats
            .push(FloatRef::placed(c, fside, c.y(), bottom, actual_outer));
    }

    fn place_block(&mut self, c: BoxRef<'a>, clr: i32) {
        let cw = self.cw;
        edges_into_box(c, cw);
        let mt = c.margin().top;
        let mut gap = collapsed_margin(mt, self.prev_margin_bottom);
        let mut c_margin_above = gap;
        let collapses_through_top = self.collapse_top_with_parent && self.cursor_y == self.inner_y;
        if collapses_through_top {
            c_margin_above = collapsed_margin(self.outer_margin, gap);
            gap = c_margin_above - self.outer_margin;
            floats::shift_placed(&mut self.floats, gap - self.float_shift);
            self.float_shift = gap;
            self.top_through = gap;
        }
        self.cursor_y += gap;
        if clr != 0 {
            let y_after_clear = floats::clear_y(&self.floats, self.cursor_y, clr);
            if y_after_clear > self.cursor_y {
                self.cursor_y = y_after_clear;
            }
        }
        let mut left_off = 0.0;
        let mut right_off = 0.0;
        let lines_wrap_floats = c.kind() == BoxKind::Block
            && floats::max_bottom(&self.floats) > self.cursor_y
            && !box_establishes_bfc(c);
        if !lines_wrap_floats {
            (left_off, right_off) = self.readable_offsets();
        }
        let mut cw_avail = cw - left_off - right_off;
        if cw_avail < 0.0 {
            cw_avail = 0.0;
        }
        c.set_x(self.inner_x + left_off);
        c.set_y(self.cursor_y - mt);
        let outer_floats = if lines_wrap_floats {
            Some(INHERITED_FLOATS.replace(Some(Inherited {
                b: c.as_ptr(),
                floats: self.floats.clone(),
                inner_x: self.inner_x,
                cw,
            })))
        } else {
            None
        };
        let outer_above = MARGIN_ABOVE.replace((c.as_ptr(), c_margin_above));
        layout_box(c, cw_avail, self.child_inherited);
        if let Some(outer) = outer_floats {
            INHERITED_FLOATS.set(outer);
        }
        MARGIN_ABOVE.set(outer_above);
        if box_establishes_bfc(c) && cw_avail > 0.0 {
            let c_h = vertical_border_box(c);
            let (span_l, span_r) =
                floats::offsets_over(&self.floats, self.cursor_y, self.cursor_y + c_h);
            if span_l > left_off || span_r > right_off {
                let retry = cw - c_max(span_l, left_off) - c_max(span_r, right_off);
                if retry > 0.0 && retry < cw_avail {
                    left_off = c_max(span_l, left_off);
                    cw_avail = retry;
                    c.set_x(self.inner_x + left_off);
                    c.set_y(self.cursor_y - mt);
                    layout_box(c, cw_avail, self.child_inherited);
                }
            }
        }
        let rtl = style_value(self.b, PropId::Direction).is_some_and(|v| v.is_keyword(c"rtl"));
        if rtl && !style_is_absolute_or_fixed(c.style()) {
            let (m, p, bd) = (c.margin(), c.padding(), c.border());
            let outer =
                m.left + m.right + c.content_width() + p.left + p.right + bd.left + bd.right;
            if (outer - cw_avail).abs() > 0.01 {
                shift_box_tree(c, cw_avail - outer, 0.0);
            }
        }
        if c.kind() == BoxKind::Block {
            legacy_align_block_child(c, self.inner_x + left_off, cw_avail, self.child_inherited);
        }
        floats::collect_escaping(c, &mut self.floats, 0);
        if empty_block_collapses_through(c, clr) {
            self.cursor_y -= gap;
            if collapses_through_top {
                self.top_through = 0.0;
            }
            self.prev_margin_bottom = collapsed_margin(
                collapsed_margin(self.prev_margin_bottom, mt),
                c.margin().bottom,
            );
        } else {
            self.collapse_top_with_parent = false;
            self.cursor_y += c.margin_top_through();
            if collapses_through_top {
                self.top_through += c.margin_top_through();
            }
            self.cursor_y += vertical_border_box(c);
            self.prev_margin_bottom = c.margin().bottom;
        }
        self.inline_line_top = -1.0;
    }

    fn place_inline(&mut self, c: BoxRef<'a>) {
        let cw = self.cw;
        self.cursor_y += self.prev_margin_bottom;
        self.prev_margin_bottom = 0.0;
        let (left_off, right_off) = self.readable_offsets();
        let mut cw_avail = cw - left_off - right_off;
        if cw_avail < 0.0 {
            cw_avail = 0.0;
        }
        c.set_x(self.inner_x + left_off);
        c.set_y(self.cursor_y);
        layout_box(c, cw_avail, self.child_inherited);
        if c.kind() == BoxKind::Inline
            && let Some(text) = c.text()
            && cw_avail < cw
        {
            let text_len = text.to_bytes().len();
            let band_bottom = floats::band_bottom(&self.floats, self.cursor_y);
            if band_bottom > self.cursor_y && c.y() + c.content_height() > band_bottom + 0.5 {
                let split = inline_run_break_before(c, band_bottom - c.y());
                if split > 0 && split < text_len {
                    inline_run_split(c, split);
                    layout_box(c, cw_avail, self.child_inherited);
                }
            }
        }
        floats::collect_escaping(c, &mut self.floats, 0);
        let line_height = inline_line_height(self.child_inherited);
        let below = c.content_height() - line_height;
        self.inline_line_top = c.y() + if 0.0 > below { 0.0 } else { below };
        self.cursor_y += c.content_height();
    }

    fn align_replaced_or_table(&self, c: BoxRef<'a>) {
        let cw = self.cw;
        let is_table = c.kind() == BoxKind::Table;
        let narrower = c.content_width() < cw;
        if !((is_replaced(c) || is_table) && narrower) {
            return;
        }
        let mut outer = c.content_width();
        if is_table {
            let (m, p, bd) = (c.margin(), c.padding(), c.border());
            outer += p.left + p.right + bd.left + bd.right + m.left + m.right;
        }
        let ta = style_ref(self.child_inherited).and_then(|s| s.get(PropId::TextAlign));
        let mut self_center = false;
        let mut self_right = false;
        if is_table {
            let al = dom_of(c).and_then(|d| d.attr(c"align"));
            if al.is_some_and(|a| a.to_bytes().eq_ignore_ascii_case(b"center")) {
                self_center = true;
            } else if al.is_some_and(|a| a.to_bytes().eq_ignore_ascii_case(b"right")) {
                self_right = true;
            }
            let ml = style_value(c, PropId::MarginLeft);
            let mr = style_value(c, PropId::MarginRight);
            if ml.is_some_and(|v| v.is_keyword(c"auto"))
                && mr.is_some_and(|v| v.is_keyword(c"auto"))
            {
                self_center = true;
            }
        }
        let ta_is = |kw: &core::ffi::CStr| ta.is_some_and(|v| v.is_keyword(kw));
        if (ta_is(c"center") || self_center) && outer < cw {
            shift_box_tree(c, self.inner_x + (cw - outer) / 2.0 - c.x(), 0.0);
        } else if (ta_is(c"right") || ta_is(c"end") || self_right) && outer < cw {
            shift_box_tree(c, self.inner_x + (cw - outer) - c.x(), 0.0);
        }
    }
}

struct Columns<'a> {
    n: i32,
    gap: f64,
    host: Option<BoxRef<'a>>,
}

fn plan_columns<'a>(b: BoxRef<'a>, cw: &mut f64, child_inherited: *const Style) -> Columns<'a> {
    let mut gap = 16.0;
    let mut n = style_of(b).map_or(1, |s| used_column_count(s, *cw, &mut gap));
    let mut host = None;
    if n > 1 {
        let distributable = multicol_distributable_children(b);
        let single_fragmentable_inline = b.first_child().is_some_and(|first| {
            first.next_sibling().is_none()
                && inline_box_can_fragment_multicol(first, child_inherited, *cw)
        });
        if distributable < 2 && !single_fragmentable_inline {
            host = multicol_column_host(b);
            if host.is_none() {
                n = 1;
            }
        }
    }
    if n > 1 {
        let col_w = (*cw - gap * f64::from(n - 1)) / f64::from(n);
        if col_w > 1.0 {
            *cw = col_w;
        } else {
            n = 1;
        }
    }
    b.set_columns(n);
    Columns { n, gap, host }
}

fn flow_height(c: BoxRef<'_>) -> f64 {
    if matches!(c.kind(), BoxKind::Block | BoxKind::Table) {
        vertical_border_box(c)
    } else {
        c.content_height()
    }
}

fn distribute_columns(
    b: BoxRef<'_>,
    cols: &Columns<'_>,
    cw: f64,
    inner_x: f64,
    inner_y: f64,
    cursor_y: &mut f64,
) {
    let n_cols = cols.n;
    let host = cols.host.unwrap_or(b);
    let first = cols.host.and_then(|h| h.first_child());
    let flow_x = first.map_or(inner_x, |f| f.x());
    let flow_y = first.map_or(inner_y, |f| f.y());
    let mut total_h = *cursor_y - inner_y;
    if cols.host.is_some() {
        total_h = 0.0;
        for c in children(host) {
            total_h += flow_height(c);
        }
    }
    let target_h = total_h / f64::from(n_cols);
    let mut cur_y = 0.0;
    let mut max_col_h = 0.0;
    let mut cur_col = 0;
    for c in children(host) {
        let c_full_h = flow_height(c);
        if cur_col < n_cols - 1 && cur_y > 0.0 && cur_y + c_full_h > target_h {
            cur_col += 1;
            cur_y = 0.0;
        }
        let target_x = flow_x + f64::from(cur_col) * (cw + cols.gap);
        let target_y = flow_y + cur_y;
        let dx = target_x - c.x();
        let dy = target_y - c.y();
        if dx != 0.0 || dy != 0.0 {
            shift_box_tree(c, dx, dy);
        }
        cur_y += c_full_h;
        if cur_y > max_col_h {
            max_col_h = cur_y;
        }
    }
    b.set_content_width(cw * f64::from(n_cols) + cols.gap * f64::from(n_cols - 1));
    if let Some(host) = cols.host {
        host.set_content_width(b.content_width());
        host.set_content_height(max_col_h);
        *cursor_y = flow_y
            + max_col_h
            + host.padding().bottom
            + host.border().bottom
            + host.margin().bottom;
    } else {
        *cursor_y = inner_y + max_col_h;
    }
}

pub fn layout_flow(
    b: BoxRef<'_>,
    content: &ContentBox,
    start: FlowStart,
    cursor_y: &mut f64,
) -> f64 {
    let mut cw = content.cw;
    let (above_box, above_margin) = MARGIN_ABOVE.get();
    let outer_margin = if core::ptr::eq(above_box, b.as_ptr()) {
        above_margin
    } else {
        b.margin().top
    };
    let cols = plan_columns(b, &mut cw, content.child_inherited);
    let mut floats = Vec::new();
    INHERITED_FLOATS.with_borrow(|inh| {
        if let Some(inh) = inh.as_ref().filter(|i| core::ptr::eq(i.b, b.as_ptr())) {
            let from = Outer {
                inner_x: inh.inner_x,
                cw: inh.cw,
            };
            floats = floats::inherit(&inh.floats, &from, content.inner_x, cw, content.inner_y);
        }
    });
    let mut flow = Flow {
        b,
        cw,
        inner_x: content.inner_x,
        inner_y: content.inner_y,
        child_inherited: content.child_inherited,
        floats,
        cursor_y: *cursor_y,
        prev_margin_bottom: 0.0,
        top_through: 0.0,
        float_shift: 0.0,
        outer_margin,
        collapse_top_with_parent: start.collapse_top_with_parent,
        inline_line_top: -1.0,
    };
    let mut next = b.first_child();
    while let Some(c) = next {
        c.set_x(flow.inner_x);
        let fside = float_side_of(style_of(c));
        let clr = clear_kind_of(style_of(c));
        let floating =
            fside >= 0 && (matches!(c.kind(), BoxKind::Block | BoxKind::Table) || is_replaced(c));
        if floating {
            flow.place_float(c, fside, clr);
        } else {
            if matches!(c.kind(), BoxKind::Block | BoxKind::Table) || is_block_level_replaced(c) {
                flow.place_block(c, clr);
            } else {
                flow.place_inline(c);
            }
            flow.align_replaced_or_table(c);
        }
        next = c.next_sibling();
    }
    let mut inner_y = flow.inner_y;
    if flow.top_through != 0.0 {
        let mut m = b.margin();
        m.top += flow.top_through;
        b.set_margin(m);
        b.set_margin_top_through(flow.top_through);
        inner_y += flow.top_through;
    }
    if start.collapse_bottom_with_parent {
        let mut m = b.margin();
        m.bottom = collapsed_margin(m.bottom, flow.prev_margin_bottom);
        b.set_margin(m);
    } else {
        flow.cursor_y += flow.prev_margin_bottom;
    }
    if box_establishes_bfc(b) && !flow.floats.is_empty() {
        let fb = floats::max_bottom(&flow.floats);
        if fb > flow.cursor_y {
            flow.cursor_y = fb;
        }
    }
    *cursor_y = flow.cursor_y;
    if cols.n > 1
        && !multicol_single_inline(
            b,
            content.inner_x,
            inner_y,
            cw,
            cols.gap,
            cols.n,
            content.child_inherited,
            cursor_y,
        )
    {
        distribute_columns(b, &cols, cw, content.inner_x, inner_y, cursor_y);
    }
    inner_y
}
