//! Southstar — the GLib regex call the form helpers make, character counting, and the urlencoded query they append to.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_long, c_uint, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean, GError};

unsafe extern "C" {
    fn g_regex_new(
        pattern: *const c_char,
        compile_options: c_uint,
        match_options: c_uint,
        error: *mut *mut GError,
    ) -> *mut c_void;
    fn g_regex_match(
        regex: *const c_void,
        string: *const c_char,
        match_options: c_uint,
        match_info: *mut *mut c_void,
    ) -> GBoolean;
    fn g_regex_unref(regex: *mut c_void);
}

pub struct Query {
    encoded: Vec<u8>,
    first: bool,
}

impl Query {
    pub fn new() -> Self {
        Self {
            encoded: Vec::new(),
            first: true,
        }
    }

    pub fn append(&mut self, name: &CStr, value: Option<&CStr>) {
        southstar_net::urlencoded_append_pair(
            &mut self.encoded,
            &mut self.first,
            name.to_bytes(),
            value.map(CStr::to_bytes),
        );
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.encoded
    }
}

pub fn utf8_strlen(s: &CStr) -> c_long {
    s.to_bytes()
        .iter()
        .filter(|byte| **byte & 0xC0 != 0x80)
        .count() as c_long
}

pub fn regex_matches(pattern: &CStr, value: &CStr) -> Option<bool> {
    let mut error = ptr::null_mut();
    let regex = unsafe { g_regex_new(pattern.as_ptr(), 0, 0, &mut error) };
    if regex.is_null() {
        if !error.is_null() {
            unsafe { glib::g_error_free(error) };
        }
        return None;
    }
    let matched = unsafe { g_regex_match(regex, value.as_ptr(), 0, ptr::null_mut()) != 0 };
    unsafe { g_regex_unref(regex) };
    Some(matched)
}
