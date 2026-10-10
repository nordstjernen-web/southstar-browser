//! Southstar — node tree mutation: appendChild, insertBefore, removeChild, replaceChild, moveBefore, the ChildNode and ParentNode methods and pre-insertion validity.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod child;
mod ffi;
mod sequence;
mod validity;

use southstar_dom::{Kind, MAX_DEPTH, Node};
use southstar_js_engine::{Scope, Value};

use crate::ffi::Js;

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

const FLAG_FRAGMENT: u32 = 1 << 2;
pub(crate) const HIERARCHY_REQUEST_ERR: i32 = 3;
pub(crate) const NOT_FOUND_ERR: i32 = 8;

pub(crate) fn is_document(node: Element) -> bool {
    node.kind() == Kind::Document && node.flags() & FLAG_FRAGMENT == 0
}

pub(crate) fn is_fragment(node: Element) -> bool {
    node.kind() == Kind::Document && node.flags() & FLAG_FRAGMENT != 0
}

pub(crate) fn is_detached_document(node: Element) -> bool {
    node.kind() == Kind::Document && node.parent().is_none()
}

pub(crate) fn within(node: Element, root: Element) -> bool {
    if node == root {
        return true;
    }
    let mut depth = 0;
    let mut cursor = node.parent();
    while let Some(ancestor) = cursor {
        if depth >= MAX_DEPTH {
            break;
        }
        depth += 1;
        if ancestor == root {
            return true;
        }
        cursor = ancestor.parent();
    }
    false
}

pub(crate) fn hierarchy_error(scope: &mut Scope<'_>, message: &str) -> Value {
    ffi::dom_exception(
        scope,
        "HierarchyRequestError",
        HIERARCHY_REQUEST_ERR,
        message,
    )
}

pub(crate) fn not_found_error(scope: &mut Scope<'_>, message: &str) -> Value {
    ffi::dom_exception(scope, "NotFoundError", NOT_FOUND_ERR, message)
}

pub(crate) fn record_move_removal(js: Js, node: Element) {
    if let Some(parent) = node.parent() {
        ffi::record_child_change(
            js,
            parent,
            None,
            Some(node),
            node.prev_sibling(),
            node.next_sibling(),
        );
    }
}

pub(crate) fn record_fragment_emptied(js: Js, fragment: Element) {
    let kids: Vec<Element> = southstar_dom::children(fragment).collect();
    if !kids.is_empty() {
        ffi::emit_child_list(js, fragment, &[], &kids, None, None);
    }
}

pub(crate) fn nodes_inserted(js: Js, parent: Element, nodes: &[Element]) {
    if ffi::in_template_content(parent) {
        return;
    }
    for &node in nodes {
        ffi::run_inserted_scripts(js, node);
    }
    for &node in nodes {
        ffi::ce_upgrade_subtree_all(js, node);
    }
}

pub(crate) fn detach_recorded(js: Option<Js>, node: Element) {
    let Some(parent) = node.parent() else {
        return;
    };
    let previous = node.prev_sibling();
    let next = node.next_sibling();
    if let Some(js) = js {
        ffi::iters_pre_remove(js, node);
        ffi::ce_disconnect_subtree(js, node);
    }
    node.detach();
    if let Some(js) = js {
        ffi::orphan(js, node);
        ffi::record_child_change(js, parent, None, Some(node), previous, next);
    }
}
