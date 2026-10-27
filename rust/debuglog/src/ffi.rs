//! Southstar — the GLib calls behind the debug log: clocks, the environment and the log file's location.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use southstar_glib as glib;

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
}

pub fn monotonic_us() -> i64 {
    // SAFETY: g_get_monotonic_time has no preconditions.
    unsafe { g_get_monotonic_time() }
}

pub fn real_time_us() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or_else(
        |before| -i64::try_from(before.duration().as_micros()).unwrap_or(i64::MAX),
        |after| i64::try_from(after.as_micros()).unwrap_or(i64::MAX),
    )
}

pub fn env(name: &str) -> Option<Vec<u8>> {
    glib::env_bytes(name)
}

pub fn default_log_path() -> Option<CString> {
    // SAFETY: g_get_user_data_dir has no preconditions and returns a string GLib keeps.
    let data_dir = unsafe { glib::g_get_user_data_dir() };
    // SAFETY: data_dir is null or a NUL-terminated string GLib keeps for the process.
    let data_dir = unsafe { glib::bytes(data_dir) }?;
    let path = glib::path_from_bytes(data_dir)?
        .join("Southstar")
        .join("southstar-debug.log");
    CString::new(glib::path_bytes(&path)).ok()
}

pub fn make_parent_dir(path: &CStr) {
    let Some(parent) = glib::path_from_bytes(path.to_bytes())
        .as_deref()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
    else {
        return;
    };
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    let _ = builder.create(parent);
}
