//! Southstar — the C ABI of serializing a CSS identifier.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use southstar_glib as glib;

use crate::lex;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_ident_serialize(name: *const c_char) -> *mut c_char {
    if name.is_null() {
        return glib::strdup(b"");
    }
    glib::strdup(&lex::ident_serialize(
        unsafe { CStr::from_ptr(name) }.to_bytes(),
    ))
}
