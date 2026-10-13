//! Southstar — set the macOS Dock icon when running unbundled, through the Objective-C runtime.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::transmute;
use core::ptr;

use southstar_gtk_ffi::{self as gtk, CAIRO_FORMAT_ARGB32, CairoSurface, GBoolean};

use super::gtk::{borrowed, build_filename};

const DOCK_ICON_DIM: c_int = 512;
const CAIRO_STATUS_SUCCESS: c_int = 0;
const ICON_RESOURCE: &CStr = c"/org/southstar/WebBrowser/icons/scalable/apps/southstar.svg";

type Id = *mut c_void;
type Sel = *const c_void;

unsafe extern "C" {
    fn ns_svg_render_bytes(
        cr: *mut gtk::Cairo,
        data: *const u8,
        len: usize,
        width: f64,
        height: f64,
    ) -> GBoolean;
    fn g_get_tmp_dir() -> *const c_char;
    fn cairo_surface_write_to_png(surface: *mut CairoSurface, filename: *const c_char) -> c_int;
}

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

unsafe fn send(receiver: Id, selector: &CStr) -> Id {
    unsafe {
        let imp: unsafe extern "C" fn(Id, Sel) -> Id =
            transmute(objc_msgSend as unsafe extern "C" fn());
        imp(receiver, sel_registerName(selector.as_ptr()))
    }
}

unsafe fn send_unit(receiver: Id, selector: &CStr) {
    unsafe {
        let imp: unsafe extern "C" fn(Id, Sel) = transmute(objc_msgSend as unsafe extern "C" fn());
        imp(receiver, sel_registerName(selector.as_ptr()))
    }
}

unsafe fn send_ptr(receiver: Id, selector: &CStr, arg: *const c_void) -> Id {
    unsafe {
        let imp: unsafe extern "C" fn(Id, Sel, *const c_void) -> Id =
            transmute(objc_msgSend as unsafe extern "C" fn());
        imp(receiver, sel_registerName(selector.as_ptr()), arg)
    }
}

unsafe fn send_void(receiver: Id, selector: &CStr, arg: Id) {
    unsafe {
        let imp: unsafe extern "C" fn(Id, Sel, Id) =
            transmute(objc_msgSend as unsafe extern "C" fn());
        imp(receiver, sel_registerName(selector.as_ptr()), arg)
    }
}

unsafe fn class(name: &CStr) -> Id {
    unsafe { objc_getClass(name.as_ptr()) }
}

fn render_icon_png() -> Option<std::ffi::CString> {
    unsafe {
        let bytes = gtk::g_resources_lookup_data(ICON_RESOURCE.as_ptr(), 0, ptr::null_mut());
        if bytes.is_null() {
            return None;
        }
        let surf =
            gtk::cairo_image_surface_create(CAIRO_FORMAT_ARGB32, DOCK_ICON_DIM, DOCK_ICON_DIM);
        if gtk::cairo_surface_status(surf) != CAIRO_STATUS_SUCCESS {
            gtk::cairo_surface_destroy(surf);
            gtk::g_bytes_unref(bytes);
            return None;
        }
        let mut len = 0usize;
        let data = gtk::g_bytes_get_data(bytes, &mut len);
        let cr = gtk::cairo_create(surf);
        let rendered = ns_svg_render_bytes(
            cr,
            data.cast(),
            len,
            f64::from(DOCK_ICON_DIM),
            f64::from(DOCK_ICON_DIM),
        ) != 0;
        gtk::cairo_destroy(cr);
        gtk::g_bytes_unref(bytes);
        if !rendered {
            gtk::cairo_surface_destroy(surf);
            return None;
        }
        gtk::cairo_surface_flush(surf);

        let tmp = borrowed(g_get_tmp_dir())?.to_owned();
        let png = build_filename(&[&tmp, c"southstar-dock.png"]);
        let status = cairo_surface_write_to_png(surf, png.as_ptr());
        gtk::cairo_surface_destroy(surf);
        (status == CAIRO_STATUS_SUCCESS).then_some(png)
    }
}

pub fn set_dock_icon() {
    let Some(png) = render_icon_png() else {
        return;
    };
    unsafe {
        let pool = objc_autoreleasePoolPush();
        let path = send_ptr(
            class(c"NSString"),
            c"stringWithUTF8String:",
            png.as_ptr().cast(),
        );
        let image = send(class(c"NSImage"), c"alloc");
        let image = send_ptr(image, c"initWithContentsOfFile:", path.cast_const());
        if !image.is_null() {
            let app = send(class(c"NSApplication"), c"sharedApplication");
            send_void(app, c"setApplicationIconImage:", image);
            send_unit(image, c"release");
        }
        objc_autoreleasePoolPop(pool);
    }
}
