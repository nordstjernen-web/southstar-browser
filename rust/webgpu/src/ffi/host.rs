//! Southstar — the C ABI of webgpu.h and the cairo, canvas and DOM calls WebGPU makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};
use core::ptr::{self, NonNull};

use southstar_dom::{Node, NsNode};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

unsafe extern "C" {
    fn ns_js_drawimage_source_surface(
        ctx: *mut JSContext,
        src: JSValue,
        out_w: *mut c_int,
        out_h: *mut c_int,
        threw: *mut c_int,
    ) -> *mut c_void;
    fn JS_GetException(ctx: *mut JSContext) -> JSValue;
    fn JS_ThrowInternalError(ctx: *mut JSContext, fmt: *const core::ffi::c_char, ...) -> JSValue;
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
    let (mut w, mut h, mut threw) = (0, 0, 0);
    let raw = unsafe {
        ns_js_drawimage_source_surface(
            quickjs::raw_context(scope),
            quickjs::raw(src),
            &mut w,
            &mut h,
            &mut threw,
        )
    };
    if threw != 0 {
        let raw = unsafe { JS_GetException(quickjs::raw_context(scope)) };
        return Err(unsafe { quickjs::take_value(scope, raw) });
    }
    Ok(NonNull::new(raw).map(|s| SourceImage {
        surface: Surface(s),
        w,
        h,
    }))
}

pub(crate) fn internal_error(scope: &mut Scope<'_>, message: &str) -> Value {
    let message = std::ffi::CString::new(message).unwrap_or_default();
    unsafe {
        JS_ThrowInternalError(
            quickjs::raw_context(scope),
            c"%s".as_ptr(),
            message.as_ptr(),
        )
    };
    quickjs::take_exception(scope)
}

pub(crate) fn canvas_attr(canvas: usize, name: &CStr) -> Option<Vec<u8>> {
    let node = unsafe { Node::from_ptr(canvas as *const NsNode) }?;
    node.attr(name).map(|text| text.to_bytes().to_vec())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_webgpu_install(
    ctx: *mut JSContext,
    _js: *mut c_void,
    navigator: JSValue,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let navigator = quickjs::borrow_value(scope, navigator);
            crate::install::install(scope, &navigator);
        });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_webgpu_get_context(
    ctx: *mut JSContext,
    _js: *mut c_void,
    canvas_obj: JSValue,
    canvas: *const NsNode,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let canvas_obj = quickjs::borrow_value(scope, canvas_obj);
            quickjs::into_raw(crate::context::get_context(
                scope,
                &canvas_obj,
                canvas as usize,
            ))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_webgpu_canvas_surface(canvas: *const NsNode) -> *mut c_void {
    crate::context::canvas_surface(canvas as usize).unwrap_or(ptr::null_mut())
}
