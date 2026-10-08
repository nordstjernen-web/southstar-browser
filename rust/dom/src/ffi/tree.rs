//! Southstar — the C ABI of the tree, modal and inertness helpers declared in src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;

use southstar_glib::{self as glib, GBoolean};

use super::{Node, NsNode, c_str};
use crate::tree;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_root(n: *const NsNode) -> *const NsNode {
    Node::ptr_or_null(unsafe { Node::from_ptr(n) }.map(tree::root))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_element_named(
    n: *const NsNode,
    tag: *const c_char,
) -> GBoolean {
    let (Some(n), Some(tag)) = (unsafe { Node::from_ptr(n) }, c_str(tag)) else {
        return glib::FALSE;
    };
    glib::boolean(n.element_name() == Some(tag.to_bytes()))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_dom_set_active_modal(modal: *const NsNode) {
    tree::set_active_modal(modal);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_dom_active_modal() -> *const NsNode {
    tree::active_modal()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_effectively_inert(el: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { Node::from_ptr(el) }.is_some_and(tree::effectively_inert))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_hidden_until_found(el: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { Node::from_ptr(el) }.is_some_and(tree::hidden_until_found))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_details_fragment_needs_open(
    details: *const NsNode,
    target: *const NsNode,
) -> GBoolean {
    let (Some(details), Some(target)) = (unsafe { Node::from_ptr(details) }, unsafe {
        Node::from_ptr(target)
    }) else {
        return glib::FALSE;
    };
    glib::boolean(tree::details_fragment_needs_open(details, target))
}
