//! Southstar — the embedding API's history call, and the local times, folders and logging behind browsing history.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use std::ffi::CString;

use southstar_config::util;
use southstar_glib as glib;

#[repr(C)]
struct GDateTime {
    _private: [u8; 0],
}

unsafe extern "C" {
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
    glib::warning(message);
}

pub(crate) fn build_filename(dir: &CStr, name: &CStr) -> CString {
    c_string(&util::build_filename(&[dir.to_bytes(), name.to_bytes()]))
}

pub(crate) fn user_data_dir() -> CString {
    // SAFETY: g_get_user_data_dir has no preconditions and returns a string GLib owns.
    let dir = unsafe { glib::g_get_user_data_dir() };
    // SAFETY: dir is NULL or the NUL-terminated string GLib keeps for the life of the process.
    c_string(&unsafe { borrowed(dir) })
}

pub(crate) fn mkdir_with_parents(path: &CStr, mode: u32) {
    if let Some(path) = util::os_path(path.to_bytes()) {
        let _ = util::make_dirs(&path, mode);
    }
}

pub(crate) fn chmod(path: &CStr, mode: u32) {
    if let Some(path) = util::os_path(path.to_bytes()) {
        let _ = util::set_mode(&path, mode);
    }
}

pub(crate) fn markup_escape(text: &[u8]) -> Vec<u8> {
    util::markup_escape(text)
}

pub(crate) fn utf8_make_valid(text: &[u8]) -> Vec<u8> {
    util::utf8_make_valid(text)
}

pub(crate) fn first_char(text: &[u8]) -> Option<char> {
    let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
    text[..end]
        .utf8_chunks()
        .next()
        .and_then(|chunk| chunk.valid().chars().next())
}

pub(crate) fn unichar_isalnum(c: char) -> bool {
    southstar_unicode::is_alnum(u32::from(c))
}

pub(crate) fn unichar_to_utf8(c: char) -> Vec<u8> {
    c.encode_utf8(&mut [0; 4]).as_bytes().to_vec()
}

pub(crate) fn uri_host(url: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::host_from(url)
}

pub(crate) struct LocalTime(*mut GDateTime);

impl LocalTime {
    pub(crate) fn now() -> LocalTime {
        LocalTime(unsafe { g_date_time_new_now_local() })
    }

    pub(crate) fn from_unix(t: i64) -> Option<LocalTime> {
        let dt = unsafe { g_date_time_new_from_unix_local(t) };
        (!dt.is_null()).then(|| LocalTime(dt))
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
pub unsafe extern "C" fn ns_history_record(url: *const c_char, title: *const c_char) {
    let url = (!url.is_null()).then(|| unsafe { CStr::from_ptr(url) });
    let title = (!title.is_null()).then(|| unsafe { CStr::from_ptr(title) });
    crate::record(url, title);
}
