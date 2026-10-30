//! Southstar — printing through the renderer: the in-process print hook and the GLib and Cairo calls that load the printed sheets.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint, c_void};
use std::sync::Mutex;

use southstar_cairo::Surface;
use southstar_glib as glib;

use crate::client::Renderer;
use crate::os::InprocId;
use crate::{os, sheet};

pub use southstar_print::PrintSetup;

pub struct PrintedPages {
    pub sheets: Vec<Surface>,
    pub scale: f64,
}

#[repr(C)]
struct CairoSurface {
    _private: [u8; 0],
}

type ReadFn = unsafe extern "C" fn(closure: *mut c_void, data: *mut u8, length: c_uint) -> c_int;

const CAIRO_STATUS_SUCCESS: c_int = 0;
const CAIRO_STATUS_READ_ERROR: c_int = 10;

pub type PrintFn = fn(conn: InprocId, setup: &mut PrintSetup) -> Option<Vec<Surface>>;

static PRINT: Mutex<Option<PrintFn>> = Mutex::new(None);

unsafe extern "C" {
    fn cairo_image_surface_create_from_png_stream(
        read: ReadFn,
        closure: *mut c_void,
    ) -> *mut CairoSurface;
}

pub fn set_print(print: Option<PrintFn>) {
    *PRINT.lock().unwrap_or_else(|e| e.into_inner()) = print;
}

pub fn exchange_dir() -> Vec<u8> {
    let mut path = glib::user_runtime_dir();
    path.extend_from_slice(b"/southstar");
    let _ = std::fs::create_dir_all(String::from_utf8_lossy(&path).as_ref());
    path
}

fn print_prefix() -> Vec<u8> {
    let mut prefix = exchange_dir();
    let now = southstar_glib::monotonic_us();
    prefix.extend_from_slice(
        format!("/southstar-print-{now}-{}", os::next_print_counter()).as_bytes(),
    );
    prefix
}

struct PngSource<'a> {
    rest: &'a [u8],
}

unsafe extern "C" fn read_png(closure: *mut c_void, data: *mut u8, length: c_uint) -> c_int {
    // SAFETY: closure is the PngSource load_sheet lends cairo for the duration of the decode.
    let source = unsafe { &mut *closure.cast::<PngSource<'_>>() };
    let length = length as usize;
    if length > source.rest.len() {
        return CAIRO_STATUS_READ_ERROR;
    }
    let (chunk, rest) = source.rest.split_at(length);
    // SAFETY: cairo passes a writable buffer of length bytes.
    let out = unsafe { core::slice::from_raw_parts_mut(data, length) };
    out.copy_from_slice(chunk);
    source.rest = rest;
    CAIRO_STATUS_SUCCESS
}

fn load_sheet(sheets: &mut Vec<Surface>, png: &[u8]) -> bool {
    let mut source = PngSource { rest: png };
    let closure = (&raw mut source).cast::<c_void>();
    // SAFETY: read_png only reads from source, which outlives this call.
    let img = unsafe { cairo_image_surface_create_from_png_stream(read_png, closure) };
    // SAFETY: cairo returns a new reference, possibly to an error surface, which the Surface takes over.
    let Some(img) = (unsafe { Surface::from_raw_full(img.cast()) }) else {
        return false;
    };
    let loaded = img.as_ref().status().is_ok();
    if loaded {
        sheets.push(img);
    }
    loaded
}

pub fn print(r: &mut Renderer, setup: &mut PrintSetup) -> Option<PrintedPages> {
    let hook = *PRINT.lock().unwrap_or_else(|e| e.into_inner());
    if let (Some(conn), Some(print)) = (r.inproc_conn, hook) {
        let sheets = print(conn, setup)?;
        return Some(PrintedPages { sheets, scale: 1.0 });
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
    let mut sheets = Vec::new();
    let pages = usize::try_from(reply.pages)
        .unwrap_or(0)
        .min(sheet::MAX_SHEETS);
    let mut loading = true;
    let mut pixels = 0u64;
    for i in 0..pages {
        let mut path = prefix.clone();
        path.extend_from_slice(format!("-{i}.png").as_bytes());
        let Some(path) = sheet::path_from(&path) else {
            break;
        };
        if loading {
            let png = sheet::read(&path);
            let size = png.as_deref().and_then(sheet::png_pixels);
            loading = match (png, size) {
                (Some(png), Some(size)) if pixels + size <= sheet::MAX_TOTAL_PIXELS => {
                    pixels += size;
                    load_sheet(&mut sheets, &png)
                }
                _ => false,
            };
        }
        let _ = std::fs::remove_file(&path);
    }
    if sheets.is_empty() {
        return None;
    }
    let scale = if reply.scale > 0.0 { reply.scale } else { 1.0 };
    Some(PrintedPages { sheets, scale })
}
