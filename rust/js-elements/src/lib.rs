//! Southstar — element-specific HTML bindings: the table API, <img> state, labels, and the small element getters and setters.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod element;
mod ffi;
mod image;
mod table;

use southstar_dom::{MAX_DEPTH, Node};
use southstar_js_engine::{Scope, Value};

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) fn named(node: Option<Element>, tag: &[u8]) -> bool {
    node.and_then(Element::element_name) == Some(tag)
}

pub(crate) fn named_ignore_case(node: Element, tag: &[u8]) -> bool {
    node.element_name()
        .is_some_and(|name| name.eq_ignore_ascii_case(tag))
}

pub(crate) fn wrap_all(scope: &mut Scope<'_>, nodes: impl Iterator<Item = Element>) -> JsResult {
    let array = scope.new_array();
    for (index, node) in nodes.enumerate() {
        let wrapper = ffi::make_node(scope, Some(node));
        scope.set_index(&array, index as u32, wrapper)?;
    }
    Ok(array)
}

pub(crate) fn first_descendant_named(root: Element, tag: &[u8]) -> Option<Element> {
    fn walk(root: Element, tag: &[u8], depth: i32) -> Option<Element> {
        if depth >= MAX_DEPTH {
            return None;
        }
        for child in southstar_dom::children(root) {
            if named_ignore_case(child, tag) {
                return Some(child);
            }
            if let Some(found) = walk(child, tag, depth + 1) {
                return Some(found);
            }
        }
        None
    }
    walk(root, tag, 0)
}

pub(crate) fn descendants_named(root: Element, tag: &[u8]) -> Vec<Element> {
    fn walk(root: Element, tag: &[u8], out: &mut Vec<Element>, depth: i32) {
        if depth >= MAX_DEPTH {
            return;
        }
        for child in southstar_dom::children(root) {
            if named_ignore_case(child, tag) {
                out.push(child);
            }
            walk(child, tag, out, depth + 1);
        }
    }
    let mut out = Vec::new();
    walk(root, tag, &mut out, 0);
    out
}
