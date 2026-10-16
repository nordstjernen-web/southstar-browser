//! Southstar — encoding a frame the renderer client returned as PNG for the headless driver.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};

use southstar_renderer_client::Frame;

#[repr(C)]
struct CairoSurface {
    _private: [u8; 0],
}

type CairoWrite = unsafe extern "C" fn(closure: *mut c_void, data: *const u8, length: u32) -> c_int;

const CAIRO_FORMAT_ARGB32: c_int = 0;
const CAIRO_STATUS_SUCCESS: c_int = 0;

unsafe extern "C" {
    fn cairo_image_surface_create_for_data(
        data: *mut u8,
        format: c_int,
        width: c_int,
        height: c_int,
        stride: c_int,
    ) -> *mut CairoSurface;
    fn cairo_surface_status(surface: *mut CairoSurface) -> c_int;
    fn cairo_surface_write_to_png_stream(
        surface: *mut CairoSurface,
        write: CairoWrite,
        closure: *mut c_void,
    ) -> c_int;
    fn cairo_surface_destroy(surface: *mut CairoSurface);
}

pub fn frame_size(frame: &Frame<'_>) -> Option<(c_int, c_int)> {
    (!frame.pixels.is_empty()).then_some((frame.width, frame.height))
}

fn covers(frame: &Frame<'_>) -> bool {
    let (Ok(stride), Ok(height)) = (usize::try_from(frame.stride), usize::try_from(frame.height))
    else {
        return false;
    };
    stride
        .checked_mul(height)
        .is_some_and(|len| len <= frame.pixels.len())
}

pub fn encode_png(frame: &Frame<'_>) -> Option<Vec<u8>> {
    if !covers(frame) {
        return None;
    }
    // SAFETY: the pixels hold stride * height bytes, and cairo only reads them while the surface lives inside this function.
    let surface = unsafe {
        cairo_image_surface_create_for_data(
            frame.pixels.as_ptr().cast_mut(),
            CAIRO_FORMAT_ARGB32,
            frame.width,
            frame.height,
            frame.stride,
        )
    };
    let mut png = Vec::new();
    // SAFETY: cairo returns a surface, possibly an error surface, that this function owns.
    let ok = unsafe { cairo_surface_status(surface) } == CAIRO_STATUS_SUCCESS;
    if ok {
        // SAFETY: png outlives the call, and png_sink only appends to the Vec it is handed.
        unsafe {
            cairo_surface_write_to_png_stream(surface, png_sink, (&mut png as *mut Vec<u8>).cast())
        };
    }
    // SAFETY: surface is this function's own reference, released exactly once here.
    unsafe { cairo_surface_destroy(surface) };
    ok.then_some(png)
}

unsafe extern "C" fn png_sink(closure: *mut c_void, data: *const u8, length: u32) -> c_int {
    // SAFETY: closure is the Vec encode_png passed, borrowed by nothing else during the write.
    let png = unsafe { &mut *closure.cast::<Vec<u8>>() };
    // SAFETY: cairo hands over length readable bytes at data.
    png.extend_from_slice(unsafe { core::slice::from_raw_parts(data, length as usize) });
    CAIRO_STATUS_SUCCESS
}
