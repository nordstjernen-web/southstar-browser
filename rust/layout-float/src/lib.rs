//! Southstar — floats and block formatting contexts ported from layout.c: float bands, escaping floats, BFC roots, overflow keywords and multicol column hosts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
pub mod floats;
mod overflow;

pub use ffi::{FloatRef, overflow_axis_keyword};
pub use overflow::{
    box_clips_children, box_establishes_bfc, box_is_scroll_container, clear_kind_of,
    clips_out_point, float_side_of, kw_scrolls, max_bottom, multicol_column_host,
    multicol_distributable_children, overflow_establishes_bfc, style_is_grid_container,
};
