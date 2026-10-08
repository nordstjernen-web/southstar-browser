//! Southstar — the style and layout pipeline of src/render.h shared by the window and headless runs: cascade, web fonts on demand, zoom, container-query passes and layout.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod fonts;
mod viewport;

pub use ffi::{RenderCtx, RenderProfile, ns_render_relayout, ns_render_relayout_profile};
