//! Southstar — the GTK shell's ported code, linked with every other Rust module into the shell executable.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod appmain;
mod downloads;
mod ffi;
mod icons;
mod pagelayers;
mod taskmgr;
mod titlerow;

pub use southstar_ffi;
