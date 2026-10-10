//! Southstar — the Node tree getters, contains, hasChildNodes, isEqualNode and compareDocumentPosition.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::serialize::is_embedded_doc;
use southstar_dom::{FLAG_PI, Kind, MAX_DEPTH, children};

use crate::names::{local_name, namespace_uri, prefix};
use crate::{Element, FLAG_CDATA, FLAG_FRAGMENT, SHADOW_ATTR, is_document};

const DISCONNECTED: i32 = 0x01;
const PRECEDING: i32 = 0x02;
const FOLLOWING: i32 = 0x04;
const CONTAINS: i32 = 0x08;
const CONTAINED_BY: i32 = 0x10;
const IMPLEMENTATION_SPECIFIC: i32 = 0x20;

pub(crate) fn is_shadow_root(node: Element) -> bool {
    node.is_element() && node.attr(SHADOW_ATTR).is_some()
}

fn is_template(node: Element) -> bool {
    node.element_name() == Some(b"template")
}

fn hidden_child(node: Element) -> bool {
    is_embedded_doc(node) || is_shadow_root(node)
}

fn is_child_element(node: Element) -> bool {
    node.is_element() && !is_shadow_root(node)
}

fn tree_parent(node: Element) -> Option<Element> {
    if is_shadow_root(node) || is_embedded_doc(node) {
        return None;
    }
    node.parent()
}

fn skip_hidden(mut node: Option<Element>, step: fn(Element) -> Option<Element>) -> Option<Element> {
    while let Some(n) = node.filter(|&n| hidden_child(n)) {
        node = step(n);
    }
    node
}

fn find_element(
    mut node: Option<Element>,
    step: fn(Element) -> Option<Element>,
) -> Option<Element> {
    while let Some(n) = node {
        if is_child_element(n) {
            return Some(n);
        }
        node = step(n);
    }
    None
}

pub(crate) fn parent_element(node: Option<Element>) -> Option<Element> {
    let node = node.filter(|&n| n.kind() != Kind::Document && !is_shadow_root(n))?;
    node.parent().filter(|&p| is_child_element(p))
}

pub(crate) fn parent_node(node: Option<Element>) -> Option<Element> {
    let node = node.filter(|&n| n.kind() != Kind::Document && !is_shadow_root(n))?;
    node.parent()
}

pub(crate) fn first_child(node: Option<Element>) -> Option<Element> {
    let node = node.filter(|&n| !is_template(n))?;
    skip_hidden(node.first_child(), Element::next_sibling)
}

pub(crate) fn last_child(node: Option<Element>) -> Option<Element> {
    let node = node.filter(|&n| !is_template(n))?;
    skip_hidden(node.last_child(), Element::prev_sibling)
}

pub(crate) fn next_sibling(node: Option<Element>) -> Option<Element> {
    skip_hidden(node?.next_sibling(), Element::next_sibling)
}

pub(crate) fn previous_sibling(node: Option<Element>) -> Option<Element> {
    skip_hidden(node?.prev_sibling(), Element::prev_sibling)
}

pub(crate) fn first_element_child(node: Option<Element>) -> Option<Element> {
    let node = node.filter(|&n| !is_template(n))?;
    find_element(node.first_child(), Element::next_sibling)
}

pub(crate) fn last_element_child(node: Option<Element>) -> Option<Element> {
    let node = node.filter(|&n| !is_template(n))?;
    find_element(node.last_child(), Element::prev_sibling)
}

pub(crate) fn next_element_sibling(node: Option<Element>) -> Option<Element> {
    find_element(node?.next_sibling(), Element::next_sibling)
}

pub(crate) fn previous_element_sibling(node: Option<Element>) -> Option<Element> {
    find_element(node?.prev_sibling(), Element::prev_sibling)
}

pub(crate) fn child_element_count(node: Option<Element>) -> i32 {
    match node.filter(|&n| !is_template(n)) {
        Some(n) => children(n).filter(|&c| is_child_element(c)).count() as i32,
        None => 0,
    }
}

pub(crate) fn has_child_nodes(node: Option<Element>) -> bool {
    node.filter(|&n| !is_template(n))
        .is_some_and(|n| children(n).any(|c| !hidden_child(c)))
}

pub(crate) fn contains(node: Option<Element>, other: Option<Element>) -> bool {
    let (Some(node), Some(other)) = (node, other) else {
        return false;
    };
    core::iter::successors(Some(other), |&n| tree_parent(n)).any(|n| n == node)
}

pub(crate) fn is_connected(node: Option<Element>) -> bool {
    let Some(mut root) = node else {
        return false;
    };
    while let Some(parent) = root.parent() {
        root = parent;
    }
    is_document(root)
}

fn tree_depth(node: Element) -> usize {
    core::iter::successors(tree_parent(node), |&n| tree_parent(n)).count()
}

fn tree_ancestor(node: Element, other: Element) -> bool {
    core::iter::successors(tree_parent(node), |&n| tree_parent(n)).any(|n| n == other)
}

pub(crate) fn compare_document_position(a: Element, b: Option<Element>) -> i32 {
    let Some(b) = b else {
        return DISCONNECTED;
    };
    if a == b {
        return 0;
    }
    if tree_ancestor(a, b) {
        return PRECEDING | CONTAINS;
    }
    if tree_ancestor(b, a) {
        return FOLLOWING | CONTAINED_BY;
    }
    let (mut x, mut y) = (a, b);
    let (mut dx, mut dy) = (tree_depth(a), tree_depth(b));
    while dx > dy {
        x = tree_parent(x).unwrap_or(x);
        dx -= 1;
    }
    while dy > dx {
        y = tree_parent(y).unwrap_or(y);
        dy -= 1;
    }
    loop {
        match (tree_parent(x), tree_parent(y)) {
            (Some(px), Some(py)) if px == py => {
                for child in children(px) {
                    if child == x {
                        return FOLLOWING;
                    }
                    if child == y {
                        return PRECEDING;
                    }
                }
                break;
            }
            (Some(px), Some(py)) => {
                x = px;
                y = py;
            }
            _ => break,
        }
    }
    let order = if (a.as_ptr() as usize) < (b.as_ptr() as usize) {
        FOLLOWING
    } else {
        PRECEDING
    };
    DISCONNECTED | IMPLEMENTATION_SPECIFIC | order
}

fn page_attr(name: Option<&CStr>) -> bool {
    name.is_some_and(|n| !southstar_dom::attrs::is_internal(n.to_bytes()))
}

fn attrs_equal(a: Element, b: Element) -> bool {
    let count = |n: Element| n.attrs().filter(|attr| page_attr(attr.name())).count();
    if count(a) != count(b) {
        return false;
    }
    a.attrs().filter(|attr| page_attr(attr.name())).all(|want| {
        let local = southstar_dom::attrs::local_name(want);
        let Some(found) = b.attrs().find(|attr| {
            attr.namespace_uri() == want.namespace_uri()
                && southstar_dom::attrs::local_name(*attr) == local
        }) else {
            return false;
        };
        want.value().unwrap_or(c"") == found.value().unwrap_or(c"")
    })
}

fn names_equal(a: Element, b: Element) -> bool {
    match a.kind() {
        Kind::Element => {
            namespace_uri(a) == namespace_uri(b)
                && prefix(a) == prefix(b)
                && local_name(a) == local_name(b)
        }
        Kind::Comment => {
            let pi = a.flags() & FLAG_PI;
            pi == b.flags() & FLAG_PI && (pi == 0 || a.name() == b.name())
        }
        Kind::Doctype => a.name() == b.name(),
        _ => true,
    }
}

pub(crate) fn equal(a: Element, b: Element, depth: i32) -> bool {
    if a == b {
        return true;
    }
    if depth >= MAX_DEPTH || a.kind_raw() != b.kind_raw() || !names_equal(a, b) {
        return false;
    }
    if a.text() != b.text() || !attrs_equal(a, b) {
        return false;
    }
    let (mut ca, mut cb) = (a.first_child(), b.first_child());
    while let (Some(x), Some(y)) = (ca, cb) {
        if !equal(x, y, depth + 1) {
            return false;
        }
        ca = x.next_sibling();
        cb = y.next_sibling();
    }
    ca.is_none() && cb.is_none()
}

pub(crate) fn node_type(node: Option<Element>) -> i32 {
    let Some(node) = node else {
        return 0;
    };
    if node.flags() & FLAG_FRAGMENT != 0 || is_shadow_root(node) {
        return 11;
    }
    match node.kind() {
        Kind::Element => 1,
        Kind::Text if node.flags() & FLAG_CDATA != 0 => 4,
        Kind::Text => 3,
        Kind::Comment if node.flags() & FLAG_PI != 0 => 7,
        Kind::Comment => 8,
        Kind::Document => 9,
        Kind::Doctype => 10,
        Kind::Other => 0,
    }
}
