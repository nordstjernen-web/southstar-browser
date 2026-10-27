//! Southstar — the GLib, C library and southstar-text calls behind Intl.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use std::ffi::CString;

use southstar_glib as glib;
use southstar_text::LogAttr;
use southstar_text::breaks::get_log_attrs;
use southstar_text::language::Language;

#[repr(C)]
struct GTimeZone {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Default)]
struct Tm {
    sec: c_int,
    min: c_int,
    hour: c_int,
    mday: c_int,
    mon: c_int,
    year: c_int,
    wday: c_int,
    yday: c_int,
    isdst: c_int,
    tail: [i64; 4],
}

pub(crate) struct BrokenDown {
    pub(crate) year: i32,
    pub(crate) month: i32,
    pub(crate) day: i32,
    pub(crate) hour: i32,
    pub(crate) minute: i32,
    pub(crate) second: i32,
}

unsafe extern "C" {
    fn g_time_zone_new_local() -> *mut GTimeZone;
    fn g_time_zone_get_identifier(tz: *mut GTimeZone) -> *const c_char;
    fn g_time_zone_unref(tz: *mut GTimeZone);
}

#[cfg(not(windows))]
unsafe extern "C" {
    fn gmtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
}

#[cfg(windows)]
unsafe extern "C" {
    fn _gmtime64_s(out: *mut Tm, time: *const i64) -> c_int;
    fn _localtime64_s(out: *mut Tm, time: *const i64) -> c_int;
}

fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&c| c == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn real_time_ms() -> f64 {
    let real_us: i64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or_else(
            |before| -i64::try_from(before.duration().as_micros()).unwrap_or(i64::MAX),
            |after| i64::try_from(after.as_micros()).unwrap_or(i64::MAX),
        );
    (real_us / 1000) as f64
}

pub(crate) fn break_down(seconds: i64, utc: bool) -> Option<BrokenDown> {
    let mut tm = Tm::default();
    #[cfg(not(windows))]
    // SAFETY: both pointers come from live locals for the duration of the call.
    let ok = unsafe {
        if utc {
            !gmtime_r(&seconds, &mut tm).is_null()
        } else {
            !localtime_r(&seconds, &mut tm).is_null()
        }
    };
    #[cfg(windows)]
    let ok = unsafe {
        if utc {
            _gmtime64_s(&mut tm, &seconds) == 0
        } else {
            _localtime64_s(&mut tm, &seconds) == 0
        }
    };
    ok.then_some(BrokenDown {
        year: tm.year.wrapping_add(1900),
        month: tm.mon,
        day: tm.mday,
        hour: tm.hour,
        minute: tm.min,
        second: tm.sec,
    })
}

pub(crate) fn local_zone_identifier() -> Option<Vec<u8>> {
    // SAFETY: g_time_zone_new_local takes no arguments and returns a new reference or null.
    let zone = unsafe { g_time_zone_new_local() };
    if zone.is_null() {
        return None;
    }
    // SAFETY: `zone` is the live reference created above, and the identifier it returns is a NUL-terminated string it owns.
    let id = unsafe { glib::bytes(g_time_zone_get_identifier(zone)) }.map(<[u8]>::to_vec);
    // SAFETY: releases the reference created above; `id` was copied out first.
    unsafe { g_time_zone_unref(zone) };
    id
}

pub(crate) fn getenv(name: &CStr) -> Option<Vec<u8>> {
    glib::env_bytes(name.to_str().ok()?)
}

pub(crate) fn language_names() -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    // SAFETY: GLib returns a NULL-terminated array it owns for the life of the process.
    let mut entry = unsafe { glib::g_get_language_names() };
    if entry.is_null() {
        return names;
    }
    // SAFETY: `entry` points into the NULL-terminated array and has not passed its terminator; each entry is null or a NUL-terminated string owned by GLib.
    while let Some(name) = unsafe { glib::bytes(*entry) } {
        names.push(name.to_vec());
        // SAFETY: the current entry was not the terminator, so the next one is in bounds.
        entry = unsafe { entry.add(1) };
    }
    names
}

pub(crate) struct LogAttrs(Vec<LogAttr>);

impl LogAttrs {
    pub(crate) fn cursor_position(&self, at: usize) -> bool {
        self.0[at].is_cursor_position()
    }

    pub(crate) fn word_boundary(&self, at: usize) -> bool {
        self.0[at].is_word_boundary()
    }

    pub(crate) fn sentence_boundary(&self, at: usize) -> bool {
        self.0[at].is_sentence_boundary()
    }
}

pub(crate) fn text_language(locale: &[u8]) -> Option<CString> {
    (!locale.is_empty()).then(|| cstring(locale))
}

pub(crate) fn log_attrs(text: &CStr, chars: usize, language: Option<&CString>) -> LogAttrs {
    let language = language.map(|tag| Language::from_string(&tag.to_string_lossy()));
    let text = text.to_string_lossy();
    let mut attrs = vec![LogAttr::default(); text.chars().count() + 1];
    get_log_attrs(&text, language.map(Language::as_str), &mut attrs);
    attrs.resize(chars + 1, LogAttr::default());
    LogAttrs(attrs)
}

pub(crate) fn c_text(bytes: &[u8]) -> CString {
    cstring(bytes)
}

#[cfg(windows)]
pub(crate) mod windows {

    #[repr(C)]
    struct SystemTime {
        fields: [u16; 8],
    }

    #[repr(C)]
    struct DynamicTimeZoneInformation {
        bias: i32,
        standard_name: [u16; 32],
        standard_date: SystemTime,
        standard_bias: i32,
        daylight_name: [u16; 32],
        daylight_date: SystemTime,
        daylight_bias: i32,
        time_zone_key_name: [u16; 128],
        dynamic_daylight_time_disabled: u8,
    }

    const TIME_ZONE_ID_INVALID: u32 = 0xffff_ffff;
    const LOCALE_NAME_MAX_LENGTH: usize = 85;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDynamicTimeZoneInformation(info: *mut DynamicTimeZoneInformation) -> u32;
        fn GetUserDefaultLocaleName(name: *mut u16, len: i32) -> i32;
    }

    fn utf8(wide: &[u16]) -> Option<Vec<u8>> {
        let end = wide
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(wide.len());
        String::from_utf16(&wide[..end])
            .ok()
            .map(String::into_bytes)
    }

    pub(crate) fn zone() -> Option<(Vec<u8>, Option<Vec<u8>>)> {
        let mut info: DynamicTimeZoneInformation = unsafe { core::mem::zeroed() };
        if unsafe { GetDynamicTimeZoneInformation(&mut info) } == TIME_ZONE_ID_INVALID {
            return None;
        }
        let key = utf8(&info.time_zone_key_name)?;
        let mut locale = [0u16; LOCALE_NAME_MAX_LENGTH];
        unsafe { GetUserDefaultLocaleName(locale.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH as i32) };
        Some((key, utf8(&locale)))
    }
}
