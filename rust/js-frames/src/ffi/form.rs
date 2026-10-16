//! Southstar — the form helpers a frame-targeted submission calls: the urlencoded form data set and the page's CSP form-action check.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use super::{Js, NsJs};

#[repr(C)]
struct GString {
    str: *mut c_char,
    len: usize,
    allocated_len: usize,
}

unsafe extern "C" {
    fn g_string_new(init: *const c_char) -> *mut GString;
    fn g_string_free(string: *mut GString, free_segment: GBoolean) -> *mut c_char;
    fn ns_form_set_submission_charset(charset: *const c_char);
    fn ns_form_collect_inputs(
        form: *const NsNode,
        n: *const NsNode,
        doc: *const NsNode,
        query: *mut GString,
        first: *mut GBoolean,
        submitter: *const NsNode,
    );
    fn ns_js_csp_form_action_allowed(js: *mut NsJs, action_url: *const c_char) -> GBoolean;
}

pub(crate) fn urlencoded(
    form: Node<'_>,
    doc: Node<'_>,
    submitter: Option<Node<'_>>,
    charset: Option<&CStr>,
) -> Vec<u8> {
    // SAFETY: an empty initial string is permitted and the call returns a new GString.
    let query = unsafe { g_string_new(ptr::null()) };
    let mut first = glib::TRUE;
    // SAFETY: the charset is a NUL-terminated string or null, and the setting is copied.
    unsafe { ns_form_set_submission_charset(charset.map_or(ptr::null(), CStr::as_ptr)) };
    // SAFETY: the nodes are live, `query` is the GString created above and `first` outlives the call.
    unsafe {
        ns_form_collect_inputs(
            form.as_ptr(),
            doc.as_ptr(),
            doc.as_ptr(),
            query,
            &mut first,
            Node::ptr_or_null(submitter),
        )
    };
    // SAFETY: a null charset restores the default submission encoding.
    unsafe { ns_form_set_submission_charset(ptr::null()) };
    // SAFETY: `query` is the live GString created above.
    let string = unsafe { &*query };
    let bytes = if string.str.is_null() {
        Vec::new()
    } else {
        // SAFETY: a GString's `str` holds `len` initialized bytes.
        unsafe { core::slice::from_raw_parts(string.str.cast::<u8>(), string.len) }.to_vec()
    };
    // SAFETY: `query` is owned here and freed once, together with its buffer.
    unsafe { g_string_free(query, glib::TRUE) };
    bytes
}

pub(crate) fn csp_form_action_allowed(js: Js, url: &CStr) -> bool {
    // SAFETY: `js` is the live page and `url` is NUL-terminated.
    unsafe { ns_js_csp_form_action_allowed(js.ptr(), url.as_ptr()) != 0 }
}
