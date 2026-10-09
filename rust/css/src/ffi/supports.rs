//! Southstar — the C ABI of @supports conditions and CSS.supports().
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use southstar_glib::{self as glib, GBoolean};

use crate::supports;

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_supports_declaration(
    property: *const c_char,
    value: *const c_char,
) -> GBoolean {
    let (Some(property), Some(value)) = (unsafe { bytes(property) }, unsafe { bytes(value) })
    else {
        return glib::FALSE;
    };
    glib::boolean(supports::declaration(property, value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_supports_condition(
    condition: *const c_char,
    allow_bare_declaration: GBoolean,
) -> GBoolean {
    let Some(condition) = (unsafe { bytes(condition) }) else {
        return glib::FALSE;
    };
    glib::boolean(supports::condition(condition, allow_bare_declaration != 0))
}
