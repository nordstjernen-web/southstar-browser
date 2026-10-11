//! Southstar — the static library meson links into the engine, renderer and helper-free C targets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;

pub use southstar_ffi;

#[unsafe(no_mangle)]
pub extern "C" fn ns_app_self_exe() -> *const c_char {
    core::ptr::null()
}
