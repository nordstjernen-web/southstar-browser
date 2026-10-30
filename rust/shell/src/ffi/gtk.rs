//! Southstar — small helpers over the GTK calls: signal connection, translated strings, C string conversion.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_ulong, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_config::util;
use southstar_glib::GStr;
use southstar_gtk_ffi::{self as gtk, GCallback};

macro_rules! handler {
    ($f:expr) => {{
        #[allow(unused_unsafe)]
        // SAFETY: `$f` is an `extern "C"` function; GLib casts the callback back to the signal's signature before calling it.
        let callback = unsafe {
            core::mem::transmute::<*const (), southstar_gtk_ffi::GCallback>($f as *const ())
        };
        callback
    }};
}

pub(crate) use handler;

/// # Safety
///
/// `instance` must be a live GObject that has `signal`, `handler` must be a callback with that
/// signal's signature, and `data` must stay valid for as long as the handler can run.
pub unsafe fn connect<T>(
    instance: *mut T,
    signal: &CStr,
    handler: GCallback,
    data: *mut c_void,
) -> c_ulong {
    // SAFETY: the caller guarantees the instance, signal signature and data lifetime, and
    // `signal` is a NUL-terminated string that outlives the call.
    unsafe { gtk::g_signal_connect_data(instance.cast(), signal.as_ptr(), handler, data, None, 0) }
}

pub fn tr(text: &CStr) -> &CStr {
    southstar_i18n::translate(text).unwrap_or(text)
}

pub fn tr_string(text: &CStr) -> String {
    tr(text).to_string_lossy().into_owned()
}

pub fn cstring(text: impl Into<Vec<u8>>) -> CString {
    let mut bytes = text.into();
    bytes.retain(|&b| b != 0);
    CString::new(bytes).unwrap_or_default()
}

/// # Safety
///
/// `p` must be null or a NUL-terminated string that stays valid and unchanged for `'a`.
pub unsafe fn borrowed<'a>(p: *const c_char) -> Option<&'a CStr> {
    // SAFETY: the caller guarantees a non-null `p` is a NUL-terminated string living for `'a`.
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

pub fn build_filename(parts: &[&CStr]) -> CString {
    let parts: Vec<&[u8]> = parts.iter().map(|part| part.to_bytes()).collect();
    cstring(util::build_filename(&parts))
}

pub fn basename(path: &CStr) -> CString {
    // SAFETY: `path` is a NUL-terminated string that outlives the call.
    let base = unsafe { gtk::g_path_get_basename(path.as_ptr()) };
    // SAFETY: g_path_get_basename hands over a newly allocated string, taken exactly once.
    unsafe { GStr::take(base) }.map_or_else(CString::default, |base| CStr::to_owned(&base))
}

pub fn file_test(path: &CStr, test: core::ffi::c_uint) -> bool {
    let Some(path) = util::os_path(path.to_bytes()) else {
        return false;
    };
    (test & gtk::FILE_TEST_EXISTS != 0 && path.exists())
        || (test & gtk::FILE_TEST_IS_DIR != 0 && path.is_dir())
        || (test & gtk::FILE_TEST_IS_REGULAR != 0 && path.is_file())
}

pub fn open_path(path: &CStr) {
    let Some(uri) = southstar_netutil::file_uri::filename_to_uri(path.to_bytes()) else {
        return;
    };
    let uri = cstring(uri);
    // SAFETY: `uri` is a NUL-terminated string that outlives the call; the launch context and
    // error out-pointer may be null.
    unsafe {
        gtk::g_app_info_launch_default_for_uri(uri.as_ptr(), ptr::null_mut(), ptr::null_mut())
    };
}
