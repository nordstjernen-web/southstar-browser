//! Southstar — the C ABI of freeing, unlinking and walking nodes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;

use southstar_glib::GBoolean;

use super::node;
use crate::{Node, NsNode};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_free(n: *mut NsNode) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(n) = unsafe { node(n) } {
        n.free_tree();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_remove(child: *mut NsNode) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(child) = unsafe { node(child) } {
        child.detach();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_next_in_subtree(
    n: *const NsNode,
    root: *const NsNode,
    descend: GBoolean,
) -> *mut NsNode {
    // SAFETY: the C callers pass null or a node of this thread's store.
    let Some(n) = (unsafe { node(n) }) else {
        return ptr::null_mut();
    };
    // SAFETY: as above, for the subtree root.
    let root = unsafe { node(root) };
    crate::index::next_in_subtree(n, root, descend != 0).map_or(ptr::null_mut(), Node::as_mut_ptr)
}
