//! Southstar — the GLib main loop, clocks, string parsing, file reading and base64 the driver needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint, c_ulong, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean, GStr};

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
    fn g_get_monotonic_time() -> i64;
    fn g_usleep(microseconds: c_ulong);
    fn g_ascii_strtoll(nptr: *const c_char, endptr: *mut *mut c_char, base: c_uint) -> i64;
    fn g_strcompress(source: *const c_char) -> *mut c_char;
    fn ns_dlog_level_name(level: c_uint) -> *const c_char;
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
    unsafe { g_get_monotonic_time() }
}

pub fn usleep(us: c_ulong) {
    unsafe { g_usleep(us) };
}

fn c_input(s: &[u8]) -> CString {
    let end = s.iter().position(|&b| b == 0).unwrap_or(s.len());
    CString::new(&s[..end]).unwrap_or_default()
}

pub fn ascii_strtoll(s: &[u8]) -> i64 {
    unsafe { g_ascii_strtoll(c_input(s).as_ptr(), ptr::null_mut(), 10) }
}

pub fn strcompress(s: &[u8]) -> GStr {
    let compressed = unsafe { GStr::take(g_strcompress(c_input(s).as_ptr())) };
    compressed.expect("g_strcompress")
}

pub fn file_contents(path: &[u8]) -> Option<GStr> {
    let mut contents: *mut c_char = ptr::null_mut();
    let ok = unsafe {
        glib::g_file_get_contents(
            c_input(path).as_ptr(),
            &mut contents,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    unsafe { GStr::take(contents) }
}

pub fn base64(data: &[u8]) -> GStr {
    let encoded = unsafe { GStr::take(glib::g_base64_encode(data.as_ptr(), data.len())) };
    encoded.expect("g_base64_encode")
}

pub fn dlog_level_name(level: u32) -> Option<&'static [u8]> {
    let name = unsafe { ns_dlog_level_name(level) };
    (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) }.to_bytes())
}
