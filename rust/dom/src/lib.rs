//! Southstar — the DOM: nodes and attributes, document indexes, serialization and form controls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod attrs;
pub mod base_url;
pub mod controls;
mod data;
mod ffi;
mod handle;
pub mod image_map;
pub mod index;
pub mod node;
pub mod select;
pub mod serialize;
pub mod store;
#[cfg(test)]
mod tests;
pub mod tree;

pub use data::{Invalidator, Kind, NsNode};
pub use handle::{Attr, AttrView, Node};
pub use index::NodeArray;
pub use store::WeakNode;

pub const MAX_DEPTH: i32 = 512;
pub const FLAG_SVG_NS: u32 = 1 << 7;
pub const FLAG_FOREIGN_NS: u32 = 1 << 9;
pub const FLAG_PI: u32 = 1 << 11;
pub const FLAG_SCRIPTING_DISABLED: u32 = 1 << 15;

pub fn children(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.first_child(), |child| child.next_sibling())
}

pub fn ancestors(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.parent(), |parent| parent.parent())
}

pub fn ancestors_and_self(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(Some(node), |n| n.parent())
}
