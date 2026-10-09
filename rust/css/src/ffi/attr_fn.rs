//! Southstar — the C ABI of attr() substitution: a pending declaration's text with its attr() calls replaced for one element, and whether attribute text reached a URL.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use crate::attr_fn;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_substitute_attrs(
    text: *const c_char,
    node: *const NsNode,
    tainted: *mut GBoolean,
) -> *mut c_char {
    if text.is_null() {
        return ptr::null_mut();
    }
    let text = unsafe { CStr::from_ptr(text) }.to_bytes();
    let mut taint = false;
    let result = attr_fn::substitute(text, unsafe { Node::from_ptr(node) }, 0, &mut taint);
    if taint && !tainted.is_null() {
        unsafe { *tainted = glib::TRUE };
    }
    result.map_or(ptr::null_mut(), |out| glib::strdup(&out))
}
