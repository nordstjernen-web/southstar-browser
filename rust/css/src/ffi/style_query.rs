//! Southstar — the C ABI of computed-style queries: css.c's ns_style read through the style mirror for layout, paint and script — writing mode, lengths in px, columns, keywords, the effective transform and currentcolor flags.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::computed_units::{RawStyle, StyleView, slot};
use super::value::{KIND_GRADIENT, KIND_KEYWORD, KIND_TRANSFORM, KIND_URL, NsCssValue};
use crate::cascade::CURRENTCOLOR_PROPS;
use crate::display::Display;
use crate::prop::Prop;
use crate::style_query;
use crate::transform::{OPS_MAX, Transform};

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

fn transform_of(v: &NsCssValue) -> Option<&Transform> {
    if v.kind == KIND_TRANSFORM {
        Some(unsafe { &v.u.transform })
    } else {
        None
    }
}

unsafe fn push_op(out: &mut Transform, op: &crate::transform::Op) {
    let at = out.n_ops as usize;
    unsafe { ptr::copy_nonoverlapping(op, &raw mut out.ops[at], 1) };
    out.n_ops += 1;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_style_effective_transform(
    style: *const RawStyle,
    transform_override: *const Transform,
    out: *mut Transform,
) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    unsafe { ptr::write_bytes(&raw mut *out, 0, 1) };
    let style = unsafe { style.as_ref() };
    let value = |prop: Prop| style.and_then(|s| unsafe { s.value(prop.id()).as_ref() });
    for prop in [Prop::Translate, Prop::Rotate, Prop::Scale] {
        if let Some(tf) = value(prop).and_then(transform_of)
            && tf.n_ops > 0
            && (out.n_ops as usize) < OPS_MAX
        {
            unsafe { push_op(out, &tf.ops[0]) };
        }
    }
    let tf = unsafe { transform_override.as_ref() }
        .or_else(|| value(Prop::Transform).and_then(transform_of));
    if let Some(tf) = tf {
        let n = usize::try_from(tf.n_ops).unwrap_or(0).min(OPS_MAX);
        for op in &tf.ops[..n] {
            if out.n_ops as usize >= OPS_MAX {
                break;
            }
            unsafe { push_op(out, op) };
        }
    }
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
pub unsafe extern "C" fn ns_css_border_image_source(style: *const RawStyle) -> *const NsCssValue {
    let Some(style) = (unsafe { style.as_ref() }) else {
        return ptr::null();
    };
    let v = style.value(Prop::BorderImageSource.id());
    match unsafe { v.as_ref() } {
        Some(value) if value.kind == KIND_URL || value.kind == KIND_GRADIENT => v,
        _ => ptr::null(),
    }
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
