//! Southstar — the C ABI of src/paint.h, and of src/paint_internal.h while paint.c is ported, over the cairo, ns-pango and engine bindings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod cairo;
pub mod engine;
pub mod pango;

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use core::mem::ManuallyDrop;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, boolean};
use southstar_layout::{BoxRef, NsBox, Style};
use southstar_style::StyleRef;

use self::cairo::Cr;
use self::engine::FontMetrics;
use self::pango::{AttrList, Layout, RawAttribute};
use crate::{marker, text, util};

fn style<'a>(s: *const Style) -> Option<StyleRef<'a>> {
    unsafe { StyleRef::from_ptr(s) }
}

fn layout(raw: *mut c_void) -> Option<ManuallyDrop<Layout>> {
    unsafe { Layout::borrowed(raw) }
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_text_context() -> *mut c_void {
    text::context().raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_create_layout() -> *mut c_void {
    text::create_layout().into_raw()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_pango_font_size(size_px: c_double) -> c_int {
    text::pango_font_size(size_px)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_pango_weight_from_css(weight: c_int) -> c_int {
    text::weight_from_css(weight)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_pango_stretch_from_css(rank: c_int) -> c_int {
    text::stretch_from_css(rank)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_wrap_mode_for(s: *const Style) -> c_int {
    text::wrap_mode_for(style(s))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_style_is_nowrap(s: *const Style) -> GBoolean {
    boolean(text::is_nowrap(style(s)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_css_line_height_px(s: *const Style) -> c_double {
    text::css_line_height_px(style(s))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_normal_line_height_px(s: *const Style) -> c_double {
    text::normal_line_height_px(style(s))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_apply_css_line_spacing(l: *mut c_void, s: *const Style) {
    text::apply_css_line_spacing(layout(l).as_deref(), style(s));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_apply_i18n(l: *mut c_void, attrs: *mut c_void, b: *const NsBox) {
    let attrs = (!attrs.is_null()).then(|| unsafe { AttrList::borrowed(attrs) });
    text::apply_i18n(layout(l).as_deref(), attrs.as_deref(), unsafe {
        BoxRef::from_ptr(b)
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_apply_font_features(
    attrs: *mut c_void,
    s: *const Style,
    start: c_uint,
    end: c_uint,
) {
    let Some(s) = style(s) else {
        return;
    };
    if attrs.is_null() {
        return;
    }
    let attrs = unsafe { AttrList::borrowed(attrs) };
    text::apply_font_features(&attrs, s, start, end);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_font_features_attr_from_values(
    kerning: c_int,
    ligatures: *const c_char,
    settings: *const c_char,
) -> *mut RawAttribute {
    text::font_features_attr(kerning, c_str(ligatures), c_str(settings))
        .map_or(core::ptr::null_mut(), pango::Attribute::into_raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_font_variations_attr_from_values(
    settings: *const c_char,
) -> *mut RawAttribute {
    text::font_variations_attr(c_str(settings))
        .map_or(core::ptr::null_mut(), pango::Attribute::into_raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_font_metrics(
    family: *const c_char,
    size_px: c_double,
    weight: c_int,
    italic: GBoolean,
    out: *mut FontMetrics,
) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    text::font_metrics(c_str(family), size_px, weight, italic != 0, out);
}

unsafe extern "C" fn font_available_cb(family: *const c_char) -> GBoolean {
    boolean(text::font_available(c_str(family)))
}

unsafe extern "C" fn font_generation_cb() -> u64 {
    text::font_generation()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_register_font_oracle() {
    engine::set_font_oracle(font_available_cb, font_generation_cb, ns_paint_font_metrics);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_apply_inline_font(l: *mut c_void, s: *const Style) {
    if let Some(l) = layout(l) {
        text::apply_inline_font(&l, style(s));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_apply_text_align(l: *mut c_void, s: *const Style) {
    if let Some(l) = layout(l) {
        text::apply_text_align(&l, style(s));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_start_align_overflow(l: *mut c_void) {
    if let Some(l) = layout(l) {
        text::start_align_overflow(&l);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_apply_nowrap_align_width(l: *mut c_void, b: *const NsBox) {
    if let (Some(l), Some(b)) = (layout(l), unsafe { BoxRef::from_ptr(b) }) {
        text::apply_nowrap_align_width(&l, b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_inline_y_offset_for_layout(
    b: *const NsBox,
    l: *mut c_void,
) -> c_double {
    match (unsafe { BoxRef::from_ptr(b) }, layout(l)) {
        (Some(b), Some(l)) => text::inline_y_offset_for_layout(b, &l),
        _ => 0.0,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_inherited_style(b: *const NsBox) -> *const Style {
    unsafe { BoxRef::from_ptr(b) }
        .and_then(util::inherited_style)
        .map_or(core::ptr::null(), StyleRef::as_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_li_is_inside(s: *const Style) -> GBoolean {
    boolean(marker::li_is_inside(style(s)))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_list_ordinals_begin() {
    marker::ordinals_begin();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_list_ordinals_end() {
    marker::ordinals_end();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_li_marker_text(
    li: *const NsNode,
    s: *const Style,
    out: *mut c_char,
    out_sz: usize,
) -> GBoolean {
    let Some(text) = marker::li_marker_text(unsafe { Node::from_ptr(li) }, style(s), out_sz) else {
        return 0;
    };
    if out.is_null() {
        return 0;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(text.as_ptr(), out.cast::<u8>(), text.len());
        *out.add(text.len()) = 0;
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_marker(cr: *mut c_void, b: *const NsBox) {
    if let Some(b) = unsafe { BoxRef::from_ptr(b) } {
        marker::paint_marker(unsafe { Cr::from_raw(cr) }, b);
    }
}
