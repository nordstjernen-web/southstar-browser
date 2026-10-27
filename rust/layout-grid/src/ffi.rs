//! Southstar — the style and box helpers grid layout borrows from the other layout crates.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::BoxRef;
use southstar_style::StyleRef;

pub(crate) use southstar_layout::GridTrack as TrackEdges;

pub(crate) fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    southstar_style::box_style(b)
}

pub(crate) use southstar_layout_geom::dispatch::layout_box as layout_child;

pub(crate) use southstar_layout_sizing::measure_natural_width as natural_width;

pub(crate) fn min_width(b: BoxRef<'_>, parent_style: Option<StyleRef<'_>>) -> f64 {
    southstar_layout_sizing::min_width_of(Some(b), parent_style)
}

pub(crate) use southstar_layout_geom::dispatch::shift_box_tree as shift;

pub(crate) fn establishes_bfc(s: StyleRef<'_>) -> bool {
    southstar_layout_float::overflow_establishes_bfc(Some(s))
}
