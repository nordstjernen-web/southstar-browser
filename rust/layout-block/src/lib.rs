//! Southstar — block layout ported from layout.c: the per-box dispatch, block widths, margin collapsing, floats and clearance, multicol columns, heights, fieldset legends and legacy alignment.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod align;
mod block;
mod dispatch;
mod ffi;
mod fieldset;
mod flow;
mod height;

pub use align::legacy_align_block_child;
pub use block::layout_block;
pub use dispatch::{layout_box, reset_container_queries, shift_box_tree};
