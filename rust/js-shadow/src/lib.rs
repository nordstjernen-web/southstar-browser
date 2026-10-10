//! Southstar — shadow DOM and slots: attachShadow, the ShadowRoot wrapper, slot assignment, assignedSlot and getRootNode.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod root;
mod slot;

use southstar_dom::{Kind, Node};
use southstar_js_engine::Value;

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) const SHADOW_ATTR: &core::ffi::CStr = c"data-nd-shadow-root";

pub(crate) fn is_shadow_root(node: Option<Element>) -> bool {
    node.is_some_and(|n| n.kind() == Kind::Element && n.attr(SHADOW_ATTR).is_some())
}

pub(crate) fn shadow_child(host: Element) -> Option<Element> {
    southstar_dom::children(host).find(|&c| is_shadow_root(Some(c)))
}

pub(crate) fn is_closed(root: Element) -> bool {
    root.attr(SHADOW_ATTR)
        .is_some_and(|mode| mode.to_bytes() == b"closed")
}

pub(crate) fn is_slot(node: Option<Element>) -> bool {
    node.and_then(Node::element_name)
        .is_some_and(|name| name.eq_ignore_ascii_case(b"slot"))
}
