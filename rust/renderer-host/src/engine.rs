//! Southstar — the engine as the renderer host sees it: the browser crate's page API plus the favicon fetch, the network log and print sheets written out as PNG.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_cairo::{Context, Format, Surface};

pub use southstar_browser::api::{
    LayerPlan, LayerTarget, Page, Post, PrintPages, PrintSetup, TileTarget, VpLayerInfo, init,
    print_setup_default, sandbox, set_color_scheme, set_device_pixel_ratio, set_next_initiator,
    set_next_referrer, set_next_user_activated, shutdown,
};

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
    let img = Surface::image_unchecked(Format::Rgb24, width, height);
    let context = Context::new(img.as_ref());
    let cr = context.as_ref();
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.paint();
    cr.scale(scale, scale);
    cr.set_source_surface(sheet, 0.0, 0.0);
    cr.paint();
    drop(context);
    img.as_ref().write_to_png(path).is_ok()
}
