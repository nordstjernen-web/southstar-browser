//! Southstar — output through the C runtime's stdout buffer and stderr, %g formatting and sscanf parsing as C does them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_long, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib::GStr;

unsafe extern "C" {
    fn printf(format: *const c_char, ...) -> c_int;
    fn putchar(c: c_int) -> c_int;
    fn fflush(stream: *mut c_void) -> c_int;
    fn sscanf(s: *const c_char, format: *const c_char, ...) -> c_int;
    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
}

#[cfg(windows)]
unsafe extern "C" {
    fn __acrt_iob_func(index: core::ffi::c_uint) -> *mut c_void;
    fn fwrite(data: *const c_void, size: usize, count: usize, stream: *mut c_void) -> usize;
}

pub fn out(bytes: &[u8]) {
    for (i, run) in bytes.split(|&b| b == 0).enumerate() {
        if i > 0 {
            unsafe { putchar(0) };
        }
        if let Ok(run) = CString::new(run) {
            if !run.is_empty() {
                unsafe { printf(c"%s".as_ptr(), run.as_ptr()) };
            }
        }
    }
}

pub fn flush() {
    unsafe { fflush(ptr::null_mut()) };
}

#[cfg(windows)]
pub fn err(bytes: &[u8]) {
    unsafe { fwrite(bytes.as_ptr().cast(), 1, bytes.len(), __acrt_iob_func(2)) };
}

#[cfg(not(windows))]
pub fn err(bytes: &[u8]) {
    use std::io::Write;
    let _ = std::io::stderr().write_all(bytes);
}

pub fn fmt_g(value: f64) -> String {
    let s = unsafe { GStr::take(g_strdup_printf(c"%g".as_ptr(), value)) };
    s.map_or_else(String::new, |s| {
        String::from_utf8_lossy(s.to_bytes()).into_owned()
    })
}

fn c_input(s: &[u8]) -> CString {
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    CString::new(&s[..end]).unwrap_or_default()
}

pub fn scan_point(s: &[u8]) -> Option<(f64, f64)> {
    let (mut x, mut y) = (0.0, 0.0);
    let n = unsafe {
        sscanf(
            c_input(s).as_ptr(),
            c"%lf , %lf".as_ptr(),
            &mut x as *mut f64,
            &mut y as *mut f64,
        )
    };
    (n == 2).then_some((x, y))
}

pub fn scan_select(s: &[u8]) -> Option<(c_int, f64, f64)> {
    let (mut kind, mut x, mut y): (c_int, f64, f64) = (0, 0.0, 0.0);
    let n = unsafe {
        sscanf(
            c_input(s).as_ptr(),
            c"%d %lf , %lf".as_ptr(),
            &mut kind as *mut c_int,
            &mut x as *mut f64,
            &mut y as *mut f64,
        )
    };
    (n == 3).then_some((kind, x, y))
}

pub fn scan_size(s: &[u8]) -> Option<(c_int, c_int)> {
    let (mut w, mut h): (c_int, c_int) = (0, 0);
    let n = unsafe {
        sscanf(
            c_input(s).as_ptr(),
            c"%d %d".as_ptr(),
            &mut w as *mut c_int,
            &mut h as *mut c_int,
        )
    };
    (n == 2).then_some((w, h))
}

pub fn scan_pair(s: &[u8]) -> Option<(f64, f64)> {
    let (mut x, mut y) = (0.0, 0.0);
    let n = unsafe {
        sscanf(
            c_input(s).as_ptr(),
            c"%lf %lf".as_ptr(),
            &mut x as *mut f64,
            &mut y as *mut f64,
        )
    };
    (n == 2).then_some((x, y))
}

pub fn scan_hold(s: &[u8]) -> Option<(f64, f64, c_long)> {
    let (mut x, mut y, mut ms): (f64, f64, c_long) = (0.0, 0.0, 0);
    let n = unsafe {
        sscanf(
            c_input(s).as_ptr(),
            c"%lf , %lf %ld".as_ptr(),
            &mut x as *mut f64,
            &mut y as *mut f64,
            &mut ms as *mut c_long,
        )
    };
    (n == 3).then_some((x, y, ms))
}

pub fn scan_drag(s: &[u8]) -> Option<[f64; 4]> {
    let mut v = [0.0f64; 4];
    let [a, b, c, d] = &mut v;
    let n = unsafe {
        sscanf(
            c_input(s).as_ptr(),
            c"%lf , %lf %lf , %lf".as_ptr(),
            a as *mut f64,
            b as *mut f64,
            c as *mut f64,
            d as *mut f64,
        )
    };
    (n == 4).then_some(v)
}
