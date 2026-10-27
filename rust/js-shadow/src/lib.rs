//! Southstar — shadow DOM and slots: attachShadow, the ShadowRoot wrapper, slot assignment, assignedSlot and getRootNode.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

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

pub(crate) fn is_closed(root: Element) -> bool {
    root.attr(SHADOW_ATTR)
        .is_some_and(|mode| mode.to_bytes() == b"closed")
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
