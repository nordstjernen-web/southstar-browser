//! Southstar — inline layout ported from layout.c: line boxes over ns-pango, line-height struts, atomic inlines, text alignment, vertical text, the measurement cache, inline intrinsic widths and the form control a point in a run falls on.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod attrs;
mod ffi;
mod form_hit;
mod intrinsic;
mod layout;
mod measure;
mod multicol;

pub use form_hit::inline_box_form_hit;
pub use intrinsic::{min_width as inline_min_width, natural_width as inline_natural_width};
pub use layout::{inline_layout as layout_inline, line_height};
pub use multicol::{Columns, can_fragment, layout_single_inline, run_break_before};
