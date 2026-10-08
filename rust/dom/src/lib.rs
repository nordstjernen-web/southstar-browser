//! Southstar — the DOM of src/dom.h: the ns_node layout, borrowed node handles and the sections of dom.c in Rust.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod controls;
mod ffi;

pub use ffi::{Node, NsNode};

pub fn children(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.first_child(), |child| child.next_sibling())
}

pub fn ancestors(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.parent(), |parent| parent.parent())
}

pub fn ancestors_and_self(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(Some(node), |n| n.parent())
}
