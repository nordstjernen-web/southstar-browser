//! Southstar — GLib's local clock, time zone, charset conversion and collation behind safe owned-byte calls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;
use core::ffi::{CStr, c_char, c_int};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use crate::GError;

#[repr(C)]
struct GDateTime {
    _private: [u8; 0],
}

#[repr(C)]
struct GTimeZone {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn g_date_time_new_now_local() -> *mut GDateTime;
    fn g_date_time_new_from_unix_local(t: i64) -> *mut GDateTime;
    fn g_date_time_get_day_of_year(datetime: *mut GDateTime) -> c_int;
    fn g_date_time_get_year(datetime: *mut GDateTime) -> c_int;
    fn g_date_time_format(datetime: *mut GDateTime, format: *const c_char) -> *mut c_char;
    fn g_date_time_unref(datetime: *mut GDateTime);
    fn g_time_zone_new_local() -> *mut GTimeZone;
    fn g_time_zone_get_identifier(tz: *mut GTimeZone) -> *const c_char;
    fn g_time_zone_unref(tz: *mut GTimeZone);
    fn g_utf8_collate(str1: *const c_char, str2: *const c_char) -> c_int;
    fn g_convert(
        text: *const c_char,
        len: isize,
        to_codeset: *const c_char,
        from_codeset: *const c_char,
        bytes_read: *mut usize,
        bytes_written: *mut usize,
        error: *mut *mut GError,
    ) -> *mut c_char;
}

pub struct LocalDateTime(NonNull<GDateTime>);

impl LocalDateTime {
    pub fn now() -> Option<LocalDateTime> {
        // SAFETY: g_date_time_new_now_local has no preconditions and returns a new reference or null.
        NonNull::new(unsafe { g_date_time_new_now_local() }).map(LocalDateTime)
    }

    pub fn from_unix(secs: i64) -> Option<LocalDateTime> {
        // SAFETY: g_date_time_new_from_unix_local takes any value and returns a new reference or null when out of range.
        NonNull::new(unsafe { g_date_time_new_from_unix_local(secs) }).map(LocalDateTime)
    }

    pub fn year(&self) -> i32 {
        // SAFETY: self holds a live GDateTime reference.
        unsafe { g_date_time_get_year(self.0.as_ptr()) }
    }

    pub fn day_of_year(&self) -> i32 {
        // SAFETY: self holds a live GDateTime reference.
        unsafe { g_date_time_get_day_of_year(self.0.as_ptr()) }
    }

    pub fn format(&self, format: &CStr) -> Option<Vec<u8>> {
        // SAFETY: self holds a live GDateTime reference and format is NUL-terminated.
        let text = unsafe { g_date_time_format(self.0.as_ptr(), format.as_ptr()) };
        // SAFETY: g_date_time_format returned null or a new string the caller owns.
        unsafe { crate::GStr::take(text) }.map(|s| s.to_bytes().to_vec())
    }
}

impl Drop for LocalDateTime {
    fn drop(&mut self) {
        // SAFETY: releases the reference this value owns.
        unsafe { g_date_time_unref(self.0.as_ptr()) };
    }
}

pub fn local_time_zone_identifier() -> Option<Vec<u8>> {
    // SAFETY: g_time_zone_new_local has no preconditions and returns a new reference or null.
    let zone = NonNull::new(unsafe { g_time_zone_new_local() })?;
    // SAFETY: zone is the live reference created above.
    let id = unsafe { g_time_zone_get_identifier(zone.as_ptr()) };
    // SAFETY: the identifier is null or a NUL-terminated string the zone owns, copied out before the zone is released.
    let id = unsafe { crate::bytes(id) }.map(<[u8]>::to_vec);
    // SAFETY: releases the reference created above after the identifier was copied out.
    unsafe { g_time_zone_unref(zone.as_ptr()) };
    id
}

fn until_nul(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub fn utf8_collate(a: &[u8], b: &[u8]) -> Ordering {
    let (a, b) = (until_nul(a), until_nul(b));
    // SAFETY: both arguments are NUL-terminated strings that outlive the call.
    unsafe { g_utf8_collate(a.as_ptr(), b.as_ptr()) }.cmp(&0)
}

pub fn convert(text: &[u8], to: &[u8], from: &[u8]) -> Option<Vec<u8>> {
    let (to, from) = (until_nul(to), until_nul(from));
    let len = isize::try_from(text.len()).ok()?;
    let mut written = 0usize;
    // SAFETY: text is len readable bytes, both codeset names are NUL-terminated and the null out-parameters are optional.
    let out = unsafe {
        g_convert(
            text.as_ptr().cast(),
            len,
            to.as_ptr(),
            from.as_ptr(),
            ptr::null_mut(),
            &mut written,
            ptr::null_mut(),
        )
    };
    let out = NonNull::new(out)?;
    // SAFETY: g_convert wrote `written` bytes to the buffer it returned.
    let bytes = unsafe { crate::slice(out.as_ptr().cast::<u8>(), written) }.to_vec();
    // SAFETY: the buffer is a new GLib allocation the caller owns and is not used again.
    unsafe { crate::g_free(out.as_ptr().cast()) };
    Some(bytes)
}
