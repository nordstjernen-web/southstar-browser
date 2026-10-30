//! Southstar — the embedding API's history call, and the local times, folders and logging behind browsing history.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use std::ffi::CString;

use southstar_config::util;
use southstar_glib as glib;

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn warn(message: &[u8]) {
    glib::warning(message);
}

pub(crate) fn build_filename(dir: &CStr, name: &CStr) -> CString {
    c_string(&util::build_filename(&[dir.to_bytes(), name.to_bytes()]))
}

pub(crate) fn user_data_dir() -> CString {
    c_string(&glib::user_data_dir())
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

/// # Safety
///
/// `url` and `title` must each be null or a NUL-terminated string valid for the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_history_record(url: *const c_char, title: *const c_char) {
    // SAFETY: the caller passes null or a NUL-terminated string valid for the call.
    let url = (!url.is_null()).then(|| unsafe { CStr::from_ptr(url) });
    // SAFETY: the caller passes null or a NUL-terminated string valid for the call.
    let title = (!title.is_null()).then(|| unsafe { CStr::from_ptr(title) });
    crate::record(url, title);
}
