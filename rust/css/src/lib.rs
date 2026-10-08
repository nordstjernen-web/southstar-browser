//! Southstar — css.c ported to Rust section by section: so far the colour parser, lengths with calc() and the other math functions, and container queries.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod calc;
mod color;
mod container;
mod ffi;
mod math;
mod scan;
mod units;

pub use color::parse_color;
pub use ffi::NsCssValue;
