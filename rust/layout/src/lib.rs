//! Southstar — the box tree of src/layout.h: the ns_box layout and borrowed box handles over it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub use ffi::{BoxKind, BoxRef, Edges, NsBox, Style};

pub fn children(b: BoxRef<'_>) -> impl Iterator<Item = BoxRef<'_>> {
    core::iter::successors(b.first_child(), |child| child.next_sibling())
}
