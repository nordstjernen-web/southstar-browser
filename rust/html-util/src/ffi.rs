//! Southstar — the GLib conversion and uchardet calls behind the HTML helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};

use southstar_glib as glib;

unsafe extern "C" {
    fn uchardet_new() -> *mut c_void;
    fn uchardet_delete(ud: *mut c_void);
    fn uchardet_handle_data(ud: *mut c_void, data: *const c_char, len: usize) -> c_int;
    fn uchardet_data_end(ud: *mut c_void);
    fn uchardet_get_charset(ud: *const c_void) -> *const c_char;
}

#[cfg(windows)]
const DIR_SEPARATOR: u8 = b'\\';
#[cfg(not(windows))]
const DIR_SEPARATOR: u8 = b'/';

fn is_dir_separator(c: u8) -> bool {
    c == b'/' || (cfg!(windows) && c == b'\\')
}

pub(crate) fn ascii_strtod(text: &[u8]) -> f64 {
    glib::ascii_strtod(text)
}

pub(crate) fn path_basename(path: &[u8]) -> Vec<u8> {
    let path = crate::until_nul(path);
    if path.is_empty() {
        return b".".to_vec();
    }
    let Some(last) = path.iter().rposition(|&c| !is_dir_separator(c)) else {
        return vec![DIR_SEPARATOR];
    };
    let has_drive =
        cfg!(windows) && path.len() >= 2 && path[0].is_ascii_alphabetic() && path[1] == b':';
    if has_drive && last == 1 {
        return vec![DIR_SEPARATOR];
    }
    let base = match path[..=last].iter().rposition(|&c| is_dir_separator(c)) {
        Some(i) => i + 1,
        None if has_drive => 2,
        None => 0,
    };
    path[base..=last].to_vec()
}

pub(crate) fn utf8_make_valid(text: &[u8]) -> Vec<u8> {
    String::from_utf8_lossy(text).into_owned().into_bytes()
}

pub(crate) fn utf8_validate(text: &[u8]) -> bool {
    !text.contains(&0) && core::str::from_utf8(text).is_ok()
}

pub(crate) fn convert(text: &[u8], from: &[u8]) -> Option<Vec<u8>> {
    glib::convert(text, b"UTF-8", from)
}

pub(crate) fn detect_charset(sample: &[u8]) -> Option<Vec<u8>> {
    let detector = unsafe { uchardet_new() };
    if detector.is_null() {
        return None;
    }
    let mut charset = None;
    if unsafe { uchardet_handle_data(detector, sample.as_ptr().cast(), sample.len()) } == 0 {
        unsafe { uchardet_data_end(detector) };
        let name = unsafe { glib::bytes(uchardet_get_charset(detector)) }.unwrap_or_default();
        if !name.is_empty()
            && !name.eq_ignore_ascii_case(b"ASCII")
            && !name.eq_ignore_ascii_case(b"UTF-8")
        {
            charset = Some(name.to_vec());
        }
    }
    unsafe { uchardet_delete(detector) };
    charset
}
