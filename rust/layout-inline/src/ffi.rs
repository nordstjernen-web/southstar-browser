//! Southstar — the inline layout entry points layout.c and paint call and the layout.c, paint and css.c calls inline layout makes back, behind safe wrappers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GStr};
use southstar_layout::{BoxRef, InlineAttr, NsBox, Style};
use southstar_paint::ffi as paint;
use southstar_paint::ffi::pango::{AttrList, Attribute, Context, Layout};
use southstar_style::StyleRef;

use crate::{attrs, intrinsic, layout};

unsafe extern "C" {
    fn ns_layout_box(b: *const NsBox, parent_content_width: f64, inherited: *const Style);
    fn ns_layout_shift_box_tree(b: *const NsBox, dx: f64, dy: f64);
    fn ns_inline_text_indent_px(run: *const NsBox, s: *const Style, basis: f64) -> f64;
    fn ns_layout_box_first_baseline(b: *const NsBox, out: *mut f64) -> GBoolean;
    fn ns_layout_box_clips_children(b: *const NsBox) -> GBoolean;
    fn ns_layout_inline_attr_control_width(r: *const InlineAttr, b: *const NsBox) -> f64;
    fn ns_layout_box_is_abs_placeholder(b: *const NsBox) -> GBoolean;
    fn ns_layout_abs_static_target(b: *const NsBox) -> *const c_void;
    fn ns_layout_record_abs_static(dom: *const c_void, run: *const NsBox, rel_x: f64, rel_y: f64);
    fn ns_layout_measure_inline_atomics_begin(
        b: *const NsBox,
        parent_style: *const Style,
        max_content: GBoolean,
    ) -> *mut c_void;
    fn ns_layout_measure_inline_atomics_end(saved: *mut c_void);
    fn ns_math_measure(math: *const c_void, font_px: f64, w: *mut f64, a: *mut f64, d: *mut f64);
    fn ns_input_is_one_line_text(n: *const NsNode) -> GBoolean;
    fn ns_css_writing_mode(s: *const Style) -> c_int;
    fn ns_css_text_orientation(s: *const Style) -> c_int;
    fn ns_vertical_stack_text(text: *const c_char) -> *mut c_char;
}

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

pub(crate) fn layout_box(b: BoxRef<'_>, width: f64, inherited: *const Style) {
    unsafe { ns_layout_box(b.as_ptr(), width, inherited) };
}

pub(crate) fn shift(b: BoxRef<'_>, dx: f64, dy: f64) {
    unsafe { ns_layout_shift_box_tree(b.as_ptr(), dx, dy) };
}

pub(crate) fn text_indent(run: BoxRef<'_>, s: *const Style, basis: f64) -> f64 {
    unsafe { ns_inline_text_indent_px(run.as_ptr(), s, basis) }
}

pub(crate) fn first_baseline(b: BoxRef<'_>) -> Option<f64> {
    let mut out = 0.0;
    (unsafe { ns_layout_box_first_baseline(b.as_ptr(), &mut out) } != 0).then_some(out)
}

pub(crate) fn clips_children(b: BoxRef<'_>) -> bool {
    unsafe { ns_layout_box_clips_children(b.as_ptr()) != 0 }
}

pub(crate) fn control_width(r: &InlineAttr, b: BoxRef<'_>) -> f64 {
    unsafe { ns_layout_inline_attr_control_width(r, b.as_ptr()) }
}

pub(crate) fn is_abs_placeholder(b: BoxRef<'_>) -> bool {
    unsafe { ns_layout_box_is_abs_placeholder(b.as_ptr()) != 0 }
}

pub(crate) fn abs_static_target(b: BoxRef<'_>) -> *const c_void {
    unsafe { ns_layout_abs_static_target(b.as_ptr()) }
}

pub(crate) fn record_abs_static(dom: *const c_void, run: BoxRef<'_>, rel_x: f64, rel_y: f64) {
    unsafe { ns_layout_record_abs_static(dom, run.as_ptr(), rel_x, rel_y) };
}

pub(crate) struct SavedAtomics(*mut c_void);

pub(crate) fn measure_atomics_begin(
    b: BoxRef<'_>,
    parent_style: *const Style,
    max_content: bool,
) -> SavedAtomics {
    SavedAtomics(unsafe {
        ns_layout_measure_inline_atomics_begin(
            b.as_ptr(),
            parent_style,
            southstar_glib::boolean(max_content),
        )
    })
}

pub(crate) fn measure_atomics_end(saved: SavedAtomics) {
    unsafe { ns_layout_measure_inline_atomics_end(saved.0) };
}

pub(crate) fn math_ascent(dom: *const c_void, font_px: f64) -> f64 {
    let (mut w, mut a, mut d) = (0.0, 0.0, 0.0);
    unsafe { ns_math_measure(dom, font_px, &mut w, &mut a, &mut d) };
    a
}

pub(crate) fn is_one_line_text_input(dom: *const c_void) -> bool {
    unsafe { ns_input_is_one_line_text(dom.cast()) != 0 }
}

pub(crate) fn writing_mode(s: *const Style) -> c_int {
    unsafe { ns_css_writing_mode(s) }
}

pub(crate) fn text_orientation(s: *const Style) -> c_int {
    unsafe { ns_css_text_orientation(s) }
}

pub(crate) fn vertical_stack_text(text: &CStr) -> Option<GStr> {
    unsafe { GStr::take(ns_vertical_stack_text(text.as_ptr())) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline(
    b: *mut NsBox,
    content_width: f64,
    parent_style: *const Style,
) {
    if let Some(b) = unsafe { BoxRef::from_ptr(b) } {
        layout::inline_layout(b, content_width, parent_style);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_line_height(parent_style: *const Style) -> f64 {
    layout::line_height(parent_style)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_natural_width(
    b: *mut NsBox,
    parent_style: *const Style,
) -> f64 {
    unsafe { BoxRef::from_ptr(b) }.map_or(0.0, |b| intrinsic::natural_width(b, parent_style))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_inline_min_width(
    b: *mut NsBox,
    parent_style: *const Style,
) -> f64 {
    unsafe { BoxRef::from_ptr(b) }.map_or(0.0, |b| intrinsic::min_width(b, parent_style))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_apply_inline_spacing(
    list: *mut c_void,
    s: *const Style,
    text: *const c_char,
) {
    if list.is_null() || text.is_null() {
        return;
    }
    let list = unsafe { AttrList::borrowed(list) };
    let text = unsafe { CStr::from_ptr(text) };
    attrs::apply_spacing(&list, style(s), text.to_bytes());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_layout_apply_inline_layout_attrs(list: *mut c_void, b: *const NsBox) {
    if list.is_null() {
        return;
    }
    if let Some(b) = unsafe { BoxRef::from_ptr(b) } {
        attrs::apply_layout_attrs(&*unsafe { AttrList::borrowed(list) }, b);
    }
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
