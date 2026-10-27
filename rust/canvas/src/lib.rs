//! Southstar — the canvas 2D API, ported from js_canvas.c and js_canvas_api.c section by section.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod api;
mod bitmap;
mod color;
pub mod context;
mod ctxmisc;
mod ctxpath;
mod draw;
pub(crate) mod ffi;
mod font;
mod gradient;
mod hidden;
pub mod host;
mod images;
pub mod links;
mod path;
mod path2d;
mod raster;
mod state;
mod style;
mod text;
mod validate;

pub use ffi::state::install_runtime_hooks;

pub use ffi::exports::nsr_canvas_clone_object;
pub use ffi::exports::nsr_canvas_hidden_get;
pub use ffi::exports::nsr_canvas_hidden_is;
pub use ffi::exports::nsr_dommatrix_make;
pub use ffi::exports::nsr_image_bitmap_is;
