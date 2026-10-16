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
mod hit;
mod media;
mod run;
mod state;
