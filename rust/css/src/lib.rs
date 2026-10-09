//! Southstar — css.c ported to Rust section by section: so far the colour parser, lengths with calc() and the other math functions, container queries, gradients, positions and image values, transforms, grid values and font values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod calc;
mod color;
mod container;
mod content;
mod ffi;
mod font;
mod gradient;
mod grid;
mod image;
mod lex;
mod math;
mod position;
mod scan;
mod text;
mod transform;
mod units;

pub use color::parse_color;
pub use ffi::NsCssValue;
