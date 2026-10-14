//! Southstar — the DOM, form and URL calls the page lifecycle makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use southstar_dom::{self as dom, Node, NsNode};
use southstar_glib::{GBoolean, GStr};

use super::glib::{GString, StrBufOwned, gstr_from};

unsafe extern "C" {
    pub fn ns_node_free(n: *mut NsNode);
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
    n.element_name() == Some(tag.to_bytes())
}

pub fn next_in_subtree<'a>(n: Node<'a>, root: Node<'a>) -> Option<Node<'a>> {
    southstar_dom::index::next_in_subtree(n, Some(root), true)
}

pub fn hidden_until_found(n: Node<'_>) -> bool {
    dom::tree::hidden_until_found(n)
}

pub fn remove_attr(n: Node<'_>, name: &CStr) {
    southstar_dom::attrs::remove_attribute(n, name);
}

pub fn set_attr(n: Node<'_>, name: &CStr, value: &CStr) {
    southstar_dom::attrs::set_attribute(n, name, Some(value.to_bytes()));
}

pub fn details_fragment_needs_open(details: Node<'_>, child: Node<'_>) -> bool {
    dom::tree::details_fragment_needs_open(details, child)
}

pub fn find_fragment_target<'a>(doc: Option<Node<'a>>, frag: &CStr) -> Option<Node<'a>> {
    doc.and_then(|doc| dom::index::find_fragment_target(doc, frag))
}

pub fn find_first_element<'a>(root: Node<'a>, tag: &CStr) -> Option<Node<'a>> {
    southstar_dom::index::find_first_element(root, tag)
}

pub fn collect_text(n: Node<'_>) -> Vec<u8> {
    let mut text = dom::serialize::collect_text(Some(n));
    text.truncate(text.iter().position(|&b| b == 0).unwrap_or(text.len()));
    text
}

pub fn node_dump(doc: Node<'_>) -> *mut c_char {
    southstar_glib::strdup(&dom::serialize::dump(Some(doc)))
}

pub unsafe fn node_free(n: *mut NsNode) {
    unsafe { ns_node_free(n) };
}

pub fn editable_value(n: Node<'_>) -> Option<&CStr> {
    Some(southstar_dom::controls::editable_value(n))
}

pub fn is_text_input(n: Node<'_>) -> bool {
    dom::controls::is_text_input(n)
}

pub fn form_owner(control: Node<'_>) -> Option<Node<'_>> {
    southstar_dom::controls::form_owner(control)
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
    southstar_url::bytes::resolve(base.map(CStr::to_bytes), href.to_bytes())
        .and_then(|abs| gstr_from(&abs))
}

pub fn url_origin_from(url: Option<&CStr>) -> Option<Vec<u8>> {
    url.and_then(|url| southstar_url::bytes::origin_from(url.to_bytes()))
}

pub fn parse_refresh(input: &CStr) -> Option<(f64, Option<GStr>)> {
    let mut seconds = 0.0;
    let mut target: *mut c_char = ptr::null_mut();
    let ok = unsafe { ns_net_parse_refresh(input.as_ptr(), &mut seconds, &mut target) } != 0;
    let target = unsafe { GStr::take(target) };
    ok.then_some((seconds, target))
}
