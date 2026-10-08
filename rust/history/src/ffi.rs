//! Southstar — the C ABI of browsing history, as declared in src/history.h, and the SQLite and GLib calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};

#[repr(C)]
struct GDateTime {
    _private: [u8; 0],
}

#[repr(C)]
struct GUri {
    _private: [u8; 0],
}

const LOG_LEVEL_WARNING: c_int = 1 << 4;
const URI_FLAGS_NONE: c_int = 0;

unsafe extern "C" {
    fn g_build_filename(first_element: *const c_char, ...) -> *mut c_char;
    fn g_mkdir_with_parents(pathname: *const c_char, mode: c_int) -> c_int;
    fn g_chmod(filename: *const c_char, mode: c_int) -> c_int;
    fn g_log(log_domain: *const c_char, log_level: c_int, format: *const c_char, ...);
    fn g_markup_escape_text(text: *const c_char, length: isize) -> *mut c_char;
    fn g_utf8_make_valid(text: *const c_char, length: isize) -> *mut c_char;
    fn g_utf8_get_char_validated(p: *const c_char, max_len: isize) -> u32;
    fn g_unichar_isalnum(c: u32) -> GBoolean;
    fn g_unichar_to_utf8(c: u32, outbuf: *mut c_char) -> c_int;
    fn g_uri_parse(uri_string: *const c_char, flags: c_int, error: *mut *mut c_void) -> *mut GUri;
    fn g_uri_get_host(uri: *mut GUri) -> *const c_char;
    fn g_uri_unref(uri: *mut GUri);
    fn g_date_time_new_now_local() -> *mut GDateTime;
    fn g_date_time_new_from_unix_local(t: i64) -> *mut GDateTime;
    fn g_date_time_get_day_of_year(datetime: *mut GDateTime) -> c_int;
    fn g_date_time_get_year(datetime: *mut GDateTime) -> c_int;
    fn g_date_time_format(datetime: *mut GDateTime, format: *const c_char) -> *mut c_char;
    fn g_date_time_unref(datetime: *mut GDateTime);
}

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

unsafe fn take(p: *mut c_char) -> Option<Vec<u8>> {
    let bytes = unsafe { glib::bytes(p) }?.to_vec();
    unsafe { glib::g_free(p.cast()) };
    Some(bytes)
}

unsafe fn borrowed(p: *const c_char) -> Vec<u8> {
    unsafe { glib::bytes(p) }.unwrap_or_default().to_vec()
}

pub(crate) fn warn(message: &[u8]) {
    let message = c_string(message);
    unsafe {
        g_log(
            ptr::null(),
            LOG_LEVEL_WARNING,
            c"%s".as_ptr(),
            message.as_ptr(),
        )
    };
}

pub(crate) fn build_filename(dir: &CStr, name: &CStr) -> CString {
    unsafe {
        let path = g_build_filename(dir.as_ptr(), name.as_ptr(), ptr::null::<c_char>());
        c_string(&take(path).unwrap_or_default())
    }
}

pub(crate) fn user_data_dir() -> CString {
    c_string(&unsafe { borrowed(glib::g_get_user_data_dir()) })
}

pub(crate) fn mkdir_with_parents(path: &CStr, mode: c_int) {
    unsafe { g_mkdir_with_parents(path.as_ptr(), mode) };
}

pub(crate) fn chmod(path: &CStr, mode: c_int) {
    unsafe { g_chmod(path.as_ptr(), mode) };
}

pub(crate) fn markup_escape(text: &[u8]) -> Vec<u8> {
    let text = c_string(text);
    unsafe { take(g_markup_escape_text(text.as_ptr(), -1)) }.unwrap_or_default()
}

pub(crate) fn utf8_make_valid(text: &[u8]) -> Vec<u8> {
    let text = c_string(text);
    unsafe { take(g_utf8_make_valid(text.as_ptr(), -1)) }.unwrap_or_default()
}

pub(crate) fn first_char(text: &[u8]) -> Option<u32> {
    let text = c_string(text);
    let c = unsafe { g_utf8_get_char_validated(text.as_ptr(), -1) };
    (c != u32::MAX && c != u32::MAX - 1).then_some(c)
}

pub(crate) fn unichar_isalnum(c: u32) -> bool {
    unsafe { g_unichar_isalnum(c) != glib::FALSE }
}

pub(crate) fn unichar_to_utf8(c: u32) -> Vec<u8> {
    let mut out = [0u8; 8];
    let n = unsafe { g_unichar_to_utf8(c, out.as_mut_ptr().cast()) };
    out[..n.clamp(0, 6) as usize].to_vec()
}

pub(crate) fn uri_host(url: &[u8]) -> Option<Vec<u8>> {
    let url = c_string(url);
    unsafe {
        let uri = g_uri_parse(url.as_ptr(), URI_FLAGS_NONE, ptr::null_mut());
        if uri.is_null() {
            return None;
        }
        let host = glib::bytes(g_uri_get_host(uri)).map(<[u8]>::to_vec);
        g_uri_unref(uri);
        host
    }
}

pub(crate) struct LocalTime(*mut GDateTime);

impl LocalTime {
    pub(crate) fn now() -> LocalTime {
        LocalTime(unsafe { g_date_time_new_now_local() })
    }

    pub(crate) fn from_unix(t: i64) -> Option<LocalTime> {
        let dt = unsafe { g_date_time_new_from_unix_local(t) };
        (!dt.is_null()).then_some(LocalTime(dt))
    }

    pub(crate) fn day_of_year(&self) -> i32 {
        unsafe { g_date_time_get_day_of_year(self.0) }
    }

    pub(crate) fn year(&self) -> i32 {
        unsafe { g_date_time_get_year(self.0) }
    }

    pub(crate) fn format(&self, format: &CStr) -> Option<Vec<u8>> {
        unsafe { take(g_date_time_format(self.0, format.as_ptr())) }
    }
}

impl Drop for LocalTime {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { g_date_time_unref(self.0) };
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_history_init() {
    crate::init();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_history_shutdown() {
    crate::shutdown();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_history_record(url: *const c_char, title: *const c_char) {
    let url = (!url.is_null()).then(|| unsafe { CStr::from_ptr(url) });
    let title = (!title.is_null()).then(|| unsafe { CStr::from_ptr(title) });
    crate::record(url, title);
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_history_clear() {
    crate::clear();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_history_html_page() -> *mut c_char {
    glib::strdup(&crate::html_page())
}
