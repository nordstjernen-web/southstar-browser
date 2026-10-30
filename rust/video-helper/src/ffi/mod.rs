//! Southstar — the helper's process entry point and its bindings to libav, shared memory, the clock and the sandbox.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod decoder;
pub mod libav;
pub mod os;
pub mod ring;

use core::ffi::{c_char, c_int};

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *const *const c_char) -> c_int {
    crate::protocol::run();
    0
}
