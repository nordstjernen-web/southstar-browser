//! Southstar — the C ABI of presentational hints: an element's attribute-derived declarations as text, and whether an attribute name is one that maps to style.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use crate::hints;

unsafe extern "C" {
    fn ns_image_supports_mime(mime: *const c_char) -> GBoolean;
}

pub(crate) fn image_supports_mime(mime: &CStr) -> bool {
    unsafe { ns_image_supports_mime(mime.as_ptr()) != 0 }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_presentational_hints(el: *const NsNode) -> *mut c_char {
    unsafe { Node::from_ptr(el) }
        .and_then(hints::presentational_hints)
        .map_or(ptr::null_mut(), |css| glib::strdup(&css))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_is_presentational_attr(name: *const c_char) -> GBoolean {
    if name.is_null() {
        return glib::FALSE;
    }
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    glib::boolean(hints::is_presentational_attr(name))
}
