//! Southstar — the layout.c calls flex layout makes back into block layout, and the entry points layout.c calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_layout_float::{kw_scrolls, overflow_axis_keyword};
use southstar_style::{PropId, StyleRef};

unsafe extern "C" {
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub use southstar_layout_sizing::{
    intrinsic_keyword_width, measure_natural_width, min_content_width_of, size_keyword_is_intrinsic,
};

pub use southstar_layout_float::box_is_scroll_container;

pub fn layout_box(b: BoxRef<'_>, parent_content_width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_ptr(), parent_content_width, inherited) }
}

pub fn overflow_scrolls(s: StyleRef<'_>, axis: PropId) -> bool {
    kw_scrolls(overflow_axis_keyword(Some(s), axis))
}

pub fn shift_box_tree(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) }
}

pub use southstar_layout_geom::flex::{
    align_is_baseline as flex_align_is_baseline, box_is_border_box as flex_box_is_border_box,
    gap_of as flex_gap_of, gap_px, grow_of as flex_grow_of, item_align as flex_item_align,
    item_baseline as flex_item_baseline, shrink_of as flex_shrink_of, wraps as flex_wraps,
};
