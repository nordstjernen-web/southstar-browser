//! Southstar — the style, box and sizing helpers table layout borrows from the other layout crates.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::{BoxRef, Style};
use southstar_style::StyleRef;

pub(crate) fn style_of(s: *const Style) -> Option<StyleRef<'static>> {
    unsafe { StyleRef::from_ptr(s) }
}

pub(crate) fn min_width_of(b: BoxRef<'_>, parent_style: *const Style) -> f64 {
    southstar_layout_sizing::min_width_of(Some(b), parent_style)
}

pub(crate) use southstar_layout_sizing::{measure_natural_width, min_content_width_of};

pub(crate) use southstar_layout::dispatch::{
    layout_block, layout_box, legacy_align_block_child, shift_box_tree,
};

pub(crate) fn translate_subtree(b: BoxRef<'_>, dx: f64, dy: f64) {
    if dx != 0.0 || dy != 0.0 {
        shift_box_tree(b, dx, dy);
    }
}
