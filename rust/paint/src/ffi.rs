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
use southstar_layout::{BoxRef, InlineAttr, NsBox, Style};
use southstar_style::StyleRef;

use self::cairo::Cr;
use self::engine::FontMetrics;
use self::pango::{AttrList, Layout, RawAttribute};
use crate::{decor, filter, inline, marker, mask, media, state, text};

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
pub extern "C" fn ns_paint_pango_font_size(size_px: c_double) -> c_int {
    text::pango_font_size(size_px)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_wrap_mode_for(s: *const Style) -> c_int {
    text::wrap_mode_for(style(s))
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
pub unsafe extern "C" fn ns_paint_start_align_overflow(l: *mut c_void) {
    if let Some(l) = layout(l) {
        text::start_align_overflow(&l);
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

fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

fn cr(raw: *mut c_void) -> Cr {
    unsafe { Cr::from_raw(raw) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_set_search(case_sensitive: GBoolean, active: *const NsBox) {
    state::set_search(case_sensitive != 0, active);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_set_js(js: *mut c_void) {
    state::set_js(js);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_set_anim(anim: *mut c_void) {
    state::set_anim(anim);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_set_caret_visible(visible: GBoolean) {
    state::set_caret_visible(visible != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_js() -> *mut c_void {
    state::js()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_anim() -> *mut c_void {
    state::anim()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_block(c: *mut c_void, b: *const NsBox) {
    if let Some(b) = box_ref(b) {
        decor::paint_block(cr(c), b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_hr(c: *mut c_void, b: *const NsBox) {
    if let Some(b) = box_ref(b) {
        decor::paint_hr(cr(c), b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_inline(
    c: *mut c_void,
    b: *const NsBox,
    highlight: *const c_char,
) {
    if let Some(b) = box_ref(b) {
        inline::paint_inline(cr(c), b, c_str(highlight));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_image(c: *mut c_void, b: *const NsBox) {
    if let Some(b) = box_ref(b) {
        media::paint_image(cr(c), b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_video(c: *mut c_void, b: *const NsBox) {
    if let Some(b) = box_ref(b) {
        media::paint_video(cr(c), b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_math(c: *mut c_void, b: *const NsBox) {
    if let Some(b) = box_ref(b) {
        media::paint_math(cr(c), b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_svg(c: *mut c_void, b: *const NsBox) {
    if let Some(b) = box_ref(b) {
        media::paint_svg(cr(c), b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_box_content_clip(c: *mut c_void, b: *const NsBox) -> GBoolean {
    boolean(box_ref(b).is_some_and(|b| media::apply_box_content_clip(cr(c), b)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_box_radii_path(
    c: *mut c_void,
    b: *const NsBox,
    x: c_double,
    y: c_double,
    w: c_double,
    h: c_double,
) {
    let radii = crate::radii::box_border_radii(box_ref(b));
    if radii.is_zero() {
        cr(c).rectangle(x, y, w, h);
    } else {
        crate::radii::rounded_rect_path(cr(c), x, y, w, h, radii);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_apply_image_filter(
    data: *mut u8,
    stride: c_int,
    w: c_int,
    h: c_int,
    filter: *const c_char,
) {
    if data.is_null() || stride <= 0 || h <= 0 {
        return;
    }
    let bytes = unsafe { core::slice::from_raw_parts_mut(data, stride as usize * h as usize) };
    filter::apply_image_filter(bytes, stride, w, h, c_str(filter));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_filter_has_bitmap_effect(filter: *const c_char) -> GBoolean {
    boolean(filter::filter_has_bitmap_effect(c_str(filter)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_mask_layers_paintable(s: *const Style) -> GBoolean {
    boolean(mask::mask_layers_paintable(style(s)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_mask_layers_pattern(
    c: *mut c_void,
    b: *const NsBox,
) -> *mut c_void {
    match box_ref(b) {
        Some(b) => mask::mask_layers_pattern(cr(c), b).into_raw(),
        None => core::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_drop_box_cache(b: *mut NsBox) {
    if let Some(b) = box_ref(b) {
        inline::drop_box_cache(b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_build_inline_layout(
    _cr: *mut c_void,
    b: *const NsBox,
) -> *mut c_void {
    box_ref(b)
        .and_then(inline::build_inline_layout)
        .map_or(core::ptr::null_mut(), Layout::into_raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_sync_inline_atomic_offsets(root: *mut NsBox) {
    if let Some(root) = box_ref(root) {
        inline::sync_inline_atomic_offsets(root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_inline_xy_to_byte(
    b: *const NsBox,
    rel_x: c_double,
    rel_y: c_double,
    out_byte: *mut usize,
) -> GBoolean {
    let Some(byte) = box_ref(b).and_then(|b| inline::inline_xy_to_byte(b, rel_x, rel_y)) else {
        return 0;
    };
    if let Some(out) = unsafe { out_byte.as_mut() } {
        *out = byte;
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_inline_word_range(
    b: *const NsBox,
    byte: usize,
    out_start: *mut usize,
    out_end: *mut usize,
) -> GBoolean {
    let Some((s, e)) = box_ref(b).and_then(|b| inline::inline_word_range(b, byte)) else {
        return 0;
    };
    if let Some(out) = unsafe { out_start.as_mut() } {
        *out = s;
    }
    if let Some(out) = unsafe { out_end.as_mut() } {
        *out = e;
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_inline_range_extents(
    b: *const NsBox,
    start: usize,
    len: usize,
    element: *const InlineAttr,
    out_x: *mut c_double,
    out_y: *mut c_double,
    out_w: *mut c_double,
    out_h: *mut c_double,
) -> GBoolean {
    let element = unsafe { element.as_ref() };
    let Some((x, y, w, h)) =
        box_ref(b).and_then(|b| inline::inline_range_extents(b, start, len, element))
    else {
        return 0;
    };
    for (out, v) in [(out_x, x), (out_y, y), (out_w, w), (out_h, h)] {
        if let Some(out) = unsafe { out.as_mut() } {
            *out = v;
        }
    }
    1
}
