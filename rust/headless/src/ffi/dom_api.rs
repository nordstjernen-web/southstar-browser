//! Southstar — the DOM, form, editing and image-map calls of the in-process run, and C's numeric parsing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GStr};

use super::engine::{GString, with_gstring};

unsafe extern "C" {
    fn ns_node_find_fragment_target(root: *const NsNode, frag: *const c_char) -> *mut NsNode;
    fn ns_element_hidden_until_found(el: *const NsNode) -> GBoolean;
    fn ns_details_fragment_needs_open(details: *const NsNode, target: *const NsNode) -> GBoolean;
    fn ns_node_root(n: *const NsNode) -> *const NsNode;
    fn ns_element_remove_attr(el: *mut NsNode, name: *const c_char);
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_form_set_submission_charset(charset: *const c_char);
    fn ns_form_collect_inputs(
        form: *const NsNode,
        n: *const NsNode,
        doc: *const NsNode,
        query: *mut GString,
        first: *mut GBoolean,
        submitter: *const NsNode,
    );
    fn ns_node_editable_value(n: *const NsNode) -> *const c_char;
    fn ns_node_is_numeric_input(n: *const NsNode) -> GBoolean;
    fn ns_numeric_filter_insert(
        insert: *const c_char,
        len: usize,
        out_len: *mut usize,
    ) -> *mut c_char;
    fn ns_form_control_length_limits_apply(n: *const NsNode) -> GBoolean;
    fn ns_node_set_editable_value(n: *mut NsNode, value: *const c_char);
    fn ns_element_effectively_disabled(el: *const NsNode) -> GBoolean;
    fn ns_form_owner(control: *const NsNode, doc: *const NsNode) -> *const NsNode;
    fn ns_form_first_invalid(
        form: *const NsNode,
        n: *const NsNode,
        doc: *const NsNode,
    ) -> *const NsNode;
    fn ns_node_is_element_named(n: *const NsNode, tag: *const c_char) -> GBoolean;
    fn ns_node_find_by_id(root: *const NsNode, id: *const c_char) -> *mut NsNode;
    fn ns_node_is_editable(n: *const NsNode) -> GBoolean;
    fn ns_node_flatten_editable(n: *mut NsNode);
    fn ns_form_is_submit_trigger(n: *const NsNode) -> GBoolean;
    fn ns_image_map_resolve(
        doc: *const NsNode,
        usemap: *const c_char,
        lx: f64,
        ly: f64,
        iw: f64,
        ih: f64,
        out_target: *mut *const c_char,
    ) -> *mut c_char;
    fn ns_input_is_checked(n: *const NsNode) -> GBoolean;
    fn ns_node_find_first_element(root: *const NsNode, tag: *const c_char) -> *mut NsNode;
    fn ns_node_is_contenteditable_host(n: *const NsNode) -> GBoolean;
    fn g_utf8_strlen(p: *const c_char, max: isize) -> c_long;
    fn atol(s: *const c_char) -> c_long;
    fn atoi(s: *const c_char) -> c_int;
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub fn fragment_target<'a>(root: Node<'a>, frag: &CStr) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_node_find_fragment_target(root.as_ptr(), frag.as_ptr())) }
}

pub fn hidden_until_found(el: Node) -> bool {
    unsafe { ns_element_hidden_until_found(el.as_ptr()) != 0 }
}

pub fn details_fragment_needs_open(details: Node, target: Node) -> bool {
    unsafe { ns_details_fragment_needs_open(details.as_ptr(), target.as_ptr()) != 0 }
}

pub fn root(n: Node<'_>) -> Option<Node<'_>> {
    unsafe { Node::from_ptr(ns_node_root(n.as_ptr())) }
}

pub fn remove_attr(el: Node, name: &CStr) {
    unsafe { ns_element_remove_attr(el.as_mut_ptr(), name.as_ptr()) };
}

pub fn set_attr(el: Node, name: &CStr, value: &CStr) {
    unsafe { ns_element_set_attr(el.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub fn set_submission_charset(charset: Option<&CStr>) {
    unsafe { ns_form_set_submission_charset(charset.map_or(ptr::null(), CStr::as_ptr)) };
}

pub fn collect_inputs(form: Node, root: Node, submitter: Option<Node>) -> Vec<u8> {
    let mut first: GBoolean = glib::TRUE;
    with_gstring(|q| unsafe {
        ns_form_collect_inputs(
            form.as_ptr(),
            root.as_ptr(),
            root.as_ptr(),
            q,
            &mut first,
            Node::ptr_or_null(submitter),
        )
    })
}

pub fn editable_value(n: Node) -> Vec<u8> {
    let v = unsafe { ns_node_editable_value(n.as_ptr()) };
    if v.is_null() {
        return Vec::new();
    }
    unsafe { CStr::from_ptr(v) }.to_bytes().to_vec()
}

pub fn has_editable_value(n: Node) -> bool {
    !unsafe { ns_node_editable_value(n.as_ptr()) }.is_null()
}

pub fn is_numeric_input(n: Node) -> bool {
    unsafe { ns_node_is_numeric_input(n.as_ptr()) != 0 }
}

pub fn numeric_filter_insert(insert: &[u8]) -> Vec<u8> {
    let mut out_len = 0usize;
    let filtered =
        unsafe { ns_numeric_filter_insert(insert.as_ptr().cast(), insert.len(), &mut out_len) };
    if filtered.is_null() {
        return Vec::new();
    }
    let bytes = unsafe { core::slice::from_raw_parts(filtered.cast::<u8>(), out_len) }.to_vec();
    unsafe { glib::g_free(filtered.cast()) };
    bytes
}

pub fn length_limits_apply(n: Node) -> bool {
    unsafe { ns_form_control_length_limits_apply(n.as_ptr()) != 0 }
}

pub fn set_editable_value(n: Node, value: &[u8]) {
    let value = cstring(value);
    unsafe { ns_node_set_editable_value(n.as_mut_ptr(), value.as_ptr()) };
}

pub fn effectively_disabled(el: Node) -> bool {
    unsafe { ns_element_effectively_disabled(el.as_ptr()) != 0 }
}

pub fn form_owner<'a>(control: Node<'a>, doc: Option<Node<'a>>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_form_owner(control.as_ptr(), Node::ptr_or_null(doc))) }
}

pub fn first_invalid<'a>(form: Node<'a>, root: Node<'a>) -> Option<Node<'a>> {
    unsafe {
        Node::from_ptr(ns_form_first_invalid(
            form.as_ptr(),
            root.as_ptr(),
            root.as_ptr(),
        ))
    }
}

pub fn is_named(n: Node, tag: &CStr) -> bool {
    unsafe { ns_node_is_element_named(n.as_ptr(), tag.as_ptr()) != 0 }
}

pub fn find_by_id<'a>(root: Node<'a>, id: &CStr) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_node_find_by_id(root.as_ptr(), id.as_ptr())) }
}

pub fn is_editable(n: Node) -> bool {
    unsafe { ns_node_is_editable(n.as_ptr()) != 0 }
}

pub fn flatten_editable(n: Node) {
    unsafe { ns_node_flatten_editable(n.as_mut_ptr()) };
}

pub fn is_submit_trigger(n: Node) -> bool {
    unsafe { ns_form_is_submit_trigger(n.as_ptr()) != 0 }
}

pub fn image_map_resolve(
    doc: Node,
    usemap: &CStr,
    (lx, ly): (f64, f64),
    (iw, ih): (f64, f64),
) -> Option<Vec<u8>> {
    let href = unsafe {
        ns_image_map_resolve(
            doc.as_ptr(),
            usemap.as_ptr(),
            lx,
            ly,
            iw,
            ih,
            ptr::null_mut(),
        )
    };
    unsafe { GStr::take(href) }.map(|h| h.to_bytes().to_vec())
}

pub fn input_is_checked(n: Node) -> bool {
    unsafe { ns_input_is_checked(n.as_ptr()) != 0 }
}

pub fn first_element<'a>(root: Node<'a>, tag: &CStr) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_node_find_first_element(root.as_ptr(), tag.as_ptr())) }
}

pub fn is_contenteditable_host(n: Node) -> bool {
    unsafe { ns_node_is_contenteditable_host(n.as_ptr()) != 0 }
}

pub fn utf8_strlen(s: &[u8]) -> c_long {
    let c = cstring(s);
    unsafe { g_utf8_strlen(c.as_ptr(), c.as_bytes().len() as isize) }
}

pub fn c_atol(s: &CStr) -> c_long {
    unsafe { atol(s.as_ptr()) }
}

pub fn c_atoi(s: &CStr) -> c_int {
    unsafe { atoi(s.as_ptr()) }
}
