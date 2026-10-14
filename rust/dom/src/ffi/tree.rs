//! Southstar — the C ABI of the tree, modal and inertness helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::{self as glib, GBoolean};

use super::node;
use crate::Node;
use crate::NsNode;
use crate::tree;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_root(n: *const NsNode) -> *const NsNode {
    // SAFETY: the C callers pass null or a node of this thread's store.
    Node::ptr_or_null(unsafe { node(n) }.map(tree::root))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dom_set_active_modal(modal: *const NsNode) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    tree::set_active_modal(unsafe { node(modal) });
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_dom_active_modal() -> *const NsNode {
    Node::ptr_or_null(tree::active_modal())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_effectively_inert(el: *const NsNode) -> GBoolean {
    // SAFETY: the C callers pass null or a node of this thread's store.
    glib::boolean(unsafe { node(el) }.is_some_and(tree::effectively_inert))
}
