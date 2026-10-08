//! Southstar — computed styles of src/css.h read from Rust: properties by name, ns_style's values and the css.c calls that interpret them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub use ffi::{Prop, StyleRef, StyleTable, Value, ValueRef, font_family_for_pango, parse_color};

pub const UNIT_PERCENT: u32 = 3;
