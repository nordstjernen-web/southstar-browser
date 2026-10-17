//! Southstar — CSS grid layout ported from layout.c: track sizing, line resolution and auto-placement, alignment, subgrids, intrinsic widths and grid-positioned absolute boxes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod abs;
mod ffi;
mod intrinsic;
mod layout;
mod lines;
mod place;
mod text;
mod tracks;

pub use abs::{
    Area, containing_block as abs_containing_block, resolved_tracks as grid_resolved_tracks,
    static_align_offset as abs_static_align_offset, static_position as abs_static_position,
};
pub use intrinsic::{
    column_flow_width as grid_column_flow_width, flows_by_column as grid_flows_by_column,
    natural_width as grid_natural_width,
};
pub use layout::layout_grid;
