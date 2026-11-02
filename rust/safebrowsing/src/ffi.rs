//! Southstar — the files, folders and platform calls behind the safe-browsing blocklist.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
#[cfg(target_vendor = "apple")]
use core::ffi::c_char;
#[cfg(target_vendor = "apple")]
use core::ffi::c_int;
#[cfg(target_vendor = "apple")]
use core::ffi::c_void;
#[cfg(target_vendor = "apple")]
use core::ptr;
use southstar_unicode::c_string_until_nul;
use std::ffi::CString;

use southstar_config::util;
use southstar_glib as glib;

#[cfg(target_vendor = "apple")]
unsafe extern "C" {
    fn _NSGetExecutablePath(buf: *mut c_char, bufsize: *mut u32) -> c_int;
    fn realpath(path: *const c_char, resolved: *mut c_char) -> *mut c_char;
    fn free(p: *mut c_void);
}

pub(crate) fn getenv(name: &CStr) -> Option<CString> {
    util::env_bytes(name.to_str().ok()?).map(|value| c_string_until_nul(&value))
}

pub(crate) fn user_config_dir() -> CString {
    c_string_until_nul(&glib::user_config_dir())
}

pub(crate) fn build_filename(parts: &[&CStr]) -> CString {
    let parts: Vec<&[u8]> = parts.iter().map(|part| part.to_bytes()).collect();
    c_string_until_nul(&util::build_filename(&parts))
}

fn path_get_dirname(path: &[u8]) -> CString {
    c_string_until_nul(&util::path_dirname(path))
}

pub(crate) fn exists(path: &CStr) -> bool {
    util::os_path(path.to_bytes()).is_some_and(|path| path.exists())
}

pub(crate) fn file_text(path: &CStr) -> Option<Vec<u8>> {
    let contents = std::fs::read(util::os_path(path.to_bytes())?).ok()?;
    Some(c_string_until_nul(&contents).into_bytes())
}

pub(crate) fn markup_escape(text: &CStr) -> Vec<u8> {
    util::markup_escape(text.to_bytes())
}

#[cfg(windows)]
pub(crate) fn self_exe_dir() -> Option<CString> {
    let exe = std::env::current_exe().ok()?;
    Some(path_get_dirname(exe.to_str()?.as_bytes()))
}

#[cfg(target_vendor = "apple")]
pub(crate) fn self_exe_dir() -> Option<CString> {
    let mut size: u32 = 0;
    unsafe { _NSGetExecutablePath(ptr::null_mut(), &mut size) };
    if size == 0 || size > 32768 {
        return None;
    }
    let mut raw: Vec<c_char> = vec![0; size as usize];
    if unsafe { _NSGetExecutablePath(raw.as_mut_ptr(), &mut size) } != 0 {
        return None;
    }
    let real = unsafe { realpath(raw.as_ptr(), ptr::null_mut()) };
    let path = if real.is_null() { raw.as_ptr() } else { real };
    let dir = path_get_dirname(unsafe { CStr::from_ptr(path) }.to_bytes());
    unsafe { free(real.cast()) };
    Some(dir)
}

#[cfg(not(any(windows, target_vendor = "apple")))]
pub(crate) fn self_exe_dir() -> Option<CString> {
    let exe = std::fs::read_link("/proc/self/exe").ok()?;
    Some(path_get_dirname(&util::path_bytes(&exe)))
}
