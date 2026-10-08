//! Southstar — the C ABI of Intl for the QuickJS engine, as declared in src/js_intl.h, and the GLib, C library and Pango calls behind it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::ffi::CString;

use southstar_glib as glib;
#[cfg(feature = "quickjs")]
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

const G_NORMALIZE_ALL: c_int = 2;
const G_UNICODE_SPACING_MARK: c_int = 10;
const G_UNICODE_ENCLOSING_MARK: c_int = 11;
const G_UNICODE_NON_SPACING_MARK: c_int = 12;

#[repr(C)]
struct GTimeZone {
    _private: [u8; 0],
}

#[repr(C)]
struct PangoLanguage {
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
    pub(crate) weekday: i32,
    pub(crate) hour: i32,
    pub(crate) minute: i32,
    pub(crate) second: i32,
}

unsafe extern "C" {
    fn g_utf8_normalize(text: *const c_char, len: isize, mode: c_int) -> *mut c_char;
    fn g_utf8_get_char(p: *const c_char) -> u32;
    fn g_unichar_type(c: u32) -> c_int;
    fn g_unichar_to_utf8(c: u32, outbuf: *mut c_char) -> c_int;
    fn g_unichar_isalnum(c: u32) -> c_int;
    fn g_utf8_casefold(text: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_collate_key(text: *const c_char, len: isize) -> *mut c_char;
    fn g_utf8_collate(a: *const c_char, b: *const c_char) -> c_int;
    fn g_get_real_time() -> i64;
    fn g_time_zone_new_local() -> *mut GTimeZone;
    fn g_time_zone_get_identifier(tz: *mut GTimeZone) -> *const c_char;
    fn g_time_zone_unref(tz: *mut GTimeZone);
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_language_from_string")]
    fn pango_language_from_string(language: *const c_char) -> *mut PangoLanguage;
    #[cfg_attr(feature = "ns-pango", link_name = "ns_pango_get_log_attrs")]
    fn pango_get_log_attrs(
        text: *const c_char,
        length: c_int,
        level: c_int,
        language: *mut PangoLanguage,
        attrs: *mut u32,
        attrs_len: c_int,
    );
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

unsafe fn take(text: *mut c_char) -> Vec<u8> {
    if text.is_null() {
        return Vec::new();
    }
    let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
    unsafe { glib::g_free(text.cast()) };
    bytes
}

pub(crate) fn utf8_skip(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        0xf8..=0xfb => 5,
        0xfc..=0xfd => 6,
        _ => 1,
    }
}

pub(crate) fn utf8_strlen(text: &[u8]) -> usize {
    let mut count = 0;
    let mut at = 0;
    while at < text.len() {
        at += utf8_skip(text[at]);
        count += 1;
    }
    count
}

fn strip_marks(text: &CStr) -> CString {
    let bytes = text.to_bytes();
    let mut kept = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let c = unsafe { g_utf8_get_char(text.as_ptr().add(at)) };
        let kind = unsafe { g_unichar_type(c) };
        if !matches!(
            kind,
            G_UNICODE_NON_SPACING_MARK | G_UNICODE_SPACING_MARK | G_UNICODE_ENCLOSING_MARK
        ) {
            let mut encoded = [0 as c_char; 8];
            let len = unsafe { g_unichar_to_utf8(c, encoded.as_mut_ptr()) };
            kept.extend(encoded[..len as usize].iter().map(|&b| b as u8));
        }
        at += utf8_skip(bytes[at]);
    }
    cstring(&kept)
}

pub(crate) fn collation_key(text: &[u8], sensitivity: &[u8]) -> Vec<u8> {
    let input = cstring(text);
    let normalized = unsafe { g_utf8_normalize(input.as_ptr(), -1, G_NORMALIZE_ALL) };
    let mut current = if normalized.is_null() {
        input
    } else {
        cstring(&unsafe { take(normalized) })
    };
    let fold = sensitivity == b"base" || sensitivity == b"accent";
    let strip = sensitivity == b"base" || sensitivity == b"case";
    if strip {
        current = strip_marks(&current);
    }
    if fold {
        current = cstring(&unsafe { take(g_utf8_casefold(current.as_ptr(), -1)) });
    }
    unsafe { take(g_utf8_collate_key(current.as_ptr(), -1)) }
}

pub(crate) fn collate(a: &[u8], b: &[u8]) -> i32 {
    let a = cstring(a);
    let b = cstring(b);
    unsafe { g_utf8_collate(a.as_ptr(), b.as_ptr()) }
}

pub(crate) fn is_alnum_at(text: &CStr, offset: usize) -> bool {
    unsafe { g_unichar_isalnum(g_utf8_get_char(text.as_ptr().add(offset))) != 0 }
}

pub(crate) fn real_time_ms() -> f64 {
    (unsafe { g_get_real_time() } / 1000) as f64
}

pub(crate) fn break_down(seconds: i64, utc: bool) -> Option<BrokenDown> {
    let mut tm = Tm::default();
    #[cfg(not(windows))]
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
        weekday: tm.wday,
        hour: tm.hour,
        minute: tm.min,
        second: tm.sec,
    })
}

pub(crate) fn local_zone_identifier() -> Option<Vec<u8>> {
    let zone = unsafe { g_time_zone_new_local() };
    if zone.is_null() {
        return None;
    }
    let id = unsafe { glib::bytes(g_time_zone_get_identifier(zone)) }.map(<[u8]>::to_vec);
    unsafe { g_time_zone_unref(zone) };
    id
}

pub(crate) fn getenv(name: &CStr) -> Option<Vec<u8>> {
    unsafe { glib::bytes(glib::g_getenv(name.as_ptr())) }.map(<[u8]>::to_vec)
}

pub(crate) fn language_names() -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    let mut entry = unsafe { glib::g_get_language_names() };
    if entry.is_null() {
        return names;
    }
    while let Some(name) = unsafe { glib::bytes(*entry) } {
        names.push(name.to_vec());
        entry = unsafe { entry.add(1) };
    }
    names
}

const CURSOR_POSITION: u32 = 4;
const WORD_START: u32 = 5;
const WORD_END: u32 = 6;
const SENTENCE_BOUNDARY: u32 = 7;

fn flag(attr: u32, bit: u32) -> bool {
    if cfg!(target_endian = "big") {
        attr & (1 << (31 - bit)) != 0
    } else {
        attr & (1 << bit) != 0
    }
}

pub(crate) struct LogAttrs(Vec<u32>);

impl LogAttrs {
    pub(crate) fn cursor_position(&self, at: usize) -> bool {
        flag(self.0[at], CURSOR_POSITION)
    }

    pub(crate) fn word_boundary(&self, at: usize) -> bool {
        flag(self.0[at], WORD_START) || flag(self.0[at], WORD_END)
    }

    pub(crate) fn sentence_boundary(&self, at: usize) -> bool {
        flag(self.0[at], SENTENCE_BOUNDARY)
    }
}

pub(crate) fn pango_language(locale: &[u8]) -> Option<CString> {
    (!locale.is_empty()).then(|| cstring(locale))
}

pub(crate) fn log_attrs(text: &CStr, chars: usize, language: Option<&CString>) -> LogAttrs {
    let language = language.map_or(ptr::null_mut(), |tag| unsafe {
        pango_language_from_string(tag.as_ptr())
    });
    let mut attrs = vec![0u32; chars + 1];
    unsafe {
        pango_get_log_attrs(
            text.as_ptr(),
            text.to_bytes().len() as c_int,
            -1,
            language,
            attrs.as_mut_ptr(),
            (chars + 1) as c_int,
        )
    };
    LogAttrs(attrs)
}

pub(crate) fn c_text(bytes: &[u8]) -> CString {
    cstring(bytes)
}

#[cfg(windows)]
pub(crate) mod windows {
    use core::ffi::c_long;

    use southstar_glib as glib;

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
        let text = unsafe {
            glib::g_utf16_to_utf8(
                wide.as_ptr(),
                -1 as c_long,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        };
        if text.is_null() {
            return None;
        }
        let bytes = unsafe { glib::bytes(text) }.unwrap_or_default().to_vec();
        unsafe { glib::g_free(text.cast()) };
        Some(bytes)
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

#[cfg(feature = "quickjs")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_intl_install(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            crate::install(scope, &global);
        });
    }
}
