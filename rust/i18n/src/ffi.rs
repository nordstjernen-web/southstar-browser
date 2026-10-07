//! Southstar — the C ABI of UI string translation, as declared in src/i18n.h, and the GLib calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;
use std::ffi::CString;

use southstar_glib as glib;

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

pub(crate) fn language_names() -> Vec<CString> {
    let names = unsafe { glib::g_get_language_names() };
    if names.is_null() {
        return Vec::new();
    }
    (0..)
        .map_while(|i| unsafe { borrow(*names.add(i)) })
        .map(CStr::to_owned)
        .collect()
}

pub(crate) fn build_filename(parts: &[&CStr]) -> CString {
    let mut args: Vec<*mut c_char> = parts.iter().map(|p| p.as_ptr().cast_mut()).collect();
    args.push(ptr::null_mut());
    unsafe { take(glib::g_build_filenamev(args.as_mut_ptr())) }
}

pub(crate) fn path_get_dirname(path: &CStr) -> CString {
    unsafe { take(glib::g_path_get_dirname(path.as_ptr())) }
}

pub(crate) fn is_regular_file(path: &CStr) -> bool {
    unsafe { glib::g_file_test(path.as_ptr(), glib::FILE_TEST_IS_REGULAR) != glib::FALSE }
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_i18n_init(self_exe: *const c_char) {
    crate::init(unsafe { borrow(self_exe) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_i18n(text: *const c_char) -> *const c_char {
    match unsafe { borrow(text) }.and_then(crate::translate) {
        Some(hit) => hit.as_ptr(),
        None => text,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_i18n_language() -> *const c_char {
    crate::language().map_or(ptr::null(), CStr::as_ptr)
}
