//! Southstar — floats and block formatting contexts ported from layout.c: float bands, escaping floats, BFC roots, overflow keywords and multicol column hosts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod floats;
mod overflow;

pub use overflow::{float_side_of, overflow_establishes_bfc, style_is_grid_container};
