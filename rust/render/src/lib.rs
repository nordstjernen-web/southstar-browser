//! Southstar — the style and layout pipeline shared by the window and headless runs: cascade, web fonts on demand, zoom, container-query passes and layout.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod fonts;

pub use ffi::{RenderCtx, RenderProfile, page_rule, page_uses_active, page_uses_hover, relayout};
