//! Southstar — the files, folders and platform calls behind the safe-browsing blocklist.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
#[cfg(target_vendor = "apple")]
use core::ffi::c_char;
#[cfg(target_vendor = "apple")]
use core::ffi::c_int;
#[cfg(any(windows, target_vendor = "apple"))]
use core::ffi::c_void;
#[cfg(any(windows, target_vendor = "apple"))]
use core::ptr;
use std::ffi::CString;

use southstar_config::util;
use southstar_glib as glib;

#[cfg(windows)]
unsafe extern "system" {
    fn GetModuleFileNameW(module: *mut c_void, filename: *mut u16, size: u32) -> u32;
}

#[cfg(target_vendor = "apple")]
unsafe extern "C" {
    fn _NSGetExecutablePath(buf: *mut c_char, bufsize: *mut u32) -> c_int;
    fn realpath(path: *const c_char, resolved: *mut c_char) -> *mut c_char;
    fn free(p: *mut c_void);
}

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn getenv(name: &CStr) -> Option<CString> {
    util::env_bytes(name.to_str().ok()?).map(|value| c_string(&value))
}

pub(crate) fn user_config_dir() -> CString {
    // SAFETY: g_get_user_config_dir has no preconditions and returns a string GLib owns.
    let dir = unsafe { glib::g_get_user_config_dir() };
    // SAFETY: dir is NULL or the NUL-terminated string GLib keeps for the life of the process.
    let dir = unsafe { glib::bytes(dir) };
    c_string(dir.unwrap_or_default())
}

pub(crate) fn build_filename(parts: &[&CStr]) -> CString {
    let parts: Vec<&[u8]> = parts.iter().map(|part| part.to_bytes()).collect();
    c_string(&util::build_filename(&parts))
}

fn path_get_dirname(path: &[u8]) -> CString {
    c_string(&util::path_dirname(path))
}

pub(crate) fn exists(path: &CStr) -> bool {
    util::os_path(path.to_bytes()).is_some_and(|path| path.exists())
}

pub(crate) fn file_text(path: &CStr) -> Option<Vec<u8>> {
    let contents = std::fs::read(util::os_path(path.to_bytes())?).ok()?;
    Some(c_string(&contents).into_bytes())
}

pub(crate) fn markup_escape(text: &CStr) -> Vec<u8> {
    util::markup_escape(text.to_bytes())
}

#[cfg(windows)]
pub(crate) fn self_exe_dir() -> Option<CString> {
    let mut cap: u32 = 260;
    let mut buf: Vec<u16> = vec![0; cap as usize];
    let mut n = unsafe { GetModuleFileNameW(ptr::null_mut(), buf.as_mut_ptr(), cap) };
    while n >= cap && cap < 32768 {
        cap *= 2;
        buf.resize(cap as usize, 0);
        n = unsafe { GetModuleFileNameW(ptr::null_mut(), buf.as_mut_ptr(), cap) };
    }
    if n == 0 || n >= cap {
        return None;
    }
    let end = buf.iter().position(|&unit| unit == 0).unwrap_or(buf.len());
    let exe = String::from_utf16(&buf[..end]).ok()?;
    Some(path_get_dirname(exe.as_bytes()))
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
