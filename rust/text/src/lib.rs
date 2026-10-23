//! Southstar text — text layout ported from web-text-lib, a Pango fork: attributes, breaking, fonts, shaping, layout and rendering.
//! Copyright 1999-2024 The Pango authors; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#![deny(unsafe_code)]

pub mod attrs;
pub mod bidi;
pub mod coverage;
pub mod emoji;
#[allow(unsafe_code)]
mod ffi;
pub mod font_description;
pub mod gravity;
pub mod language;
pub mod matrix;
pub mod metrics;
pub mod script;
pub mod tabs;
pub mod types;
pub mod unicode;

pub use types::*;
