//! Southstar — the C ABI of the safe-browsing blocklist, and the GLib and platform calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(target_vendor = "apple")]
use core::ffi::c_int;
#[cfg(any(windows, target_vendor = "apple"))]
use core::ffi::c_void;
use core::ffi::{CStr, c_char};
use core::ptr;
use std::ffi::CString;

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

unsafe fn borrow<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

unsafe fn take(p: *mut c_char) -> CString {
    let Some(owned) = (unsafe { borrow(p) }).map(CStr::to_owned) else {
        return CString::default();
    };
    unsafe { glib::g_free(p.cast()) };
    owned
}

pub(crate) fn getenv(name: &CStr) -> Option<CString> {
    unsafe { borrow(glib::g_getenv(name.as_ptr())) }.map(CStr::to_owned)
}

pub(crate) fn user_config_dir() -> CString {
    unsafe { borrow(glib::g_get_user_config_dir()) }
        .map(CStr::to_owned)
        .unwrap_or_default()
}

pub(crate) fn build_filename(parts: &[&CStr]) -> CString {
    let mut args: Vec<*mut c_char> = parts.iter().map(|p| p.as_ptr().cast_mut()).collect();
    args.push(ptr::null_mut());
    unsafe { take(glib::g_build_filenamev(args.as_mut_ptr())) }
}

fn path_get_dirname(path: &CStr) -> CString {
    unsafe { take(glib::g_path_get_dirname(path.as_ptr())) }
}

pub(crate) fn exists(path: &CStr) -> bool {
    unsafe { glib::g_file_test(path.as_ptr(), glib::FILE_TEST_EXISTS) != glib::FALSE }
}

pub(crate) fn file_text(path: &CStr) -> Option<Vec<u8>> {
    let mut contents: *mut c_char = ptr::null_mut();
    let loaded = unsafe {
        glib::g_file_get_contents(
            path.as_ptr(),
            &mut contents,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    (loaded != glib::FALSE).then(|| unsafe { take(contents) }.into_bytes())
}

pub(crate) fn markup_escape(text: &CStr) -> Vec<u8> {
    unsafe { take(glib::g_markup_escape_text(text.as_ptr(), -1)) }.into_bytes()
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
    let utf8 = unsafe {
        glib::g_utf16_to_utf8(
            buf.as_ptr(),
            -1,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    (!utf8.is_null()).then(|| path_get_dirname(&unsafe { take(utf8) }))
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
    let dir = path_get_dirname(unsafe { CStr::from_ptr(path) });
    unsafe { free(real.cast()) };
    Some(dir)
}

#[cfg(not(any(windows, target_vendor = "apple")))]
pub(crate) fn self_exe_dir() -> Option<CString> {
    let exe = unsafe { glib::g_file_read_link(c"/proc/self/exe".as_ptr(), ptr::null_mut()) };
    (!exe.is_null()).then(|| path_get_dirname(&unsafe { take(exe) }))
}
