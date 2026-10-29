//! Southstar — the per-user directories and preferred UI languages GLib works out for the platform, as owned bytes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

const USER_DIRECTORY_DOWNLOAD: c_int = 2;

unsafe extern "C" {
    fn g_get_user_runtime_dir() -> *const c_char;
    fn g_get_home_dir() -> *const c_char;
    fn g_get_tmp_dir() -> *const c_char;
    fn g_get_user_special_dir(directory: c_int) -> *const c_char;
    fn g_get_system_data_dirs() -> *const *const c_char;
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

pub fn download_dir() -> Vec<u8> {
    // SAFETY: g_get_user_special_dir takes any GUserDirectory value and returns null or a string GLib keeps.
    kept(unsafe { g_get_user_special_dir(USER_DIRECTORY_DOWNLOAD) })
}

pub fn system_data_dirs() -> Vec<Vec<u8>> {
    // SAFETY: g_get_system_data_dirs has no preconditions and returns a null-terminated array GLib keeps.
    kept_list(unsafe { g_get_system_data_dirs() })
}

pub fn language_names() -> Vec<Vec<u8>> {
    // SAFETY: g_get_language_names has no preconditions and returns a null-terminated array GLib keeps.
    kept_list(unsafe { crate::g_get_language_names() })
}

fn kept_list(mut entry: *const *const c_char) -> Vec<Vec<u8>> {
    let mut names = Vec::new();
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
