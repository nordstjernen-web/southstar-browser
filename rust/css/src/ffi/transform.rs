//! Southstar — the C ABI of transforms and of the specified-value text clean-ups.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_void};
use core::ptr;
use std::sync::OnceLock;

use southstar_glib::{self as glib, GBoolean, GPtrArray};
use southstar_mat4::Mat4;

use super::value::{self, NsCssValue};
use crate::text;
use crate::transform::{self, Individual, Transform};

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn owned(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
}

fn individual(prop: c_int) -> Option<Individual> {
    static IDS: OnceLock<[c_int; 3]> = OnceLock::new();
    let ids = IDS.get_or_init(|| unsafe {
        [
            ns_css_prop_id(c"translate".as_ptr()),
            ns_css_prop_id(c"rotate".as_ptr()),
            ns_css_prop_id(c"scale".as_ptr()),
        ]
    });
    [Individual::Translate, Individual::Rotate, Individual::Scale]
        .into_iter()
        .zip(ids)
        .find(|&(_, &id)| id == prop)
        .map(|(kind, _)| kind)
}

fn transform_value(tf: Option<Transform>) -> *mut NsCssValue {
    tf.map_or(ptr::null_mut(), |tf| value::new_transform(&tf))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_transform(text: *const c_char) -> *mut NsCssValue {
    transform_value(unsafe { bytes(text) }.and_then(transform::parse_transform))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_transform_origin(text: *const c_char) -> *mut NsCssValue {
    transform_value(unsafe { bytes(text) }.and_then(transform::parse_transform_origin))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_translate_prop(text: *const c_char) -> *mut NsCssValue {
    transform_value(unsafe { bytes(text) }.and_then(transform::parse_translate_prop))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_rotate_prop(text: *const c_char) -> *mut NsCssValue {
    transform_value(unsafe { bytes(text) }.and_then(transform::parse_rotate_prop))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_scale_prop(text: *const c_char) -> *mut NsCssValue {
    transform_value(unsafe { bytes(text) }.and_then(transform::parse_scale_prop))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_individual_transform_serialize(
    v: *const NsCssValue,
    prop: c_int,
) -> *mut c_char {
    let Some(v) = (unsafe { v.as_ref() }) else {
        return ptr::null_mut();
    };
    if v.kind == value::KIND_KEYWORD {
        return unsafe { glib::g_strdup(v.u.keyword) };
    }
    if v.kind != value::KIND_TRANSFORM {
        return ptr::null_mut();
    }
    let tf = unsafe { &v.u.transform };
    owned(individual(prop).and_then(|prop| transform::individual_serialize(tf, prop)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_transform_serialize(tf: *const Transform) -> *mut c_char {
    let text = unsafe { tf.as_ref() }
        .map(transform::serialize)
        .unwrap_or_default();
    glib::strdup(&text)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_transform_is_3d(tf: *const Transform) -> GBoolean {
    glib::boolean(unsafe { tf.as_ref() }.is_some_and(transform::is_3d))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_transform_to_mat4(
    tf: *const Transform,
    bw: c_double,
    bh: c_double,
    out: *mut Mat4,
) {
    let matrix = match unsafe { tf.as_ref() } {
        Some(tf) => transform::to_mat4(tf, bw, bh),
        None => Mat4::IDENTITY,
    };
    unsafe { *out = matrix };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_transform_canonical(value: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(value) }.and_then(transform::transform_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_transform_list_canonical(value: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(value) }.and_then(transform::list_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_individual_transform_canonical(
    value: *const c_char,
    prop: c_int,
) -> *mut c_char {
    let prop = individual(prop);
    owned(unsafe { bytes(value) }.and_then(|value| transform::individual_canonical(value, prop?)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_transform_origin_canonical(
    value: *const c_char,
    two_only: GBoolean,
) -> *mut c_char {
    owned(
        unsafe { bytes(value) }.and_then(|value| transform::origin_canonical(value, two_only != 0)),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_is_math_fn_start(s: *const c_char) -> GBoolean {
    glib::boolean(unsafe { bytes(s) }.is_some_and(transform::is_math_fn_start))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_add_leading_zeros(v: *mut c_char) -> *mut c_char {
    let Some(text) = (unsafe { bytes(v) }) else {
        return ptr::null_mut();
    };
    let fixed = text::add_leading_zeros(text);
    if fixed == text {
        return v;
    }
    unsafe { glib::g_free(v.cast()) };
    glib::strdup(&fixed)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_normalize_negative_zero(v: *mut c_char) -> *mut c_char {
    let Some(text) = (unsafe { bytes(v) }) else {
        return ptr::null_mut();
    };
    let fixed = text::normalize_negative_zero(text);
    if fixed == text {
        return v;
    }
    unsafe { glib::g_free(v.cast()) };
    glib::strdup(&fixed)
}

unsafe extern "C" fn free_string(data: *mut c_void) {
    unsafe { glib::g_free(data) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_split_top_level_commas(text: *const c_char) -> *mut GPtrArray {
    let parts = text::split_top_level_commas(unsafe { bytes(text) }.unwrap_or_default());
    let array = unsafe { glib::g_ptr_array_new_with_free_func(Some(free_string)) };
    for part in parts {
        unsafe { glib::g_ptr_array_add(array, glib::strdup(&part).cast()) };
    }
    array
}
