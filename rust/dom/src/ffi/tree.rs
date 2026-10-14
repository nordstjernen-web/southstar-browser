//! Southstar — the C ABI of the tree, modal and inertness helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::{self as glib, GBoolean};

use super::{Node, NsNode};
use crate::tree;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_root(n: *const NsNode) -> *const NsNode {
    Node::ptr_or_null(unsafe { Node::from_ptr(n) }.map(tree::root))
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
