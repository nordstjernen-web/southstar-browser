//! Southstar — the C ABI of computed-style queries: css.c's ns_style read through the style mirror for layout, paint and script — writing mode, lengths in px, columns, keywords and currentcolor flags.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::computed_units::{RawStyle, StyleView, slot};
use super::value::{KIND_KEYWORD, NsCssValue};
use crate::cascade::CURRENTCOLOR_PROPS;
use crate::display::Display;
use crate::prop::Prop;
use crate::style_query;

unsafe fn view<'a>(style: *const RawStyle) -> Option<StyleView<'a>> {
    unsafe { style.as_ref() }.map(StyleView)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_writing_mode(style: *const RawStyle) -> c_int {
    style_query::writing_mode(unsafe { view(style) }.as_ref())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_text_orientation(style: *const RawStyle) -> c_int {
    style_query::text_orientation(unsafe { view(style) }.as_ref())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_value_layer_count(head: *const NsCssValue) -> c_int {
    let mut n = 0;
    let mut layer = head;
    while let Some(v) = unsafe { layer.as_ref() } {
        n += 1;
        layer = v.next_layer;
    }
    n
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_value_layer(
    head: *const NsCssValue,
    index: c_int,
) -> *const NsCssValue {
    let n = unsafe { ns_css_value_layer_count(head) };
    if n == 0 {
        return ptr::null();
    }
    let mut index = index % n;
    let mut layer = head;
    while index > 0 {
        layer = unsafe { (*layer).next_layer };
        index -= 1;
    }
    layer
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_dimension_px(
    v: *const NsCssValue,
    font_size: c_double,
    basis: c_double,
) -> c_double {
    style_query::dimension_px(unsafe { v.as_ref() }.map(slot), font_size, basis)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_used_column_count(
    style: *const RawStyle,
    avail_w: c_double,
    out_gap: *mut c_double,
) -> c_int {
    let (n, gap) = style_query::used_column_count(unsafe { view(style) }.as_ref(), avail_w);
    if let Some(out) = unsafe { out_gap.as_mut() } {
        *out = gap;
    }
    n
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_keyword_is(v: *const NsCssValue, kw: *const c_char) -> GBoolean {
    let Some(v) = (unsafe { v.as_ref() }).filter(|v| v.kind == KIND_KEYWORD) else {
        return glib::FALSE;
    };
    let keyword = unsafe { v.u.keyword };
    if kw.is_null() || keyword.is_null() {
        return glib::FALSE;
    }
    glib::boolean(unsafe { CStr::from_ptr(keyword) == CStr::from_ptr(kw) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_display_of(style: *const RawStyle) -> Display {
    unsafe { style.as_ref() }.map_or_else(Display::default, |s| s.display)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_alignment_base(kw: *const c_char) -> *const c_char {
    if kw.is_null() {
        return ptr::null();
    }
    style_query::alignment_base(unsafe { CStr::from_ptr(kw) }).as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_style_keyword(style: *const RawStyle, prop: c_int) -> *const c_char {
    let (Some(view), Some(prop)) = (
        unsafe { view(style) },
        usize::try_from(prop).ok().and_then(Prop::from_id),
    ) else {
        return ptr::null();
    };
    style_query::style_keyword(&view, prop).map_or(ptr::null(), CStr::as_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_style_overflow_keyword(
    style: *const RawStyle,
    axis: c_int,
) -> *const c_char {
    let axis = usize::try_from(axis)
        .ok()
        .and_then(Prop::from_id)
        .unwrap_or(Prop::OverflowX);
    let view = unsafe { view(style) };
    style_query::overflow_keyword(view.as_ref(), axis).as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_style_prop_from_currentcolor(
    style: *const RawStyle,
    prop: c_int,
) -> GBoolean {
    let Some(style) = (unsafe { style.as_ref() }) else {
        return glib::FALSE;
    };
    let bit = CURRENTCOLOR_PROPS
        .iter()
        .position(|p| p.id() as c_int == prop);
    glib::boolean(bit.is_some_and(|i| (style.currentcolor_bits >> i) & 1 != 0))
}
