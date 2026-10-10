//! Southstar — painting the layout box tree with cairo and ns-pango, ported from paint.c section by section: so far page text setup, fonts, line heights, list markers, box decorations, replaced content, masks and inline text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod blur;
mod decor;
pub mod ffi;
mod filter;
mod inline;
mod marker;
mod mask;
mod media;
mod radii;
mod state;
mod text;
mod util;
