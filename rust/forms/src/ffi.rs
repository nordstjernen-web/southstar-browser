//! Southstar — the C ABI of the form helpers, as declared in src/forms.h, over the ns_node layout and the control functions of src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_long, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GError};

unsafe extern "C" {
    fn ns_url_is_valid_absolute(url: *const c_char) -> GBoolean;
    fn ns_form_urlencoded_append_pair(
        out: *mut c_void,
        first: *mut GBoolean,
        name: *const c_char,
        value: *const c_char,
    );
    fn g_utf8_strlen(p: *const c_char, max: isize) -> c_long;
    fn g_regex_new(
        pattern: *const c_char,
        compile_options: c_uint,
        match_options: c_uint,
        error: *mut *mut GError,
    ) -> *mut c_void;
    fn g_regex_match(
        regex: *const c_void,
        string: *const c_char,
        match_options: c_uint,
        match_info: *mut *mut c_void,
    ) -> GBoolean;
    fn g_regex_unref(regex: *mut c_void);
}

fn opt_ptr(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

pub struct Query {
    out: *mut c_void,
    first: *mut GBoolean,
}

impl Query {
    pub fn append(&mut self, name: &CStr, value: Option<&CStr>) {
        unsafe {
            ns_form_urlencoded_append_pair(self.out, self.first, name.as_ptr(), opt_ptr(value))
        };
    }
}

pub fn url_is_valid_absolute(url: &CStr) -> bool {
    unsafe { ns_url_is_valid_absolute(url.as_ptr()) != 0 }
}

pub fn utf8_strlen(s: &CStr) -> c_long {
    unsafe { g_utf8_strlen(s.as_ptr(), -1) }
}

pub fn regex_matches(pattern: &CStr, value: &CStr) -> Option<bool> {
    let mut error = ptr::null_mut();
    let regex = unsafe { g_regex_new(pattern.as_ptr(), 0, 0, &mut error) };
    if regex.is_null() {
        if !error.is_null() {
            unsafe { glib::g_error_free(error) };
        }
        return None;
    }
    let matched = unsafe { g_regex_match(regex, value.as_ptr(), 0, ptr::null_mut()) != 0 };
    unsafe { g_regex_unref(regex) };
    Some(matched)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_is_submit_trigger(n: *const NsNode) -> GBoolean {
    let node = unsafe { Node::from_ptr(n) };
    glib::boolean(node.is_some_and(crate::is_submit_trigger))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_is_reset_trigger(n: *const NsNode) -> GBoolean {
    let node = unsafe { Node::from_ptr(n) };
    glib::boolean(node.is_some_and(crate::is_reset_trigger))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_collect_inputs(
    form: *const NsNode,
    n: *const NsNode,
    _doc: *const NsNode,
    query: *mut c_void,
    first: *mut GBoolean,
    submitter: *const NsNode,
) {
    let mut query = Query { out: query, first };
    unsafe {
        crate::collect_inputs(
            Node::from_ptr(form),
            Node::from_ptr(n),
            &mut query,
            Node::from_ptr(submitter),
            0,
        )
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_first_invalid(
    form: *const NsNode,
    n: *const NsNode,
    doc: *const NsNode,
) -> *const NsNode {
    let invalid = unsafe {
        crate::first_invalid(
            Node::from_ptr(form),
            Node::from_ptr(n),
            Node::from_ptr(doc),
            0,
        )
    };
    Node::ptr_or_null(invalid)
}
