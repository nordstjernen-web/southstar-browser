//! Southstar — box-tree operations ported from layout.c: the box pool, inline run merging and splitting, appending children, order reordering and table box construction.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod order;
mod runs;
mod table;

pub use runs::{append_child, clone_range};
