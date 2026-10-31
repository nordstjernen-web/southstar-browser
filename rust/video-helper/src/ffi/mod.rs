//! Southstar — the helper's process entry point and its bindings to libav, shared memory, the clock and the sandbox.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod decoder;
pub mod os;
pub mod ring;

#[cfg(not(feature = "rust-main"))]
#[unsafe(no_mangle)]
pub extern "C" fn main(
    _argc: core::ffi::c_int,
    _argv: *const *const core::ffi::c_char,
) -> core::ffi::c_int {
    crate::protocol::run();
    0
}
