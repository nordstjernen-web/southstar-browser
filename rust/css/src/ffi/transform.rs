//! Southstar — the C ABI of transforms, the translate, rotate and scale properties, and the math-function check.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int};
use core::ptr;
use std::sync::OnceLock;

use southstar_glib::{self as glib, GBoolean};
use southstar_mat4::Mat4;

use super::value::{self, NsCssValue};
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
pub unsafe extern "C" fn ns_css_is_math_fn_start(s: *const c_char) -> GBoolean {
    glib::boolean(unsafe { bytes(s) }.is_some_and(transform::is_math_fn_start))
}
