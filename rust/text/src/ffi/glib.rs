//! Southstar text — GLib's Unicode character database: general category, line break class, script, mirroring and case.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::c_int;

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_unichar_type(c: u32) -> c_int;
    fn g_unichar_break_type(c: u32) -> c_int;
    fn g_unichar_get_script(c: u32) -> c_int;
    fn g_unichar_get_mirror_char(c: u32, mirrored: *mut u32) -> c_int;
    fn g_unichar_iszerowidth(c: u32) -> c_int;
    fn g_unichar_isspace(c: u32) -> c_int;
    fn g_unichar_ismark(c: u32) -> c_int;
    fn g_unichar_iswide(c: u32) -> c_int;
    fn g_unichar_combining_class(c: u32) -> c_int;
    fn g_unichar_tolower(c: u32) -> u32;
    fn g_unichar_toupper(c: u32) -> u32;
    fn g_unichar_totitle(c: u32) -> u32;
}

pub(crate) fn unichar_type(c: u32) -> i32 {
    // SAFETY: g_unichar_type is a pure table lookup defined for every u32.
    unsafe { g_unichar_type(c) }
}

pub(crate) fn unichar_break_type(c: u32) -> i32 {
    // SAFETY: g_unichar_break_type is a pure table lookup defined for every u32.
    unsafe { g_unichar_break_type(c) }
}

pub(crate) fn unichar_get_script(c: u32) -> i32 {
    // SAFETY: g_unichar_get_script is a pure table lookup defined for every u32.
    unsafe { g_unichar_get_script(c) }
}

pub(crate) fn unichar_get_mirror_char(c: u32) -> Option<u32> {
    let mut mirrored = c;
    // SAFETY: `mirrored` is a valid, writable u32 for the duration of the call.
    let found = unsafe { g_unichar_get_mirror_char(c, &mut mirrored) };
    (found != 0).then_some(mirrored)
}

pub(crate) fn unichar_iszerowidth(c: u32) -> bool {
    // SAFETY: g_unichar_iszerowidth is a pure table lookup defined for every u32.
    unsafe { g_unichar_iszerowidth(c) != 0 }
}

pub(crate) fn unichar_isspace(c: u32) -> bool {
    // SAFETY: g_unichar_isspace is a pure table lookup defined for every u32.
    unsafe { g_unichar_isspace(c) != 0 }
}

pub(crate) fn unichar_ismark(c: u32) -> bool {
    // SAFETY: g_unichar_ismark is a pure table lookup defined for every u32.
    unsafe { g_unichar_ismark(c) != 0 }
}

pub(crate) fn unichar_iswide(c: u32) -> bool {
    // SAFETY: g_unichar_iswide is a pure table lookup defined for every u32.
    unsafe { g_unichar_iswide(c) != 0 }
}

pub(crate) fn unichar_combining_class(c: u32) -> i32 {
    // SAFETY: g_unichar_combining_class is a pure table lookup defined for every u32.
    unsafe { g_unichar_combining_class(c) }
}

pub(crate) fn unichar_tolower(c: u32) -> u32 {
    // SAFETY: g_unichar_tolower is a pure table lookup defined for every u32.
    unsafe { g_unichar_tolower(c) }
}

pub(crate) fn unichar_toupper(c: u32) -> u32 {
    // SAFETY: g_unichar_toupper is a pure table lookup defined for every u32.
    unsafe { g_unichar_toupper(c) }
}

pub(crate) fn unichar_totitle(c: u32) -> u32 {
    // SAFETY: g_unichar_totitle is a pure table lookup defined for every u32.
    unsafe { g_unichar_totitle(c) }
}
