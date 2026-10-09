//! Southstar — the box tree of src/layout.h with borrowed box handles over it, and layout.c ported by section: so far choosing the image an <img> loads from srcset, sizes and <picture> sources.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod image_source;
mod srcset;

pub use ffi::{BoxKind, BoxRef, Edges, MediaRef, NsBox, NsBoxMedia, Style};

pub fn children(b: BoxRef<'_>) -> impl Iterator<Item = BoxRef<'_>> {
    core::iter::successors(b.first_child(), |child| child.next_sibling())
}
