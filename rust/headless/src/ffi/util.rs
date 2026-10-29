//! Southstar — the GLib main loop, clocks, string parsing, file reading and base64 the driver needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};

#[repr(C)]
struct GMainLoop {
    _private: [u8; 0],
}

type GSourceFunc = unsafe extern "C" fn(data: *mut c_void) -> GBoolean;

unsafe extern "C" {
    fn g_main_loop_new(context: *mut c_void, is_running: GBoolean) -> *mut GMainLoop;
    fn g_main_loop_run(main_loop: *mut GMainLoop);
    fn g_main_loop_quit(main_loop: *mut GMainLoop);
    fn g_main_loop_unref(main_loop: *mut GMainLoop);
    fn g_idle_add(function: GSourceFunc, data: *mut c_void) -> c_uint;
}

pub struct MainLoop(NonNull<GMainLoop>);

pub struct Quitter(*mut GMainLoop);

unsafe impl Send for Quitter {}

unsafe extern "C" fn quit_loop(data: *mut c_void) -> GBoolean {
    unsafe { g_main_loop_quit(data.cast()) };
    glib::FALSE
}

impl MainLoop {
    pub fn new() -> MainLoop {
        let main_loop = unsafe { g_main_loop_new(ptr::null_mut(), glib::FALSE) };
        MainLoop(NonNull::new(main_loop).expect("g_main_loop_new"))
    }

    pub fn run(&self) {
        unsafe { g_main_loop_run(self.0.as_ptr()) };
    }

    pub fn quitter(&self) -> Quitter {
        Quitter(self.0.as_ptr())
    }
}

impl Drop for MainLoop {
    fn drop(&mut self) {
        unsafe { g_main_loop_unref(self.0.as_ptr()) };
    }
}

impl Quitter {
    pub fn quit_when_idle(self) {
        unsafe { g_idle_add(quit_loop, self.0.cast()) };
    }
}

pub fn monotonic_us() -> i64 {
    southstar_glib::monotonic_us()
}

pub fn usleep(us: u64) {
    std::thread::sleep(std::time::Duration::from_micros(us));
}

fn c_input(s: &[u8]) -> CString {
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    CString::new(&s[..end]).unwrap_or_default()
}

pub fn ascii_strtoll(s: &[u8]) -> i64 {
    let s = until_nul(s);
    let start = s
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .unwrap_or(s.len());
    let mut rest = &s[start..];
    let negative = rest.first() == Some(&b'-');
    if matches!(rest.first(), Some(b'+' | b'-')) {
        rest = &rest[1..];
    }
    let magnitude = rest
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .fold(0i128, |acc, d| {
            (acc * 10 + i128::from(d - b'0')).min(i128::from(u64::MAX))
        });
    let value = if negative { -magnitude } else { magnitude };
    i64::try_from(value).unwrap_or(if negative { i64::MIN } else { i64::MAX })
}

fn until_nul(s: &[u8]) -> &[u8] {
    &s[..s.iter().position(|&b| b == 0).unwrap_or(s.len())]
}

pub fn strcompress(s: &[u8]) -> CString {
    let mut out = Vec::with_capacity(s.len());
    let mut rest = until_nul(s);
    while let Some((&b, tail)) = rest.split_first() {
        rest = tail;
        if b != b'\\' {
            out.push(b);
            continue;
        }
        let Some((&escaped, tail)) = rest.split_first() else {
            break;
        };
        rest = tail;
        let byte = match escaped {
            b'0'..=b'7' => {
                let mut value = u32::from(escaped - b'0');
                for _ in 0..2 {
                    match rest.split_first() {
                        Some((&d @ b'0'..=b'7', tail)) => {
                            value = value * 8 + u32::from(d - b'0');
                            rest = tail;
                        }
                        _ => break,
                    }
                }
                value.to_le_bytes()[0]
            }
            b'b' => 0x08,
            b'f' => 0x0c,
            b'n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'v' => 0x0b,
            other => other,
        };
        out.push(byte);
    }
    c_input(&out)
}

pub fn file_contents(path: &[u8]) -> Option<CString> {
    glib::file_get_contents(until_nul(path)).map(|contents| c_input(&contents))
}

pub fn dlog_level_name(level: u32) -> Option<&'static [u8]> {
    Some(southstar_debuglog::level_name(level as c_int).to_bytes())
}
