//! Southstar — painting the layout box tree with cairo and ns-pango: the box tree walk with stacking, clips, transforms and 3D, box decorations, replaced content, masks, inline text, list markers, the page text setup and the viewport layer planner.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod blur;
mod decor;
mod document;
pub mod ffi;
mod filter;
mod inline;
mod marker;
mod mask;
mod media;
mod radii;
mod state;
mod text;
mod three_d;
mod util;
mod walk;
