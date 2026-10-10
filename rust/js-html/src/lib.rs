//! Southstar — HTML fragment parsing and serialization bindings: innerHTML, outerHTML, getHTML, setHTMLUnsafe, insertAdjacent*, textContent and microdata.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod markup;
mod microdata;
mod text;

use southstar_dom::{FLAG_SCRIPTING_DISABLED, Kind, MAX_DEPTH, Node};
use southstar_js_engine::Value;

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

const FLAG_FRAGMENT: u32 = 1 << 2;
const FLAG_XML_DOC: u32 = 1 << 14;

pub(crate) fn is_document(node: Element) -> bool {
    node.kind() == Kind::Document && node.flags() & FLAG_FRAGMENT == 0
}

pub(crate) fn is_fragment(node: Element) -> bool {
    node.kind() == Kind::Document && node.flags() & FLAG_FRAGMENT != 0
}

pub(crate) fn scripting_enabled(node: Element) -> bool {
    southstar_dom::tree::root(node).flags() & FLAG_SCRIPTING_DISABLED == 0
}

pub(crate) fn inclusive_ancestor(ancestor: Element, node: Element) -> bool {
    if node == ancestor {
        return true;
    }
    let mut depth = 0;
    let mut cursor = node.parent();
    while let Some(parent) = cursor {
        if depth >= MAX_DEPTH {
            break;
        }
        depth += 1;
        if parent == ancestor {
            return true;
        }
        cursor = parent.parent();
    }
    false
}

pub(crate) fn children(node: Element) -> Vec<Element> {
    southstar_dom::children(node).collect()
}

pub(crate) fn prepend(parent: Element, node: Element) {
    node.detach();
    match parent.first_child() {
        Some(first) => ffi::insert_sibling_before(first, node),
        None => parent.append(node),
    }
}

pub(crate) fn insert_after(reference: Element, node: Element) {
    node.detach();
    match (reference.next_sibling(), reference.parent()) {
        (Some(next), _) => ffi::insert_sibling_before(next, node),
        (None, Some(parent)) => parent.append(node),
        (None, None) => {}
    }
}
