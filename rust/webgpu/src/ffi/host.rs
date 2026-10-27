//! Southstar — WebGPU's entry points and the cairo, canvas and DOM calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};
use core::ptr::{self, NonNull};

use southstar_canvas::host as canvas;
use southstar_dom::{Node, NsNode};
use southstar_js_engine::{Scope, Value};

unsafe extern "C" {
    fn cairo_image_surface_create(format: c_int, w: c_int, h: c_int) -> *mut c_void;
    fn cairo_surface_status(s: *mut c_void) -> c_int;
    fn cairo_surface_destroy(s: *mut c_void);
    fn cairo_surface_flush(s: *mut c_void);
    fn cairo_surface_mark_dirty(s: *mut c_void);
    fn cairo_image_surface_get_data(s: *mut c_void) -> *mut u8;
    fn cairo_image_surface_get_stride(s: *mut c_void) -> c_int;
    fn cairo_image_surface_get_height(s: *mut c_void) -> c_int;
}

const CAIRO_FORMAT_ARGB32: c_int = 0;
const CAIRO_STATUS_SUCCESS: c_int = 0;

pub(crate) struct Surface(NonNull<c_void>);

impl Surface {
    pub(crate) fn new(w: i32, h: i32) -> Option<Surface> {
        let raw = unsafe { cairo_image_surface_create(CAIRO_FORMAT_ARGB32, w, h) };
        NonNull::new(raw).map(Surface)
    }

    pub(crate) fn ok(&self) -> bool {
        unsafe { cairo_surface_status(self.as_ptr()) == CAIRO_STATUS_SUCCESS }
    }

    pub(crate) fn as_ptr(&self) -> *mut c_void {
        self.0.as_ptr()
    }

    pub(crate) fn flush(&self) {
        unsafe { cairo_surface_flush(self.as_ptr()) };
    }

    pub(crate) fn mark_dirty(&self) {
        unsafe { cairo_surface_mark_dirty(self.as_ptr()) };
    }

    pub(crate) fn pixels(&mut self) -> Option<(&mut [u8], usize)> {
        let raw = self.as_ptr();
        let data = unsafe { cairo_image_surface_get_data(raw) };
        let stride = unsafe { cairo_image_surface_get_stride(raw) };
        let height = unsafe { cairo_image_surface_get_height(raw) };
        if data.is_null() || stride <= 0 || height <= 0 {
            return None;
        }
        let len = stride as usize * height as usize;
        Some((
            unsafe { core::slice::from_raw_parts_mut(data, len) },
            stride as usize,
        ))
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { cairo_surface_destroy(self.as_ptr()) };
    }
}

pub(crate) struct SourceImage {
    surface: Surface,
    pub(crate) w: i32,
    pub(crate) h: i32,
}

impl SourceImage {
    pub(crate) fn pixels(&mut self) -> Option<(&[u8], usize)> {
        self.surface.pixels().map(|(data, stride)| (&*data, stride))
    }
}

pub(crate) fn drawimage_source(
    scope: &mut Scope<'_>,
    src: &Value,
) -> Result<Option<SourceImage>, Value> {
    let source = canvas::drawimage_source_surface(scope, src)?;
    Ok(source.and_then(|(raw, w, h)| {
        NonNull::new(raw).map(|s| SourceImage {
            surface: Surface(s),
            w,
            h,
        })
    }))
}

pub(crate) fn internal_error(scope: &mut Scope<'_>, message: &str) -> Value {
    scope.internal_error(message)
}

pub(crate) fn canvas_attr(canvas: usize, name: &CStr) -> Option<Vec<u8>> {
    let node = unsafe { Node::from_ptr(canvas as *const NsNode) }?;
    node.attr(name).map(|text| text.to_bytes().to_vec())
}

pub(crate) fn canvas_surface(canvas: usize) -> *mut c_void {
    crate::context::canvas_surface(canvas).unwrap_or(ptr::null_mut())
}
