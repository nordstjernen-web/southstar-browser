//! Southstar text — GLib's case mapping and character classes, and its script to ISO 15924 table, as shaping and itemization read them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr::NonNull;

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_unichar_islower(c: u32) -> c_int;
    fn g_unichar_isalnum(c: u32) -> c_int;
    fn g_utf8_strup(text: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_strdown(text: *const c_char, len: isize) -> *mut c_char;
    fn g_unicode_script_to_iso15924(script: c_int) -> u32;
    fn g_free(mem: *mut c_void);
}

pub(crate) fn unichar_islower(c: u32) -> bool {
    // SAFETY: g_unichar_islower is a pure table lookup defined for every u32.
    unsafe { g_unichar_islower(c) != 0 }
}

pub(crate) fn unichar_isalnum(c: u32) -> bool {
    // SAFETY: g_unichar_isalnum is a pure table lookup defined for every u32.
    unsafe { g_unichar_isalnum(c) != 0 }
}

pub(crate) fn unicode_script_to_iso15924(script: i32) -> u32 {
    // SAFETY: g_unicode_script_to_iso15924 is a pure table lookup defined for every int.
    unsafe { g_unicode_script_to_iso15924(script) }
}

fn take_glib_string(raw: *mut c_char) -> String {
    let Some(raw) = NonNull::new(raw) else {
        return String::new();
    };
    // SAFETY: GLib returned a NUL-terminated string that is ours until freed below.
    let text = unsafe { CStr::from_ptr(raw.as_ptr()) }
        .to_string_lossy()
        .into_owned();
    // SAFETY: the string was allocated by GLib and is freed exactly once.
    unsafe { g_free(raw.as_ptr().cast()) };
    text
}

pub(crate) fn utf8_strup(text: &str) -> String {
    // SAFETY: GLib reads exactly text.len() bytes of valid UTF-8 and returns a new string.
    take_glib_string(unsafe { g_utf8_strup(text.as_ptr().cast(), text.len() as isize) })
}

pub(crate) fn utf8_strdown(text: &str) -> String {
    // SAFETY: GLib reads exactly text.len() bytes of valid UTF-8 and returns a new string.
    take_glib_string(unsafe { g_utf8_strdown(text.as_ptr().cast(), text.len() as isize) })
}
