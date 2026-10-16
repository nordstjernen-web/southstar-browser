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
