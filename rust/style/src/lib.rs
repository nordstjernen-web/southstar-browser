//! Southstar — computed styles read from Rust: properties by name and by id, ns_style's values with their gradients, shadows, transforms and layers, display and pseudo-element styles, comparing two styles, and the css.c calls that interpret them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub use ffi::{
    CalcSize, Display, Kind, NsCssValue, PROP_COUNT, Prop, RectValue, SizeValue, StyleRef,
    StyleTable, Value, ValueRef, attr_style, border_image_params, border_image_source, box_style,
    col_hint_style, display_of, effective_transform, font_family_for_pango, font_stretch_rank,
    layer, layer_count, styles_equal, value_slot, values_equal,
};
pub use southstar_css::{
    BorderImageParams, CALC_SIZE_ANY, CALC_SIZE_AUTO, CALC_SIZE_FIT_CONTENT, CALC_SIZE_MAX_CONTENT,
    CALC_SIZE_MIN_CONTENT, CALC_SIZE_PERCENT, CALC_SIZE_STRETCH, Gradient, GradientStop,
    GridAreaRect, GridAreas, GridLineName, GridTrack, GridTracks, Prop as PropId, Shadow,
    ShadowList, TimingFunction, Transform, TransformOp, gradient_angle, gradient_radii,
    parse_color, timing_linear_eval, timing_parse, timing_serialize, transform_is_3d,
    transform_to_mat4,
};

pub const UNIT_PX: u32 = 0;
pub const UNIT_PERCENT: u32 = 3;
