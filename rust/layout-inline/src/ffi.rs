//! Southstar — the inline entry points paint calls, the mathml and form-control calls inline layout makes, and the sibling-crate helpers it borrows.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_layout::{BoxRef, InlineAttr};
use southstar_layout_sizing::SavedGeometry;
use southstar_paint::text_layout::{AttrList, Attribute, Layout};
use southstar_style::StyleRef;

use crate::attrs;

pub(crate) const LINE_HEIGHT_KEY: &CStr = c"ns-css-line-height";

pub(crate) fn new_layout(parent_style: Option<StyleRef<'_>>) -> Layout {
    let layout = Layout::new(southstar_paint::text_context());
    southstar_paint::apply_inline_font(&layout, parent_style);
    layout
}

pub(crate) fn style_text_indent(s: Option<StyleRef<'_>>, basis: f64) -> f64 {
    southstar_layout_geom::text_indent_px(s, basis)
}

pub(crate) fn paint_inline_layout(run: BoxRef<'_>) -> Option<Layout> {
    southstar_paint::build_inline_layout(run)
}

pub(crate) fn wrap_mode_for(s: Option<StyleRef<'_>>) -> c_int {
    southstar_paint::wrap_mode_for(s)
}

pub(crate) fn apply_css_line_spacing(layout: &Layout, s: Option<StyleRef<'_>>) {
    southstar_paint::apply_css_line_spacing(layout, s);
}

pub(crate) fn apply_i18n(layout: &Layout, attrs: &AttrList, b: BoxRef<'_>) {
    southstar_paint::apply_i18n(layout, attrs, b);
}

pub(crate) fn apply_font_features(attrs: &AttrList, s: Option<StyleRef<'_>>) {
    southstar_paint::apply_font_features(attrs, s);
}

pub(crate) fn start_align_overflow(layout: &Layout) {
    southstar_paint::start_align_overflow(layout);
}

pub(crate) fn css_line_height_px(s: Option<StyleRef<'_>>) -> f64 {
    southstar_paint::css_line_height_px(s)
}

pub(crate) fn normal_line_height_px(s: Option<StyleRef<'_>>) -> f64 {
    southstar_paint::normal_line_height_px(s)
}

pub(crate) fn pango_font_size(size_px: f64) -> c_int {
    southstar_paint::pango_font_size(size_px)
}

pub(crate) fn font_features_attr(r: &InlineAttr) -> Option<Attribute> {
    southstar_paint::font_features_attr(r.font_kerning, r.font_ligatures(), r.font_features())
}

pub(crate) fn font_variations_attr(r: &InlineAttr) -> Option<Attribute> {
    southstar_paint::font_variations_attr(r.font_variations())
}

pub(crate) use southstar_layout_geom::dispatch::{layout_box, shift_box_tree as shift};

pub(crate) fn text_indent(run: BoxRef<'_>, s: Option<StyleRef<'_>>, basis: f64) -> f64 {
    southstar_layout_geom::inline_text_indent_px(run, s, basis)
}

pub(crate) use southstar_layout_geom::flex::box_first_baseline as first_baseline;

pub(crate) use southstar_layout_float::box_clips_children as clips_children;

pub(crate) use southstar_layout_content::inline_attr_control_width as control_width;

pub(crate) use southstar_layout_build::{
    abs_static_target, box_is_abs_placeholder as is_abs_placeholder, record_abs_static,
};
pub(crate) use southstar_layout_sizing::measure_inline_atomics_begin as measure_atomics_begin;

pub(crate) fn measure_atomics_end(saved: Option<Box<SavedGeometry<'_>>>) {
    if let Some(saved) = saved {
        southstar_layout_sizing::measure_inline_atomics_end(*saved);
    }
}

pub(crate) fn math_ascent(math: Option<Node<'_>>, font_px: f64) -> f64 {
    southstar_mathml::measure(math, font_px).1
}

pub(crate) fn is_one_line_text_input(dom: Option<Node<'_>>) -> bool {
    dom.is_some_and(southstar_dom::controls::is_one_line_text)
}

pub(crate) fn writing_mode(s: Option<StyleRef<'_>>) -> c_int {
    southstar_layout_geom::writing_mode(s)
}

pub(crate) fn text_orientation(s: Option<StyleRef<'_>>) -> c_int {
    southstar_css::text_orientation(s.map(StyleRef::data))
}

pub(crate) fn vertical_stack_text(text: &CStr) -> CString {
    southstar_layout_geom::vertical_stack_text(text.to_bytes())
}

pub fn apply_atomic_shapes(list: &AttrList, b: BoxRef<'_>) {
    attrs::apply_atomic_shapes(list, b);
}

pub fn layout_set_attrs(layout: &Layout, list: &AttrList, b: BoxRef<'_>) {
    attrs::set_attrs(layout, Some(list), Some(b));
}
