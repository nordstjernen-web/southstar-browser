//! Southstar — selector queries and live collections: querySelector, matches and closest, the getElementsBy* lookups, static NodeLists and live HTMLCollections.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod live;
mod nodelist;
mod page;
mod query;

use core::ffi::CStr;

use southstar_dom::serialize::is_embedded_doc;
use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, Kind, MAX_DEPTH, Node};
use southstar_js_engine::{Scope, Value};

pub(crate) type JsResult<T = Value> = Result<T, Value>;
pub(crate) type Element = Node<'static>;

pub(crate) const FLAG_FRAGMENT: u32 = 1 << 2;
pub(crate) const FLAG_QUIRKS: u32 = 1 << 5;
pub(crate) const FLAG_XML_DOC: u32 = 1 << 14;
pub(crate) const FOREIGN: u32 = FLAG_SVG_NS | FLAG_FOREIGN_NS;
pub(crate) const SHADOW_ATTR: &CStr = c"data-nd-shadow-root";

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn is_shadow_root(node: Element) -> bool {
    node.is_element() && node.attr(SHADOW_ATTR).is_some()
}

pub(crate) fn hidden_child(node: Element) -> bool {
    is_embedded_doc(node) || is_shadow_root(node)
}

pub(crate) fn is_named(node: Element, tag: &[u8]) -> bool {
    node.element_name() == Some(tag)
}

pub(crate) fn is_template(node: Element) -> bool {
    is_named(node, b"template")
}

pub(crate) fn is_class_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\x0c' | b'\r')
}

pub(crate) fn class_tokens(list: &[u8]) -> impl Iterator<Item = &[u8]> {
    list.split(|&c| is_class_space(c))
        .filter(|token| !token.is_empty())
}

pub(crate) fn id_is(node: Element, id: &[u8]) -> bool {
    node.is_element() && node.attr(c"id").is_some_and(|v| v.to_bytes() == id)
}

pub(crate) fn has_class(node: Element, class: &[u8]) -> bool {
    if !node.is_element() || class.is_empty() {
        return false;
    }
    node.attr(c"class")
        .is_some_and(|list| class_tokens(list.to_bytes()).any(|token| token == class))
}

pub(crate) fn is_ancestor_or_self(node: Element, root: Element) -> bool {
    if node == root {
        return true;
    }
    southstar_dom::ancestors(node)
        .take(MAX_DEPTH as usize)
        .any(|p| p == root)
}

pub(crate) fn element_children(node: Element) -> impl Iterator<Item = Element> {
    southstar_dom::children(node)
}

pub(crate) fn walk_elements(
    node: Element,
    depth: i32,
    stop_at_template: bool,
    visit: &mut dyn FnMut(Element),
) {
    if depth >= MAX_DEPTH || hidden_child(node) {
        return;
    }
    if node.kind() == Kind::Element {
        visit(node);
    }
    if stop_at_template && is_template(node) {
        return;
    }
    for child in element_children(node) {
        walk_elements(child, depth + 1, stop_at_template, visit);
    }
}

pub(crate) fn first_element(
    node: Element,
    depth: i32,
    stop_at_template: bool,
    test: &mut dyn FnMut(Element) -> bool,
) -> Option<Element> {
    if depth >= MAX_DEPTH || hidden_child(node) {
        return None;
    }
    if node.kind() == Kind::Element && test(node) {
        return Some(node);
    }
    if stop_at_template && is_template(node) {
        return None;
    }
    element_children(node).find_map(|child| first_element(child, depth + 1, stop_at_template, test))
}

pub(crate) fn push(scope: &mut Scope<'_>, array: &Value, index: &mut u32, value: Value) {
    let _ = scope.set_index(array, *index, value);
    *index += 1;
}
