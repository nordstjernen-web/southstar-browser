//! Southstar — the C ABI of the HTML parser for the crates that still reach it by symbol.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_html5::Ns;

use crate::sink::{parse_document, parse_fragment, parse_fragment_in_namespace};

/// # Safety
///
/// `input` points at `len` readable bytes, or at a NUL-terminated string when `len` is negative.
unsafe fn input_bytes<'a>(input: *const c_char, len: isize) -> &'a [u8] {
    if len < 0 {
        // SAFETY: guaranteed by the caller.
        unsafe { CStr::from_ptr(input) }.to_bytes()
    } else {
        // SAFETY: guaranteed by the caller.
        unsafe { glib::slice(input.cast(), len as usize) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse_with_scripting(
    input: *const c_char,
    len: isize,
    scripting: GBoolean,
) -> *mut NsNode {
    if input.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C callers pass `len` readable bytes, or a NUL-terminated string when `len` is negative.
    let bytes = unsafe { input_bytes(input, len) };
    parse_document(bytes, scripting != 0).into_raw()
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    // SAFETY: the C callers pass null or a NUL-terminated string that outlives the call.
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse_fragment_with_scripting(
    context_tag: *const c_char,
    input: *const c_char,
    len: isize,
    scripting: GBoolean,
) -> *mut NsNode {
    if input.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C callers pass `len` readable bytes, or a NUL-terminated string when `len` is negative.
    let bytes = unsafe { input_bytes(input, len) };
    parse_fragment_in_namespace(c_str(context_tag), Ns::Html, bytes, scripting != 0).into_raw()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse_fragment_in_context(
    context: *const NsNode,
    input: *const c_char,
    len: isize,
    scripting: GBoolean,
) -> *mut NsNode {
    if input.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C callers pass null or a node of this thread's store.
    let context = unsafe { Node::from_ptr(context) };
    // SAFETY: the C callers pass `len` readable bytes, or a NUL-terminated string when `len` is negative.
    let bytes = unsafe { input_bytes(input, len) };
    parse_fragment(context, bytes, scripting != 0).into_raw()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_convert_declarative_shadow(root: *mut NsNode) {
    crate::convert_declarative_shadow(unsafe { Node::from_ptr(root) }, 0);
}
