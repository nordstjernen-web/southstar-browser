//! Southstar — the GLib monotonic clock and message log, behind safe functions so callers need no FFI of their own.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
use std::ffi::CString;

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
    fn g_log(domain: *const c_char, level: c_int, format: *const c_char, ...);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Critical,
    Warning,
    Message,
    Info,
    Debug,
}

impl LogLevel {
    fn flag(self) -> c_int {
        match self {
            LogLevel::Critical => 1 << 3,
            LogLevel::Warning => 1 << 4,
            LogLevel::Message => 1 << 5,
            LogLevel::Info => 1 << 6,
            LogLevel::Debug => 1 << 7,
        }
    }
}

pub fn monotonic_us() -> i64 {
    // SAFETY: g_get_monotonic_time takes no arguments and only reads the system clock.
    unsafe { g_get_monotonic_time() }
}

pub fn log(level: LogLevel, message: &[u8]) {
    let text: Vec<u8> = message.iter().copied().filter(|&b| b != 0).collect();
    let text = CString::new(text).unwrap_or_default();
    // SAFETY: the format is "%s" and its one argument is a NUL-terminated string alive for the call.
    unsafe {
        g_log(
            core::ptr::null(),
            level.flag(),
            c"%s".as_ptr(),
            text.as_ptr(),
        )
    };
}

pub fn warning(message: impl AsRef<[u8]>) {
    log(LogLevel::Warning, message.as_ref());
}
