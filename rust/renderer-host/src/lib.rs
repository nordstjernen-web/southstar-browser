//! Southstar — the renderer side of the renderer protocol, serving a page to the window that owns it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod engine;
mod ffi;
mod frame;
mod inproc;
mod process;
mod serve;
mod tiles;

pub use inproc::{single_process_enable, single_process_enabled, single_process_shutdown};
