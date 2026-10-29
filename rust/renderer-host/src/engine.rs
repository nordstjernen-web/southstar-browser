//! Southstar — the engine as the renderer host sees it: the browser crate's page API plus the favicon fetch, the network log and print sheets written out as PNG.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use std::ffi::CString;

pub use southstar_browser::api::{
    LayerPlan, LayerTarget, Page, Post, PrintPages, PrintSetup, TileTarget, VpLayerInfo, init,
    print_setup_default, sandbox, set_color_scheme, set_device_pixel_ratio, set_next_initiator,
    set_next_referrer, set_next_user_activated, shutdown,
};

#[repr(C)]
struct CairoSurface {
    _private: [u8; 0],
}

#[repr(C)]
struct Cairo {
    _private: [u8; 0],
}

const CAIRO_FORMAT_RGB24: c_int = 1;
const CAIRO_STATUS_SUCCESS: c_int = 0;

unsafe extern "C" {

    fn cairo_image_surface_create(format: c_int, width: c_int, height: c_int) -> *mut CairoSurface;
    fn cairo_create(target: *mut CairoSurface) -> *mut Cairo;
    fn cairo_set_source_rgb(cr: *mut Cairo, r: f64, g: f64, b: f64);
    fn cairo_paint(cr: *mut Cairo);
    fn cairo_scale(cr: *mut Cairo, sx: f64, sy: f64);
    fn cairo_set_source_surface(cr: *mut Cairo, surface: *mut CairoSurface, x: f64, y: f64);
    fn cairo_destroy(cr: *mut Cairo);
    fn cairo_surface_write_to_png(surface: *mut CairoSurface, filename: *const c_char) -> c_int;
    fn cairo_surface_destroy(surface: *mut CairoSurface);
}

pub fn cstring(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub fn net_idle() -> bool {
    southstar_net::idle()
}

pub fn monotonic_us() -> i64 {
    southstar_glib::monotonic_us()
}

pub fn net_log_clear() {
    southstar_net::log_clear();
}

pub fn net_log_dump() -> Option<Vec<u8>> {
    Some(southstar_net::log_dump())
}

pub fn same_origin(a: &CStr, b: &CStr) -> bool {
    southstar_url::bytes::same_origin(a.to_bytes(), b.to_bytes())
}

pub struct Favicon {
    pub width: c_int,
    pub height: c_int,
    pub pixels: Vec<u8>,
}

pub fn fetch_favicon(url: &CStr) -> Option<Favicon> {
    let resp = southstar_net::fetch_blocking(url)?;
    let body = resp
        .body()
        .filter(|body| resp.status() < 400 && !body.is_empty())?;
    let decoded = southstar_image::decode_bytes(body);
    drop(resp);
    let (tex, w, h) = decoded?;
    if !(1..=512).contains(&w) || !(1..=512).contains(&h) {
        return None;
    }
    let stride = w as usize * 4;
    let mut pixels = vec![0u8; stride * h as usize];
    tex.download(&mut pixels, stride);
    Some(Favicon {
        width: w,
        height: h,
        pixels,
    })
}

pub fn write_png(
    pages: &PrintPages,
    index: usize,
    width: c_int,
    height: c_int,
    scale: f64,
    path: &CStr,
) -> bool {
    let Some(sheet) = pages.sheet(index) else {
        return false;
    };
    unsafe {
        let img = cairo_image_surface_create(CAIRO_FORMAT_RGB24, width, height);
        let cr = cairo_create(img);
        cairo_set_source_rgb(cr, 1.0, 1.0, 1.0);
        cairo_paint(cr);
        cairo_scale(cr, scale, scale);
        cairo_set_source_surface(cr, sheet.as_ptr().cast(), 0.0, 0.0);
        cairo_paint(cr);
        cairo_destroy(cr);
        let ok = cairo_surface_write_to_png(img, path.as_ptr()) == CAIRO_STATUS_SUCCESS;
        cairo_surface_destroy(img);
        ok
    }
}
