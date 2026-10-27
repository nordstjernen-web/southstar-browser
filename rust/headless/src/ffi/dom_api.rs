//! Southstar — the DOM, form, editing and image-map calls of the in-process run, and C-style leading-integer parsing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long};
use std::ffi::CString;

use southstar_dom::{self as dom, Node};

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub fn fragment_target<'a>(root: Node<'a>, frag: &CStr) -> Option<Node<'a>> {
    dom::index::find_fragment_target(root, frag)
}

pub fn hidden_until_found(el: Node) -> bool {
    dom::tree::hidden_until_found(el)
}

pub fn details_fragment_needs_open(details: Node, target: Node) -> bool {
    dom::tree::details_fragment_needs_open(details, target)
}

pub fn root(n: Node<'_>) -> Option<Node<'_>> {
    Some(dom::tree::root(n))
}

pub fn remove_attr(el: Node, name: &CStr) {
    southstar_dom::attrs::remove_attribute(el, name);
}

pub fn set_attr(el: Node, name: &CStr, value: &CStr) {
    southstar_dom::attrs::set_attribute(el, name, Some(value.to_bytes()));
}

pub fn set_submission_charset(charset: Option<&CStr>) {
    southstar_net::set_submission_charset(charset.map(CStr::to_bytes));
}

pub fn editable_value(n: Node) -> Vec<u8> {
    southstar_dom::controls::editable_value(n)
        .to_bytes()
        .to_vec()
}

pub fn is_numeric_input(n: Node) -> bool {
    dom::controls::is_numeric_input(n)
}

pub fn numeric_filter_insert(insert: &[u8]) -> Vec<u8> {
    dom::controls::numeric_filter(insert)
}

pub fn length_limits_apply(n: Node) -> bool {
    dom::controls::length_limits_apply(n)
}

pub fn set_editable_value(n: Node, value: &[u8]) {
    dom::controls::set_editable_value(n, &cstring(value));
}

pub fn effectively_disabled(el: Node) -> bool {
    southstar_dom::controls::effectively_disabled(el)
}

pub fn form_owner(control: Node<'_>) -> Option<Node<'_>> {
    southstar_dom::controls::form_owner(control)
}

pub fn is_named(n: Node, tag: &CStr) -> bool {
    n.element_name() == Some(tag.to_bytes())
}

pub fn find_by_id<'a>(root: Node<'a>, id: &CStr) -> Option<Node<'a>> {
    dom::index::find_by_id(root, id)
}

pub fn is_editable(n: Node) -> bool {
    dom::controls::is_editable(n)
}

pub fn flatten_editable(n: Node) {
    dom::controls::flatten_editable(n);
}

pub fn image_map_resolve(
    doc: Node,
    usemap: &CStr,
    (lx, ly): (f64, f64),
    (iw, ih): (f64, f64),
) -> Option<Vec<u8>> {
    dom::image_map::resolve(doc, usemap, (lx, ly), (iw, ih))
        .map(|(href, _)| href.to_bytes().to_vec())
}

pub fn input_is_checked(n: Node) -> bool {
    dom::controls::is_checked(n)
}

pub fn first_element<'a>(root: Node<'a>, tag: &CStr) -> Option<Node<'a>> {
    southstar_dom::index::find_first_element(root, tag)
}

pub fn is_contenteditable_host(n: Node) -> bool {
    dom::controls::is_contenteditable_host(n)
}

pub fn utf8_strlen(s: &[u8]) -> c_long {
    let c = cstring(s);
    let chars = c.as_bytes().iter().filter(|&&b| b & 0xc0 != 0x80).count();
    c_long::try_from(chars).unwrap_or(c_long::MAX)
}

fn leading_integer(s: &CStr) -> i64 {
    let bytes = s.to_bytes();
    let start = bytes
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .unwrap_or(bytes.len());
    let (negative, digits) = match bytes.get(start) {
        Some(b'-') => (true, &bytes[start + 1..]),
        Some(b'+') => (false, &bytes[start + 1..]),
        _ => (false, &bytes[start..]),
    };
    let magnitude = digits
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0i64, |n, d| {
            n.saturating_mul(10).saturating_add(i64::from(d - b'0'))
        });
    if negative {
        magnitude.saturating_neg()
    } else {
        magnitude
    }
}

pub fn c_atol(s: &CStr) -> c_long {
    leading_integer(s).clamp(c_long::MIN.into(), c_long::MAX.into()) as c_long
}

pub fn c_atoi(s: &CStr) -> c_int {
    leading_integer(s).clamp(c_int::MIN.into(), c_int::MAX.into()) as c_int
}
