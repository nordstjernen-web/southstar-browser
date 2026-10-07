//! Southstar — the C ABI of the date helpers, as declared in src/datetime.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long};
use core::ptr;

#[unsafe(no_mangle)]
pub extern "C" fn ns_dt_floormod(a: c_long, b: c_long) -> c_long {
    crate::floormod(a, b)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_dt_days_from_civil(y: c_int, m: c_int, d: c_int) -> c_long {
    crate::days_from_civil(y, m, d)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dt_civil_from_days(
    z: c_long,
    y: *mut c_int,
    m: *mut c_int,
    d: *mut c_int,
) {
    let (year, month, day) = crate::civil_from_days(z);
    unsafe {
        *y = year;
        *m = month;
        *d = day;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_dt_days_in_month(y: c_int, m: c_int) -> c_int {
    crate::days_in_month(y, m)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_dt_iso_weeks_in_year(y: c_int) -> c_int {
    crate::iso_weeks_in_year(y)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_dt_iso_week1_monday(y: c_int) -> c_long {
    crate::iso_week1_monday(y)
}

unsafe fn bytes<'a>(p: *const c_char) -> &'a [u8] {
    unsafe { CStr::from_ptr(p) }.to_bytes()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dt_rd_digits(
    p: *const c_char,
    min: c_int,
    max: c_int,
    out: *mut c_int,
) -> *const c_char {
    match crate::read_digits(unsafe { bytes(p) }, min, max) {
        Some((n, value)) => unsafe {
            *out = value;
            p.add(n)
        },
        None => ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dt_rd_date(
    p: *const c_char,
    y: *mut c_int,
    m: *mut c_int,
    d: *mut c_int,
) -> *const c_char {
    match crate::read_date(unsafe { bytes(p) }) {
        Some((n, year, month, day)) => unsafe {
            *y = year;
            *m = month;
            *d = day;
            p.add(n)
        },
        None => ptr::null(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dt_rd_time(p: *const c_char, ms: *mut c_int) -> *const c_char {
    match crate::read_time(unsafe { bytes(p) }) {
        Some((n, millis)) => unsafe {
            *ms = millis;
            p.add(n)
        },
        None => ptr::null(),
    }
}
