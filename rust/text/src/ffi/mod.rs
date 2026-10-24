//! Southstar text — the C libraries text layout calls: GLib Unicode data, fontconfig, FreeType, HarfBuzz and cairo.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

pub(crate) mod cairo;
pub(crate) mod fontconfig;
pub(crate) mod glib;
pub(crate) mod glib_case;
pub(crate) mod glib_decompose;
pub(crate) mod glib_lang;
pub(crate) mod harfbuzz;
#[cfg(all(test, feature = "rust-shaper"))]
pub(crate) mod harfbuzz_shape_test;
pub(crate) mod harfbuzz_strings;
#[cfg(feature = "thai")]
pub(crate) mod thai;
