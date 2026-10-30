//! Southstar — the southstar-audio helper process: meson links the `main` this exports, and a Rust `main` calls `run`.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod decode;
mod ffi;
mod helper;
mod mixer;
mod mp3;

pub use helper::run;

#[cfg(not(feature = "rust-main"))]
#[unsafe(no_mangle)]
pub extern "C" fn main(
    _argc: core::ffi::c_int,
    _argv: *mut *mut core::ffi::c_char,
) -> core::ffi::c_int {
    run();
    0
}
