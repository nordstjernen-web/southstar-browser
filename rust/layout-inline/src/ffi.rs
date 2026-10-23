//! Southstar — the inline entry points paint calls, the mathml and form-control calls inline layout makes, and the sibling-crate helpers it borrows.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::Node;
use southstar_layout::{BoxRef, InlineAttr, NsBox, Style};
use southstar_layout_sizing::SavedGeometry;
use southstar_paint::ffi as paint;
use southstar_paint::text_layout::{AttrList, Attribute, Context, Layout};
use southstar_style::StyleRef;

use crate::attrs;

unsafe extern "C" {}

pub(crate) const LINE_HEIGHT_KEY: &CStr = c"ns-css-line-height";

pub(crate) fn style<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

pub(crate) fn node<'a>(dom: *const c_void) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(dom.cast()) }
}

pub(crate) fn new_layout(parent_style: *const Style) -> Layout {
    let layout = Layout::new(unsafe { Context::from_raw(paint::ns_paint_text_context()) });
    unsafe { paint::ns_paint_apply_inline_font(layout.raw(), parent_style) };
    layout
}

pub(crate) fn style_text_indent(s: *const Style, basis: f64) -> f64 {
    southstar_layout_geom::text_indent_px(style(s), basis)
}

pub(crate) fn paint_inline_layout(run: BoxRef<'_>) -> Option<Layout> {
    let raw = unsafe { paint::ns_paint_build_inline_layout(ptr::null_mut(), run.as_ptr()) };
    (!raw.is_null()).then(|| unsafe { Layout::from_owned(raw) })
}

pub(crate) fn wrap_mode_for(s: *const Style) -> c_int {
    unsafe { paint::ns_paint_wrap_mode_for(s) }
}

pub(crate) fn apply_css_line_spacing(layout: &Layout, s: *const Style) {
    unsafe { paint::ns_paint_apply_css_line_spacing(layout.raw(), s) };
}

pub(crate) fn apply_i18n(layout: &Layout, attrs: &AttrList, b: BoxRef<'_>) {
    unsafe { paint::ns_paint_apply_i18n(layout.raw(), attrs.raw(), b.as_ptr()) };
}

pub(crate) fn apply_font_features(attrs: &AttrList, s: *const Style) {
    unsafe { paint::ns_paint_apply_font_features(attrs.raw(), s, 0, u32::MAX) };
}

pub(crate) fn start_align_overflow(layout: &Layout) {
    unsafe { paint::ns_paint_start_align_overflow(layout.raw()) };
}

pub(crate) fn css_line_height_px(s: *const Style) -> f64 {
    unsafe { paint::ns_paint_css_line_height_px(s) }
}

pub(crate) fn normal_line_height_px(s: *const Style) -> f64 {
    unsafe { paint::ns_paint_normal_line_height_px(s) }
}

pub(crate) fn pango_font_size(size_px: f64) -> c_int {
    paint::ns_paint_pango_font_size(size_px)
}

fn c_ptr(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

pub(crate) fn font_features_attr(r: &InlineAttr) -> Option<Attribute> {
    unsafe {
        Attribute::from_raw(paint::ns_paint_font_features_attr_from_values(
            r.font_kerning,
            c_ptr(r.font_ligatures()),
            c_ptr(r.font_features()),
        ))
    }
}

pub(crate) fn font_variations_attr(r: &InlineAttr) -> Option<Attribute> {
    unsafe {
        Attribute::from_raw(paint::ns_paint_font_variations_attr_from_values(c_ptr(
            r.font_variations(),
        )))
    }
}

pub(crate) use southstar_layout::dispatch::{layout_box, shift_box_tree as shift};

pub(crate) fn text_indent(run: BoxRef<'_>, s: *const Style, basis: f64) -> f64 {
    southstar_layout_geom::inline_text_indent_px(run, style(s), basis)
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

pub(crate) fn is_one_line_text_input(dom: *const c_void) -> bool {
    node(dom).is_some_and(southstar_dom::controls::is_one_line_text)
}

pub(crate) fn writing_mode(s: *const Style) -> c_int {
    southstar_layout_geom::writing_mode(style(s))
}

pub(crate) fn text_orientation(s: *const Style) -> c_int {
    southstar_css::text_orientation(style(s).map(StyleRef::data))
}

pub(crate) fn vertical_stack_text(text: &CStr) -> CString {
    southstar_layout_geom::vertical_stack_text(text.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_inline_apply_atomic_shapes(list: *mut c_void, b: *const NsBox) {
    if list.is_null() {
        return;
    }
    if let Some(b) = unsafe { BoxRef::from_ptr(b) } {
        attrs::apply_atomic_shapes(&*unsafe { AttrList::borrowed(list) }, b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_inline_layout_set_attrs(
    layout: *mut c_void,
    list: *mut c_void,
    b: *const NsBox,
) {
    let Some(layout) = (unsafe { Layout::borrowed(layout) }) else {
        return;
    };
    let list = (!list.is_null()).then(|| unsafe { AttrList::borrowed(list) });
    attrs::set_attrs(&layout, list.as_deref(), unsafe { BoxRef::from_ptr(b) });
}
