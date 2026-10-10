//! Southstar — computed styles of src/css.h read from Rust: properties by name and by id, ns_style's values with their gradients, shadows, transforms and layers, display and pseudo-element styles, comparing two styles, and the css.c calls that interpret them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

pub use ffi::{
    Display, Kind, NsCssValue, PROP_COUNT, Prop, RectValue, SizeValue, StyleRef, StyleTable, Value,
    ValueRef, display_of, font_family_for_pango, layer, layer_count, parse_color, retain,
    styles_equal, value_slot, values_equal,
};
pub use southstar_css::{
    Gradient, GradientStop, Prop as PropId, Shadow, ShadowList, Transform, TransformOp,
};

pub const UNIT_PERCENT: u32 = 3;
