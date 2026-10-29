//! Southstar — the per-user directories and preferred UI languages GLib works out for the platform, as owned bytes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

unsafe extern "C" {
    fn g_get_user_runtime_dir() -> *const c_char;
    fn g_get_home_dir() -> *const c_char;
    fn g_get_tmp_dir() -> *const c_char;
}

fn kept(dir: *const c_char) -> Vec<u8> {
    if dir.is_null() {
        return Vec::new();
    }
    // SAFETY: GLib's directory getters return null or a NUL-terminated string it keeps for the whole process.
    unsafe { CStr::from_ptr(dir) }.to_bytes().to_vec()
}

pub fn user_config_dir() -> Vec<u8> {
    // SAFETY: g_get_user_config_dir has no preconditions.
    kept(unsafe { crate::g_get_user_config_dir() })
}

pub fn user_data_dir() -> Vec<u8> {
    // SAFETY: g_get_user_data_dir has no preconditions.
    kept(unsafe { crate::g_get_user_data_dir() })
}

pub fn user_cache_dir() -> Vec<u8> {
    // SAFETY: g_get_user_cache_dir has no preconditions.
    kept(unsafe { crate::g_get_user_cache_dir() })
}

pub fn user_runtime_dir() -> Vec<u8> {
    // SAFETY: g_get_user_runtime_dir has no preconditions.
    kept(unsafe { g_get_user_runtime_dir() })
}

pub fn home_dir() -> Vec<u8> {
    // SAFETY: g_get_home_dir has no preconditions.
    kept(unsafe { g_get_home_dir() })
}

pub fn tmp_dir() -> Vec<u8> {
    // SAFETY: g_get_tmp_dir has no preconditions.
    kept(unsafe { g_get_tmp_dir() })
}

pub fn language_names() -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    // SAFETY: g_get_language_names has no preconditions and returns a null-terminated array GLib keeps.
    let mut entry = unsafe { crate::g_get_language_names() };
    if entry.is_null() {
        return names;
    }
    loop {
        // SAFETY: entry points into the array and has not passed its terminating null.
        let name = unsafe { *entry };
        if name.is_null() {
            return names;
        }
        names.push(kept(name));
        // SAFETY: the current entry was not the terminator, so the next one is still in the array.
        entry = unsafe { entry.add(1) };
    }
}
