//! Southstar — replaced-element and intrinsic sizing ported from layout.c: image, video and svg boxes, min-content and max-content widths, sizing keywords and natural-width estimates.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod estimate;
mod ffi;
mod intrinsic;
mod replaced;

pub use estimate::{estimate_natural_width, text_input_intrinsic_width};
pub use ffi::{Measures, install};
pub use intrinsic::{
    intrinsic_keyword_width, measure_natural_width, min_content_width_of, min_width_of,
    size_keyword_is_intrinsic,
};
pub use replaced::{
    SavedGeometry, layout_image, measure_inline_atomics_begin, measure_inline_atomics_end,
};
