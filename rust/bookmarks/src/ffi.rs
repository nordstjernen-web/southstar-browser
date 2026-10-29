//! Southstar — the GLib file calls behind bookmarks storage: locating, reading and atomically writing the bookmarks file.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, FALSE, GBoolean, GError};

const APP_DIR_NAME: &CStr = c"southstar";
const BOOKMARKS_FILE: &CStr = c"bookmarks.txt";
const SET_CONTENTS_CONSISTENT: c_int = 1;

unsafe extern "C" {
    fn g_build_filename(first_element: *const c_char, ...) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_file_set_contents_full(
        filename: *const c_char,
        contents: *const c_char,
        length: isize,
        flags: c_int,
        mode: c_int,
        error: *mut *mut GError,
    ) -> GBoolean;
}

fn take_path(path: *mut c_char) -> CString {
    // SAFETY: g_build_filename returns a newly allocated NUL-terminated string.
    let owned = unsafe { CStr::from_ptr(path) }.to_owned();
    // SAFETY: the string was allocated by GLib and is not used after this.
    unsafe { glib::g_free(path.cast()) };
    owned
}

fn join(dir: *const c_char, name: &CStr) -> CString {
    // SAFETY: both elements are NUL-terminated and the list ends with a null pointer.
    take_path(unsafe { g_build_filename(dir, name.as_ptr(), ptr::null::<c_char>()) })
}

pub fn bookmarks_path() -> CString {
    // SAFETY: g_get_user_config_dir has no preconditions and returns a string GLib keeps.
    let dir = join(unsafe { glib::g_get_user_config_dir() }, APP_DIR_NAME);
    // SAFETY: dir is a NUL-terminated path.
    unsafe { g_mkdir_with_parents(dir.as_ptr(), 0o700) };
    join(dir.as_ptr(), BOOKMARKS_FILE)
}

pub fn read(path: &CStr) -> Option<Vec<u8>> {
    let mut contents: *mut c_char = ptr::null_mut();
    let mut len = 0usize;
    // SAFETY: path is NUL-terminated, contents and len are writable locals and no error is wanted.
    let ok = unsafe {
        glib::g_file_get_contents(path.as_ptr(), &mut contents, &mut len, ptr::null_mut())
    };
    if ok == FALSE || contents.is_null() {
        return None;
    }
    // SAFETY: g_file_get_contents stored len readable bytes at contents.
    let bytes = unsafe { glib::slice(contents.cast_const().cast(), len) }.to_vec();
    // SAFETY: contents was allocated by g_file_get_contents and is not used after this.
    unsafe { glib::g_free(contents.cast()) };
    Some(bytes)
}

pub fn write(path: &CStr, contents: &[u8]) {
    let mut err: *mut GError = ptr::null_mut();
    // SAFETY: path is NUL-terminated, contents holds contents.len() bytes and err is a writable local.
    let ok = unsafe {
        g_file_set_contents_full(
            path.as_ptr(),
            contents.as_ptr().cast(),
            contents.len() as isize,
            SET_CONTENTS_CONSISTENT,
            0o600,
            &mut err,
        )
    };
    if ok != FALSE || err.is_null() {
        return;
    }
    // SAFETY: err is the GError the failed call set.
    let message = unsafe { (*err).message };
    // SAFETY: a GError's message is a NUL-terminated string that lives as long as the error.
    let message = unsafe { glib::bytes(message) }.unwrap_or_default();
    let mut text = b"bookmarks: failed to write ".to_vec();
    text.extend_from_slice(path.to_bytes());
    text.extend_from_slice(b": ");
    text.extend_from_slice(message);
    glib::warning(text);
    // SAFETY: err was set by the call above and is freed once.
    unsafe { glib::g_error_free(err) };
}
