//! Southstar — positioned layout ported from layout.c: relative offsets, absolutely and fixed positioned boxes, their static positions, and sticky offsets; and the document layout entry point that wires the layout crates together.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod abs;
mod ffi;
mod relative;
mod static_pos;
mod sticky;

pub use sticky::offset as sticky_offset;
