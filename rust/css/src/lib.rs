//! Southstar — css.c ported to Rust section by section: so far the colour parser, lengths with calc() and the other math functions, container queries, gradients, positions and image values, transforms, grid values, font values, shadows, time values, easing functions and animation lists, and display, counter and list values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod animation;
mod calc;
mod color;
mod container;
mod content;
mod counter;
mod display;
mod ffi;
mod font;
mod gradient;
mod grid;
mod image;
mod lex;
mod math;
mod position;
mod scan;
mod shadow;
mod text;
mod time;
mod timing;
mod transform;
mod units;

pub use color::parse_color;
pub use ffi::NsCssValue;
