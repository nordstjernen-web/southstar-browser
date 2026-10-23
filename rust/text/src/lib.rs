//! Southstar text — text layout ported from web-text-lib, a Pango fork: attributes, breaking, fonts, shaping, layout and rendering.
//! Copyright 1999-2024 The Pango authors; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#![deny(unsafe_code)]

pub mod attrs;
pub mod bidi;
pub mod breaks;
pub mod context;
pub mod coverage;
mod ellipsize;
pub mod emoji;
#[allow(unsafe_code)]
mod ffi;
pub mod font;
pub mod font_description;
pub mod fontmap;
pub mod fontset;
pub mod glyph;
pub mod glyph_item;
pub mod gravity;
pub mod item;
pub mod item_cache;
pub mod itemize;
pub mod language;
pub mod layout;
pub mod matrix;
pub mod metrics;
pub mod render;
#[cfg(feature = "rust-shaper")]
pub mod rust_shaper;
pub mod script;
pub mod shape;
pub mod shape_cache;
pub mod tabs;
pub mod types;
pub mod unicode;
mod utf8;
pub mod utils;

pub use types::*;
