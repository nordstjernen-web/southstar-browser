//! Southstar — the box tree's layout entry points with the inherited style as a typed style handle, for the formatting-context crates to recurse through.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::BoxRef;
use southstar_layout::dispatch as tree;
use southstar_style::StyleRef;

pub use tree::shift_box_tree;

pub fn layout_box(b: BoxRef<'_>, parent_content_width: f64, inherited: Option<StyleRef<'_>>) {
    tree::layout_box(b, parent_content_width, inherited.map(StyleRef::data));
}

pub fn layout_block(b: BoxRef<'_>, parent_content_width: f64, inherited: Option<StyleRef<'_>>) {
    tree::layout_block(b, parent_content_width, inherited.map(StyleRef::data));
}

pub fn legacy_align_block_child(
    c: BoxRef<'_>,
    avail_x: f64,
    avail_w: f64,
    inherited: Option<StyleRef<'_>>,
) {
    tree::legacy_align_block_child(c, avail_x, avail_w, inherited.map(StyleRef::data));
}
