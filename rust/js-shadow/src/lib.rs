//! Southstar — shadow DOM and slots: attachShadow, the ShadowRoot wrapper, slot assignment, assignedSlot and getRootNode.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

pub(crate) mod ffi;
pub mod root;
pub mod slot;

pub use slot::assigned_slot_node;

use southstar_dom::{Kind, Node};
use southstar_js_engine::Value;

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) const SHADOW_ATTR: &core::ffi::CStr = c"data-nd-shadow-root";

pub fn is_shadow_root(node: Node<'_>) -> bool {
    node.kind() == Kind::Element && node.attr(SHADOW_ATTR).is_some()
}

pub fn shadow_child(host: Element) -> Option<Element> {
    southstar_dom::children(host).find(|&c| is_shadow_root(c))
}

pub fn is_closed(root: Node<'_>) -> bool {
    root.attr(SHADOW_ATTR)
        .is_some_and(|mode| mode.to_bytes() == b"closed")
}

fn shadow_root_of(node: Node<'_>) -> Option<Node<'_>> {
    southstar_dom::ancestors_and_self(node).find(|&n| is_shadow_root(n))
}

fn inclusive_ancestor(ancestor: Node<'_>, node: Node<'_>) -> bool {
    southstar_dom::ancestors_and_self(node).any(|n| n == ancestor)
}

pub fn retarget<'a>(target: Node<'a>, against: Node<'_>) -> Node<'a> {
    let mut visible = target;
    for _ in 0..southstar_dom::MAX_DEPTH {
        let Some(root) = shadow_root_of(visible) else {
            return visible;
        };
        if inclusive_ancestor(root, against) {
            return visible;
        }
        let Some(host) = root.parent() else {
            return visible;
        };
        visible = host;
    }
    visible
}

pub fn in_shadow_tree(node: Node<'_>) -> bool {
    shadow_root_of(node).is_some()
}

pub fn hidden_in_closed_tree(node: Node<'_>, from: Option<Node<'_>>) -> bool {
    southstar_dom::ancestors_and_self(node)
        .filter(|&n| is_shadow_root(n) && is_closed(n))
        .any(|root| !from.is_some_and(|from| inclusive_ancestor(root, from)))
}

pub(crate) fn is_slot(node: Option<Element>) -> bool {
    node.and_then(Node::element_name)
        .is_some_and(|name| name.eq_ignore_ascii_case(b"slot"))
}

pub fn install_runtime_hooks() {
    southstar_js_forms::links::set_js_shadow_links(southstar_js_forms::links::JsShadowLinks {
        element_get_shadow_root: crate::ffi::nsr_element_get_shadowRoot,
    });
}
