//! Southstar — the GLib conversion and uchardet calls behind the HTML helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};

use southstar_glib as glib;

unsafe extern "C" {
    safe fn uchardet_new() -> *mut c_void;
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

struct Detector(*mut c_void);

impl Detector {
    fn new() -> Option<Detector> {
        let detector = uchardet_new();
        (!detector.is_null()).then_some(Detector(detector))
    }

    fn feed(&mut self, data: &[u8]) -> bool {
        // SAFETY: the detector is live and `data` is `data.len()` readable bytes.
        unsafe { uchardet_handle_data(self.0, data.as_ptr().cast(), data.len()) == 0 }
    }

    fn finish(&mut self) -> Vec<u8> {
        // SAFETY: the detector is live.
        unsafe { uchardet_data_end(self.0) };
        // SAFETY: the detector is live; it returns a charset name it owns.
        let name = unsafe { uchardet_get_charset(self.0) };
        // SAFETY: the name is null or NUL-terminated and lives as long as the detector, which
        // outlives this copy.
        unsafe { glib::bytes(name) }.map_or_else(Vec::new, <[u8]>::to_vec)
    }
}

impl Drop for Detector {
    fn drop(&mut self) {
        // SAFETY: the detector is the non-null one `new` created, deleted exactly once, here.
        unsafe { uchardet_delete(self.0) };
    }
}

pub(crate) fn detect_charset(sample: &[u8]) -> Option<Vec<u8>> {
    let mut detector = Detector::new()?;
    if !detector.feed(sample) {
        return None;
    }
    let name = detector.finish();
    (!name.is_empty()
        && !name.eq_ignore_ascii_case(b"ASCII")
        && !name.eq_ignore_ascii_case(b"UTF-8"))
    .then_some(name)
}
