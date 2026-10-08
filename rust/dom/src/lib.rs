//! Southstar — the ns_node layout of src/dom.h and borrowed node handles for ported modules that read the C DOM.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub use ffi::{Node, NsNode};

pub fn children(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.first_child(), |child| child.next_sibling())
}

pub fn ancestors(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.parent(), |parent| parent.parent())
}
