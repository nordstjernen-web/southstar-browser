//! Southstar — generated content and form-control text ported from layout.c: CSS counters, pseudo-element content and quotes, control sizing and inline attributes, select and datalist rows and text input columns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod controls;
mod counters;
mod ffi;
pub mod pseudo;
pub mod select;
pub mod tags;
pub mod text_input;

pub use counters::{build as counters_build, clear as counters_clear};
pub use ffi::{inline_attr_control_width, install_image_peek, utf8_casefold};
