//! Southstar — the in-process debug log: level names and the log-file line format.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

mod ffi;

pub const CAPACITY: usize = 1024;

pub fn level_name(level: c_int) -> &'static CStr {
    match level {
        0 => c"info",
        1 => c"warn",
        2 => c"error",
        3 => c"render",
        4 => c"net",
        5 => c"js",
        _ => c"?",
    }
}

pub fn file_line(millis: i64, level: c_int, category: &[u8], message: &[u8]) -> Vec<u8> {
    let mut line = format!(
        "{} {:<5} ",
        millis,
        level_name(level).to_str().unwrap_or("?")
    )
    .into_bytes();
    line.extend_from_slice(category);
    line.extend_from_slice(b": ");
    line.extend_from_slice(message);
    line.push(b'\n');
    line
}
