//! Southstar — the box tree with borrowed box handles over it, the layout entry points the layout crates recurse through, and layout.c ported by section: so far choosing the image an <img> loads from srcset, sizes and <picture> sources, and the image-map area under a point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod dispatch;
mod ffi;
mod image_map;
mod image_source;
mod srcset;

pub use ffi::{
    BoxKind, BoxPool, BoxRef, Edges, InlineAtomic, InlineAttr, LinkRange, LinkVec, MediaRef, NsBox,
    NsBoxMedia, Style, TableColHint, choose_img, image_map_area, img_chosen_density,
    img_chosen_url, inline_kind, link_clear_fn,
};
pub use image_source::Chosen;

pub fn children(b: BoxRef<'_>) -> impl Iterator<Item = BoxRef<'_>> {
    core::iter::successors(b.first_child(), |child| child.next_sibling())
}
