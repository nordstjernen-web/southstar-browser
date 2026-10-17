//! Southstar — painting the layout box tree with cairo and ns-pango: the box tree walk with stacking, clips, transforms and 3D, box decorations, replaced content, masks, inline text, list markers, the page text setup and the viewport layer planner.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod api;
mod blur;
mod clip_shape;
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

pub use api::{
    canvas_color, css_line_height_px, doc_layers, inline_xy_to_byte, inside_list_marker,
    invalidate_3d, list_ordinals_begin, list_ordinals_end, normal_line_height_px, paint,
    pango_font_size, pick_3d, plan_layers, registered_3d, set_anim, set_caret_visible, set_js,
    set_search, sync_inline_atomic_offsets, text_context, vp_layer,
};
pub use document::LayerPlan;
pub use walk::VpCapture;
