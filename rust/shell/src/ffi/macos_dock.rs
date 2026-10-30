//! Southstar — set the macOS Dock icon when running unbundled, through the Objective-C runtime.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::transmute;
use std::ffi::CString;

use southstar_cairo::{Context, Format, Surface};

use super::gtk::{build_filename, cstring};

const DOCK_ICON_DIM: c_int = 512;
const ICON_RESOURCE: &CStr = c"/org/southstar/WebBrowser/icons/scalable/apps/southstar.svg";

type Id = *mut c_void;
type Sel = *const c_void;

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    safe fn objc_autoreleasePoolPush() -> *mut c_void;
    fn objc_autoreleasePoolPop(pool: *mut c_void);
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

fn selector(name: &CStr) -> Sel {
    // SAFETY: `name` is a NUL-terminated string that outlives the call; the runtime copies it.
    unsafe { sel_registerName(name.as_ptr()) }
}

fn class(name: &CStr) -> Id {
    // SAFETY: `name` is a NUL-terminated string that outlives the call; an unknown class is nil.
    unsafe { objc_getClass(name.as_ptr()) }
}

/// # Safety
///
/// `receiver` must be nil or a live object whose `selector` method takes no arguments and
/// returns an object.
unsafe fn send(receiver: Id, selector_name: &CStr) -> Id {
    // SAFETY: objc_msgSend is called through a pointer cast to the method's exact signature,
    // which the caller guarantees.
    let imp: unsafe extern "C" fn(Id, Sel) -> Id =
        unsafe { transmute(objc_msgSend as unsafe extern "C" fn()) };
    // SAFETY: the caller guarantees `receiver` answers `selector_name` with this signature.
    unsafe { imp(receiver, selector(selector_name)) }
}

/// # Safety
///
/// `receiver` must be nil or a live object whose `selector` method takes no arguments and
/// returns nothing.
unsafe fn send_unit(receiver: Id, selector_name: &CStr) {
    // SAFETY: objc_msgSend is called through a pointer cast to the method's exact signature,
    // which the caller guarantees.
    let imp: unsafe extern "C" fn(Id, Sel) =
        unsafe { transmute(objc_msgSend as unsafe extern "C" fn()) };
    // SAFETY: the caller guarantees `receiver` answers `selector_name` with this signature.
    unsafe { imp(receiver, selector(selector_name)) }
}

/// # Safety
///
/// `receiver` must be nil or a live object whose `selector` method takes one pointer argument
/// and returns an object, and `arg` must be what that method expects.
unsafe fn send_ptr(receiver: Id, selector_name: &CStr, arg: *const c_void) -> Id {
    // SAFETY: objc_msgSend is called through a pointer cast to the method's exact signature,
    // which the caller guarantees.
    let imp: unsafe extern "C" fn(Id, Sel, *const c_void) -> Id =
        unsafe { transmute(objc_msgSend as unsafe extern "C" fn()) };
    // SAFETY: the caller guarantees `receiver` answers `selector_name` with this signature and
    // that `arg` is valid for it.
    unsafe { imp(receiver, selector(selector_name), arg) }
}

/// # Safety
///
/// `receiver` must be nil or a live object whose `selector` method takes one object argument
/// and returns nothing, and `arg` must be nil or a live object of the expected class.
unsafe fn send_void(receiver: Id, selector_name: &CStr, arg: Id) {
    // SAFETY: objc_msgSend is called through a pointer cast to the method's exact signature,
    // which the caller guarantees.
    let imp: unsafe extern "C" fn(Id, Sel, Id) =
        unsafe { transmute(objc_msgSend as unsafe extern "C" fn()) };
    // SAFETY: the caller guarantees `receiver` answers `selector_name` with this signature and
    // that `arg` is valid for it.
    unsafe { imp(receiver, selector(selector_name), arg) }
}

fn render_icon_png() -> Option<CString> {
    let svg = southstar_gtk::resource_bytes(ICON_RESOURCE)?;
    let surface = Surface::image(Format::Argb32, DOCK_ICON_DIM, DOCK_ICON_DIM)?;
    let rendered = {
        let cr = Context::new(surface.as_ref());
        southstar_svg::render_bytes(
            southstar_svg::Canvas::from(cr.as_ref()),
            &svg,
            f64::from(DOCK_ICON_DIM),
            f64::from(DOCK_ICON_DIM),
        )
    };
    if !rendered {
        return None;
    }
    surface.as_ref().flush();
    let tmp = Some(southstar_glib::tmp_dir()).filter(|dir| !dir.is_empty())?;
    let png = build_filename(&[&cstring(tmp), c"southstar-dock.png"]);
    surface.as_ref().write_to_png(&png).ok()?;
    Some(png)
}

pub fn set_dock_icon() {
    let Some(png) = render_icon_png() else {
        return;
    };
    let pool = objc_autoreleasePoolPush();
    // SAFETY: +[NSString stringWithUTF8String:] takes a C string and returns an object, and `png`
    // is NUL-terminated and outlives the call.
    let path = unsafe {
        send_ptr(
            class(c"NSString"),
            c"stringWithUTF8String:",
            png.as_ptr().cast(),
        )
    };
    // SAFETY: +[NSImage alloc] takes no arguments and returns an object.
    let image = unsafe { send(class(c"NSImage"), c"alloc") };
    // SAFETY: -[NSImage initWithContentsOfFile:] takes an NSString, and `path` is one or nil.
    let image = unsafe { send_ptr(image, c"initWithContentsOfFile:", path.cast_const()) };
    if !image.is_null() {
        // SAFETY: +[NSApplication sharedApplication] takes no arguments and returns an object.
        let app = unsafe { send(class(c"NSApplication"), c"sharedApplication") };
        // SAFETY: -[NSApplication setApplicationIconImage:] takes an NSImage, which `image` is.
        unsafe { send_void(app, c"setApplicationIconImage:", image) };
        // SAFETY: `image` is the object alloc/init returned, released exactly once here.
        unsafe { send_unit(image, c"release") };
    }
    // SAFETY: `pool` is the pool pushed above, popped exactly once.
    unsafe { objc_autoreleasePoolPop(pool) };
}
