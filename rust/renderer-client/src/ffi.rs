//! Southstar — printing through the renderer: the in-process print hook and the GLib and Cairo calls that load the printed sheets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr::NonNull;
use std::ffi::CString;
use std::sync::Mutex;

use southstar_glib::{self as glib, GPtrArray};

use crate::client::Renderer;
use crate::os;

pub use southstar_print::PrintSetup;

pub struct PrintedPages {
    pub sheets: NonNull<GPtrArray>,
    pub scale: f64,
}

#[repr(C)]
struct CairoSurface {
    _private: [u8; 0],
}

pub type PrintFn =
    unsafe extern "C" fn(conn: *mut c_void, out_setup: *mut c_void) -> *mut GPtrArray;

static PRINT: Mutex<Option<PrintFn>> = Mutex::new(None);

unsafe extern "C" {
    fn g_get_user_runtime_dir() -> *const c_char;
    fn g_get_monotonic_time() -> i64;
    fn g_ptr_array_new() -> *mut GPtrArray;
    fn g_ptr_array_add(array: *mut GPtrArray, data: *mut c_void);
    fn g_ptr_array_free(array: *mut GPtrArray, free_segment: c_int) -> *mut *mut c_void;
    fn g_remove(filename: *const c_char) -> c_int;
    fn cairo_image_surface_create_from_png(filename: *const c_char) -> *mut CairoSurface;
    fn cairo_surface_status(surface: *mut CairoSurface) -> c_int;
    fn cairo_surface_destroy(surface: *mut CairoSurface);
}

pub fn set_print(print: Option<PrintFn>) {
    *PRINT.lock().unwrap_or_else(|e| e.into_inner()) = print;
}

fn print_prefix() -> Vec<u8> {
    // SAFETY: g_get_user_runtime_dir takes no arguments and returns a string GLib owns.
    let dir = unsafe { g_get_user_runtime_dir() };
    // SAFETY: dir is a NUL-terminated string GLib keeps for the life of the process.
    let mut prefix = unsafe { glib::bytes(dir) }.unwrap_or_default().to_vec();
    // SAFETY: g_get_monotonic_time takes no arguments and only reads the clock.
    let now = unsafe { g_get_monotonic_time() };
    prefix.extend_from_slice(
        format!("/southstar-print-{now}-{}", os::next_print_counter()).as_bytes(),
    );
    prefix
}

fn load_sheet(sheets: *mut GPtrArray, path: &CString) {
    // SAFETY: path is a NUL-terminated file name.
    let img = unsafe { cairo_image_surface_create_from_png(path.as_ptr()) };
    // SAFETY: cairo returns a surface, possibly an error surface, that the caller owns.
    if unsafe { cairo_surface_status(img) } == 0 {
        // SAFETY: sheets is a live array from g_ptr_array_new; it takes over the surface.
        unsafe { g_ptr_array_add(sheets, img.cast()) };
    } else {
        // SAFETY: img is the caller's own reference, released exactly once here.
        unsafe { cairo_surface_destroy(img) };
    }
}

pub fn print(r: &mut Renderer, setup: &mut PrintSetup) -> Option<PrintedPages> {
    let hook = *PRINT.lock().unwrap_or_else(|e| e.into_inner());
    if let (false, Some(print)) = (r.inproc_conn.is_null(), hook) {
        // SAFETY: inproc_conn is the live connection the attach hook returned, and setup is a valid PrintSetup the hook fills in.
        let sheets = unsafe { print(r.inproc_conn, (setup as *mut PrintSetup).cast()) };
        return NonNull::new(sheets).map(|sheets| PrintedPages { sheets, scale: 1.0 });
    }
    let prefix = print_prefix();
    let reply = r.print_request(&prefix)?;
    *setup = PrintSetup::a4();
    let fields = [
        (&mut setup.width, reply.width),
        (&mut setup.height, reply.height),
        (&mut setup.margin_top, reply.margins[0]),
        (&mut setup.margin_right, reply.margins[1]),
        (&mut setup.margin_bottom, reply.margins[2]),
        (&mut setup.margin_left, reply.margins[3]),
    ];
    for (slot, value) in fields {
        if let Some(value) = value {
            *slot = value;
        }
    }
    // SAFETY: g_ptr_array_new takes no arguments and returns a new array.
    let sheets = unsafe { g_ptr_array_new() };
    for i in 0..reply.pages.max(0) {
        let mut path = prefix.clone();
        path.extend_from_slice(format!("-{i}.png").as_bytes());
        let path = CString::new(path).unwrap_or_default();
        // SAFETY: path is a NUL-terminated file name.
        if unsafe { glib::g_file_test(path.as_ptr(), glib::FILE_TEST_EXISTS) } == 0 {
            break;
        }
        load_sheet(sheets, &path);
        // SAFETY: path is a NUL-terminated file name.
        unsafe { g_remove(path.as_ptr()) };
    }
    // SAFETY: sheets is the live array g_ptr_array_new returned above.
    if unsafe { (*sheets).len } == 0 {
        // SAFETY: nothing else holds the empty array, so it is freed once here.
        unsafe { g_ptr_array_free(sheets, 1) };
        return None;
    }
    let scale = if reply.scale > 0.0 { reply.scale } else { 1.0 };
    NonNull::new(sheets).map(|sheets| PrintedPages { sheets, scale })
}
