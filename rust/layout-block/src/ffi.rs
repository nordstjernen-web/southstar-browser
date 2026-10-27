//! Southstar — the mathml measure block layout calls and the sibling-crate helpers it borrows.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::Node;
use southstar_layout::{BoxRef, Style};
use southstar_style::StyleRef;

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn style_ref<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub use southstar_layout_sizing::{
    layout_image, measure_natural_width, size_keyword_is_intrinsic, text_input_intrinsic_width,
};

pub fn min_width_of(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    southstar_layout_sizing::min_width_of(Some(b), parent_style)
}

pub fn intrinsic_keyword_width(
    b: BoxRef<'_>,
    kw: Option<&CStr>,
    mi: *const Style,
    avail: f64,
) -> f64 {
    kw.map_or(-1.0, |kw| {
        southstar_layout_sizing::intrinsic_keyword_width(b, kw, mi, avail)
    })
}

pub use southstar_layout_tree::{
    join_splits as inline_runs_join_splits, reorder_children as reorder_children_by_order,
};

pub use southstar_layout_inline::{
    can_fragment as inline_box_can_fragment_multicol, layout_inline,
    line_height as inline_line_height, run_break_before as inline_run_break_before,
};
pub use southstar_layout_table::layout_table;

#[allow(clippy::too_many_arguments)]
pub fn multicol_single_inline(
    b: BoxRef<'_>,
    inner_x: f64,
    inner_y: f64,
    col_w: f64,
    col_gap: f64,
    n_cols: i32,
    child_inherited: *const Style,
    cursor_y: &mut f64,
) -> bool {
    let cols = southstar_layout_inline::Columns {
        inner_x,
        inner_y,
        col_w,
        col_gap,
        n_cols,
    };
    match southstar_layout_inline::layout_single_inline(b, &cols, child_inherited) {
        Some(y) => {
            *cursor_y = y;
            true
        }
        None => false,
    }
}

pub fn inline_run_split(run: BoxRef<'_>, split: usize) {
    southstar_layout_tree::run_split(run, split);
}

pub struct ContentBox {
    pub cw: f64,
    pub inner_x: f64,
    pub inner_y: f64,
    pub child_inherited: *const Style,
}

fn flex_frame(cb: &ContentBox, reverse: bool) -> southstar_layout_flex::Frame {
    southstar_layout_flex::Frame {
        cw: cb.cw,
        inner_x: cb.inner_x,
        inner_y: cb.inner_y,
        child_inherited: cb.child_inherited,
        reverse,
    }
}

pub fn layout_flex_row(
    b: BoxRef<'_>,
    cb: &ContentBox,
    reverse: bool,
    wrap: bool,
    parent_content_width: f64,
    cursor_y: &mut f64,
) {
    let at = flex_frame(cb, reverse);
    *cursor_y = if wrap {
        southstar_layout_flex::layout_row_wrap(b, &at)
    } else {
        southstar_layout_flex::layout_row(b, &at, parent_content_width)
    };
}

pub fn layout_flex_column(
    b: BoxRef<'_>,
    cb: &ContentBox,
    reverse: bool,
    parent_content_width: f64,
    cursor_y: &mut f64,
) {
    let at = flex_frame(cb, reverse);
    *cursor_y = southstar_layout_flex::layout_column(b, &at, parent_content_width);
}

pub fn layout_grid(b: BoxRef<'_>, cb: &ContentBox, cursor_y: &mut f64) {
    *cursor_y =
        southstar_layout_grid::layout_grid(b, cb.cw, cb.inner_x, cb.inner_y, cb.child_inherited);
}

pub use southstar_mathml::measure as math_measure;

pub fn set_container_dims(inline_px: f64, block_px: f64) {
    southstar_css::set_container_unit_dims(inline_px, block_px);
}
