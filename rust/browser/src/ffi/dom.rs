//! Southstar — the DOM, form and URL calls the page lifecycle makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{self as dom, Node};
use southstar_glib::GStr;

use super::glib::gstr_from;

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

pub fn node_dump(doc: Node<'_>) -> Vec<u8> {
    dom::serialize::dump(Some(doc))
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

pub fn set_submission_charset(charset: Option<&CStr>) {
    southstar_net::set_submission_charset(charset.map(CStr::to_bytes));
}

pub fn url_resolve(base: Option<&CStr>, href: &CStr) -> Option<GStr> {
    southstar_url::bytes::resolve(base.map(CStr::to_bytes), href.to_bytes())
        .and_then(|abs| gstr_from(&abs))
}

pub fn url_origin_from(url: Option<&CStr>) -> Option<Vec<u8>> {
    url.and_then(|url| southstar_url::bytes::origin_from(url.to_bytes()))
}

pub fn parse_refresh(input: &CStr) -> Option<(f64, Option<GStr>)> {
    let (seconds, target) = southstar_net::parse_refresh(input.to_bytes())?;
    Some((seconds, target.and_then(|target| gstr_from(&target))))
}
