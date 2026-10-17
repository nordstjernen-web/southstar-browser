//! Southstar — box-tree operations ported from layout.c: the box pool, inline run merging and splitting, appending children, order reordering and table box construction.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod ffi;
mod order;
mod runs;
mod table;

pub use ffi::{Builders, install};
pub use order::reorder_children;
pub use runs::{append_child, clone_range, join_splits, merge_prefix, run_split};
pub use table::{build_anonymous_table, build_table, is_table_box, node_is_table_internal};
