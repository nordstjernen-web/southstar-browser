//! Southstar — the DOM, form and URL calls the page lifecycle makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{GBoolean, GStr};

use super::glib::{GString, StrBufOwned};

unsafe extern "C" {
    fn ns_node_is_element_named(n: *const NsNode, tag: *const c_char) -> GBoolean;
    fn ns_node_next_in_subtree(
        n: *const NsNode,
        root: *const NsNode,
        descend: GBoolean,
    ) -> *mut NsNode;
    fn ns_element_hidden_until_found(el: *const NsNode) -> GBoolean;
    fn ns_element_remove_attr(el: *mut NsNode, name: *const c_char);
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_details_fragment_needs_open(details: *const NsNode, child: *const NsNode) -> GBoolean;
    fn ns_node_find_fragment_target(root: *const NsNode, frag: *const c_char) -> *mut NsNode;
    fn ns_node_find_first_element(root: *const NsNode, tag: *const c_char) -> *mut NsNode;
    fn ns_node_collect_text(root: *const NsNode) -> *mut c_char;
    fn ns_node_dump(n: *const NsNode) -> *mut GString;
    pub fn ns_node_free(n: *mut NsNode);
    fn ns_node_editable_value(n: *const NsNode) -> *const c_char;
    fn ns_node_is_text_input(n: *const NsNode) -> GBoolean;
    fn ns_form_owner(control: *const NsNode, doc: *const NsNode) -> *const NsNode;
    fn ns_form_first_invalid(
        form: *const NsNode,
        n: *const NsNode,
        doc: *const NsNode,
    ) -> *const NsNode;
    fn ns_form_is_submit_trigger(n: *const NsNode) -> GBoolean;
    fn ns_form_collect_inputs(
        form: *const NsNode,
        n: *const NsNode,
        doc: *const NsNode,
        query: *mut GString,
        first: *mut GBoolean,
        submitter: *const NsNode,
    );
    fn ns_form_set_submission_charset(charset: *const c_char);
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_net_parse_refresh(
        input: *const c_char,
        time_out: *mut f64,
        url_out: *mut *mut c_char,
    ) -> GBoolean;
}

fn opt(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

pub fn is_named(n: Node<'_>, tag: &CStr) -> bool {
    unsafe { ns_node_is_element_named(n.as_ptr(), tag.as_ptr()) != 0 }
}

pub fn next_in_subtree<'a>(n: Node<'a>, root: Node<'a>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_node_next_in_subtree(n.as_ptr(), root.as_ptr(), 1)) }
}

pub fn hidden_until_found(n: Node<'_>) -> bool {
    unsafe { ns_element_hidden_until_found(n.as_ptr()) != 0 }
}

pub fn remove_attr(n: Node<'_>, name: &CStr) {
    unsafe { ns_element_remove_attr(n.as_mut_ptr(), name.as_ptr()) };
}

pub fn set_attr(n: Node<'_>, name: &CStr, value: &CStr) {
    unsafe { ns_element_set_attr(n.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub fn details_fragment_needs_open(details: Node<'_>, child: Node<'_>) -> bool {
    unsafe { ns_details_fragment_needs_open(details.as_ptr(), child.as_ptr()) != 0 }
}

pub fn find_fragment_target<'a>(doc: Option<Node<'a>>, frag: &CStr) -> Option<Node<'a>> {
    unsafe {
        Node::from_ptr(ns_node_find_fragment_target(
            Node::ptr_or_null(doc),
            frag.as_ptr(),
        ))
    }
}

pub fn find_first_element<'a>(root: Node<'a>, tag: &CStr) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_node_find_first_element(root.as_ptr(), tag.as_ptr())) }
}

pub fn collect_text(n: Node<'_>) -> Option<GStr> {
    unsafe { GStr::take(ns_node_collect_text(n.as_ptr())) }
}

pub fn node_dump(doc: Node<'_>) -> *mut c_char {
    let out = unsafe { ns_node_dump(doc.as_ptr()) };
    if out.is_null() {
        return ptr::null_mut();
    }
    unsafe { g_string_free_keep(out) }
}

unsafe fn g_string_free_keep(s: *mut GString) -> *mut c_char {
    unsafe extern "C" {
        fn g_string_free(s: *mut GString, free_segment: GBoolean) -> *mut c_char;
    }
    unsafe { g_string_free(s, 0) }
}

pub unsafe fn node_free(n: *mut NsNode) {
    unsafe { ns_node_free(n) };
}

pub fn editable_value(n: Node<'_>) -> Option<&CStr> {
    let v = unsafe { ns_node_editable_value(n.as_ptr()) };
    (!v.is_null()).then(|| unsafe { CStr::from_ptr(v) })
}

pub fn is_text_input(n: Node<'_>) -> bool {
    unsafe { ns_node_is_text_input(n.as_ptr()) != 0 }
}

pub fn form_owner<'a>(control: Node<'a>, doc: Node<'a>) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(ns_form_owner(control.as_ptr(), doc.as_ptr())) }
}

pub fn form_first_invalid<'a>(form: Node<'a>, doc: Node<'a>) -> Option<Node<'a>> {
    unsafe {
        Node::from_ptr(ns_form_first_invalid(
            form.as_ptr(),
            doc.as_ptr(),
            doc.as_ptr(),
        ))
    }
}

pub fn form_is_submit_trigger(n: Node<'_>) -> bool {
    unsafe { ns_form_is_submit_trigger(n.as_ptr()) != 0 }
}

pub fn form_collect_inputs(
    form: Node<'_>,
    doc: Node<'_>,
    out: &StrBufOwned,
    submitter: Option<Node<'_>>,
) {
    let mut first: GBoolean = 1;
    unsafe {
        ns_form_collect_inputs(
            form.as_ptr(),
            doc.as_ptr(),
            doc.as_ptr(),
            out.raw(),
            &mut first,
            Node::ptr_or_null(submitter),
        )
    };
}

pub fn set_submission_charset(charset: Option<&CStr>) {
    unsafe { ns_form_set_submission_charset(opt(charset)) };
}

pub fn url_resolve(base: Option<&CStr>, href: &CStr) -> Option<GStr> {
    unsafe { GStr::take(ns_url_resolve(opt(base), href.as_ptr())) }
}

pub fn url_origin_from(url: Option<&CStr>) -> Option<GStr> {
    unsafe { GStr::take(ns_url_origin_from(opt(url))) }
}

pub fn parse_refresh(input: &CStr) -> Option<(f64, Option<GStr>)> {
    let mut seconds = 0.0;
    let mut target: *mut c_char = ptr::null_mut();
    let ok = unsafe { ns_net_parse_refresh(input.as_ptr(), &mut seconds, &mut target) } != 0;
    let target = unsafe { GStr::take(target) };
    ok.then_some((seconds, target))
}
