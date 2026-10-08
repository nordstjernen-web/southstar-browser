//! Southstar — css.c ported to Rust section by section: so far the colour parser, lengths with calc() and the other math functions, container queries, gradients, positions and image values, and transforms.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod calc;
mod color;
mod container;
mod content;
mod ffi;
mod gradient;
mod image;
mod math;
mod position;
mod scan;
mod text;
mod transform;
mod units;

pub use color::parse_color;
pub use ffi::NsCssValue;
