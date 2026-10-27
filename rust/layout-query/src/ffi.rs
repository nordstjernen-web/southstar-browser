//! Southstar — the box-tree query entry points and the length resolution and inline extents they call back into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_layout::{BoxRef, InlineAttr, inline_kind, paint_links};
use southstar_style::StyleRef;

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub fn dom_of(b: BoxRef<'_>) -> Option<Node<'_>> {
    b.dom()
}

pub fn inline_range_extents(b: BoxRef<'_>, r: &InlineAttr) -> Option<(f64, f64, f64, f64)> {
    let element = (r.kind == inline_kind::ELEMENT).then_some(r);
    paint_links::inline_range_extents(b, r.start, r.len, element)
}
