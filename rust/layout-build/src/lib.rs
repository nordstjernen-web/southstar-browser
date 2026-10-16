//! Southstar — box-tree construction ported from layout.c: blocks, inline runs, replaced and form-control boxes, pseudo-element boxes, shadow slots, the pending absolutely positioned boxes and the layout entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod atomic;
mod block;
mod build;
mod classify;
mod collect;
mod controls;
mod ffi;
mod generated;
mod media;
mod run;
mod state;

pub use ffi::{
    abs_build_box, abs_pending_clear, abs_pending_entry, abs_pending_len, abs_static_run,
    abs_static_target, box_is_abs_placeholder, flat_parent, image_dimension_attr,
    record_abs_static,
};
