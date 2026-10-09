//! Southstar — the C ABI of box-shadow and text-shadow values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::value::{self, NsCssValue};
use crate::shadow::{self, ShadowList};

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_box_shadow(text: *const c_char) -> *mut NsCssValue {
    unsafe { bytes(text) }
        .and_then(shadow::parse_list)
        .map_or(ptr::null_mut(), |list| value::new_shadow(&list))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_shadow_specified_canonical(
    text: *const c_char,
    is_text: GBoolean,
) -> *mut c_char {
    unsafe { bytes(text) }
        .and_then(|t| shadow::specified_canonical(t, is_text != 0))
        .map_or(ptr::null_mut(), |canon| glib::strdup(&canon))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_shadow_serialize(list: *const ShadowList) -> *mut c_char {
    let text = unsafe { list.as_ref() }
        .map(shadow::serialize)
        .unwrap_or_default();
    glib::strdup(&text)
}
