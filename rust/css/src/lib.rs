//! Southstar — css.c ported to Rust section by section, starting with the colour parser and the scanning helpers it needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod color;
mod ffi;
mod scan;

pub use color::parse_color;
