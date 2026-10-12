//! Southstar — the southstar-audio helper process, linked by meson into the executable whose `main` this exports.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod decode;
mod ffi;
mod helper;
mod mixer;

use core::ffi::{c_char, c_int};

#[unsafe(no_mangle)]
pub extern "C" fn main(_argc: c_int, _argv: *mut *mut c_char) -> c_int {
    helper::run();
    0
}
