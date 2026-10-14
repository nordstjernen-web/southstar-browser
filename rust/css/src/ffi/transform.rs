//! Southstar — the C ABI of parsing transforms and the translate, rotate and scale properties, and the math-function check.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::value::{self, NsCssValue};
use crate::transform::{self, Transform};

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn owned(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
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
pub unsafe extern "C" fn ns_css_transform_canonical(value: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(value) }.and_then(transform::transform_canonical))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_is_math_fn_start(s: *const c_char) -> GBoolean {
    glib::boolean(unsafe { bytes(s) }.is_some_and(transform::is_math_fn_start))
}
