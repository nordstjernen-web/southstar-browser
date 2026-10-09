//! Southstar — the cairo calls the canvas code makes, behind a copyable context handle and owned paths and recording surfaces.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_double, c_int, c_uint, c_void};
use core::ptr;

#[repr(C)]
pub struct Cairo {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PathHeader {
    kind: c_int,
    length: c_int,
}

#[repr(C)]
union PathData {
    header: PathHeader,
    point: [c_double; 2],
}

#[repr(C)]
pub struct CairoPathData {
    status: c_int,
    data: *mut PathData,
    num_data: c_int,
}

const CONTENT_COLOR_ALPHA: c_int = 0x3000;

unsafe extern "C" {
    fn cairo_create(target: *mut c_void) -> *mut Cairo;
    fn cairo_destroy(cr: *mut Cairo);
    fn cairo_recording_surface_create(content: c_int, extents: *const c_void) -> *mut c_void;
    fn cairo_surface_destroy(surface: *mut c_void);
    fn cairo_move_to(cr: *mut Cairo, x: c_double, y: c_double);
    fn cairo_line_to(cr: *mut Cairo, x: c_double, y: c_double);
    fn cairo_curve_to(
        cr: *mut Cairo,
        x1: c_double,
        y1: c_double,
        x2: c_double,
        y2: c_double,
        x3: c_double,
        y3: c_double,
    );
    fn cairo_close_path(cr: *mut Cairo);
    fn cairo_new_path(cr: *mut Cairo);
    fn cairo_new_sub_path(cr: *mut Cairo);
    fn cairo_save(cr: *mut Cairo);
    fn cairo_restore(cr: *mut Cairo);
    fn cairo_translate(cr: *mut Cairo, tx: c_double, ty: c_double);
    fn cairo_rotate(cr: *mut Cairo, angle: c_double);
    fn cairo_scale(cr: *mut Cairo, sx: c_double, sy: c_double);
    fn cairo_rectangle(cr: *mut Cairo, x: c_double, y: c_double, w: c_double, h: c_double);
    fn cairo_arc(
        cr: *mut Cairo,
        xc: c_double,
        yc: c_double,
        radius: c_double,
        angle1: c_double,
        angle2: c_double,
    );
    fn cairo_arc_negative(
        cr: *mut Cairo,
        xc: c_double,
        yc: c_double,
        radius: c_double,
        angle1: c_double,
        angle2: c_double,
    );
    fn cairo_has_current_point(cr: *mut Cairo) -> c_int;
    fn cairo_get_current_point(cr: *mut Cairo, x: *mut c_double, y: *mut c_double);
    fn cairo_copy_path(cr: *mut Cairo) -> *mut CairoPathData;
    fn cairo_append_path(cr: *mut Cairo, path: *const CairoPathData);
    fn cairo_path_destroy(path: *mut CairoPathData);
    fn cairo_image_surface_create(format: c_int, width: c_int, height: c_int) -> *mut c_void;
    fn cairo_image_surface_get_width(surface: *mut c_void) -> c_int;
    fn cairo_image_surface_get_height(surface: *mut c_void) -> c_int;
    fn cairo_image_surface_get_data(surface: *mut c_void) -> *mut u8;
    fn cairo_image_surface_get_stride(surface: *mut c_void) -> c_int;
    fn cairo_surface_status(surface: *mut c_void) -> c_int;
    fn cairo_surface_flush(surface: *mut c_void);
    fn cairo_surface_mark_dirty(surface: *mut c_void);
    fn cairo_surface_reference(surface: *mut c_void) -> *mut c_void;
    fn cairo_set_source_surface(cr: *mut Cairo, surface: *mut c_void, x: c_double, y: c_double);
    fn cairo_set_operator(cr: *mut Cairo, op: c_int);
    fn cairo_paint(cr: *mut Cairo);
    fn cairo_set_source_rgba(cr: *mut Cairo, r: c_double, g: c_double, b: c_double, a: c_double);
    fn cairo_surface_write_to_png_stream(
        surface: *mut c_void,
        write: unsafe extern "C" fn(*mut c_void, *const u8, c_uint) -> c_int,
        closure: *mut c_void,
    ) -> c_int;
}

unsafe extern "C" fn collect_png(closure: *mut c_void, data: *const u8, length: c_uint) -> c_int {
    let out = unsafe { &mut *closure.cast::<Vec<u8>>() };
    out.extend_from_slice(unsafe { core::slice::from_raw_parts(data, length as usize) });
    0
}

const FORMAT_ARGB32: c_int = 0;

const OPERATOR_SOURCE: c_int = 1;

pub(crate) struct Surface(*mut c_void);

impl Surface {
    pub unsafe fn from_raw(surface: *mut c_void) -> Option<Surface> {
        (!surface.is_null()).then_some(Surface(surface))
    }

    pub unsafe fn from_borrowed(surface: *mut c_void) -> Option<Surface> {
        (!surface.is_null()).then(|| Surface(unsafe { cairo_surface_reference(surface) }))
    }

    pub fn into_raw(self) -> *mut c_void {
        let raw = self.0;
        core::mem::forget(self);
        raw
    }

    pub fn reference(&self) -> Surface {
        Surface(unsafe { cairo_surface_reference(self.0) })
    }

    pub fn image(width: i32, height: i32) -> Surface {
        Surface(unsafe { cairo_image_surface_create(FORMAT_ARGB32, width, height) })
    }

    pub fn is_ok(&self) -> bool {
        unsafe { cairo_surface_status(self.0) == 0 }
    }

    pub fn size(&self) -> (i32, i32) {
        unsafe {
            (
                cairo_image_surface_get_width(self.0),
                cairo_image_surface_get_height(self.0),
            )
        }
    }

    pub fn write_pixels(&self, f: impl FnOnce(&mut [u8], usize)) {
        unsafe { cairo_surface_flush(self.0) };
        let data = unsafe { cairo_image_surface_get_data(self.0) };
        let stride = unsafe { cairo_image_surface_get_stride(self.0) };
        let (_, height) = self.size();
        if data.is_null() || stride <= 0 || height <= 0 {
            return;
        }
        let len = stride as usize * height as usize;
        f(
            unsafe { core::slice::from_raw_parts_mut(data, len) },
            stride as usize,
        );
        unsafe { cairo_surface_mark_dirty(self.0) };
    }

    pub fn png(&self) -> Option<Vec<u8>> {
        let mut out: Vec<u8> = Vec::new();
        let closure = (&mut out as *mut Vec<u8>).cast::<c_void>();
        let status = unsafe { cairo_surface_write_to_png_stream(self.0, collect_png, closure) };
        (status == 0).then_some(out)
    }

    pub fn fill_opaque_black(&self) {
        unsafe {
            let cr = cairo_create(self.0);
            cairo_set_operator(cr, OPERATOR_SOURCE);
            cairo_set_source_rgba(cr, 0.0, 0.0, 0.0, 1.0);
            cairo_paint(cr);
            cairo_destroy(cr);
        }
    }

    pub fn paint_onto(&self, target: &Surface, offset: (f64, f64), replace: bool) {
        unsafe {
            let cr = cairo_create(target.0);
            cairo_set_source_surface(cr, self.0, offset.0, offset.1);
            if replace {
                cairo_set_operator(cr, OPERATOR_SOURCE);
            }
            cairo_paint(cr);
            cairo_destroy(cr);
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { cairo_surface_destroy(self.0) };
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Context(*mut Cairo);

pub(crate) struct Path(*mut CairoPathData);

impl Path {
    pub fn is_ok(&self) -> bool {
        unsafe { self.0.as_ref() }.is_some_and(|path| path.status == 0)
    }

    pub fn transform(&mut self, m: [f64; 6]) {
        let Some(path) = (unsafe { self.0.as_mut() }) else {
            return;
        };
        if path.data.is_null() || path.num_data <= 0 {
            return;
        }
        let data = unsafe { core::slice::from_raw_parts_mut(path.data, path.num_data as usize) };
        let total = data.len();
        let mut i = 0;
        while i < total {
            let length = unsafe { data[i].header.length }.max(1) as usize;
            let end = (i + length).min(total);
            for entry in &mut data[i + 1..end] {
                let [x, y] = unsafe { entry.point };
                entry.point = [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
            }
            i += length;
        }
    }
}

impl Drop for Path {
    fn drop(&mut self) {
        unsafe { cairo_path_destroy(self.0) };
    }
}

pub(crate) struct Recording {
    surface: *mut c_void,
    cr: *mut Cairo,
}

impl Recording {
    pub fn new() -> Recording {
        let surface = unsafe { cairo_recording_surface_create(CONTENT_COLOR_ALPHA, ptr::null()) };
        let cr = unsafe { cairo_create(surface) };
        Recording { surface, cr }
    }

    pub fn context(&self) -> Context {
        Context(self.cr)
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        unsafe {
            cairo_destroy(self.cr);
            cairo_surface_destroy(self.surface);
        }
    }
}

impl Context {
    pub unsafe fn from_raw(cr: *mut Cairo) -> Option<Context> {
        (!cr.is_null()).then_some(Context(cr))
    }

    pub fn move_to(self, x: f64, y: f64) {
        unsafe { cairo_move_to(self.0, x, y) };
    }

    pub fn line_to(self, x: f64, y: f64) {
        unsafe { cairo_line_to(self.0, x, y) };
    }

    pub fn curve_to(self, c1: (f64, f64), c2: (f64, f64), to: (f64, f64)) {
        unsafe { cairo_curve_to(self.0, c1.0, c1.1, c2.0, c2.1, to.0, to.1) };
    }

    pub fn close_path(self) {
        unsafe { cairo_close_path(self.0) };
    }

    pub fn new_path(self) {
        unsafe { cairo_new_path(self.0) };
    }

    pub fn new_sub_path(self) {
        unsafe { cairo_new_sub_path(self.0) };
    }

    pub fn save(self) {
        unsafe { cairo_save(self.0) };
    }

    pub fn restore(self) {
        unsafe { cairo_restore(self.0) };
    }

    pub fn translate(self, x: f64, y: f64) {
        unsafe { cairo_translate(self.0, x, y) };
    }

    pub fn rotate(self, angle: f64) {
        unsafe { cairo_rotate(self.0, angle) };
    }

    pub fn scale(self, x: f64, y: f64) {
        unsafe { cairo_scale(self.0, x, y) };
    }

    pub fn rectangle(self, x: f64, y: f64, w: f64, h: f64) {
        unsafe { cairo_rectangle(self.0, x, y, w, h) };
    }

    pub fn arc(self, center: (f64, f64), radius: f64, angles: (f64, f64), negative: bool) {
        let (xc, yc) = center;
        let (a1, a2) = angles;
        unsafe {
            if negative {
                cairo_arc_negative(self.0, xc, yc, radius, a1, a2);
            } else {
                cairo_arc(self.0, xc, yc, radius, a1, a2);
            }
        }
    }

    pub fn has_current_point(self) -> bool {
        unsafe { cairo_has_current_point(self.0) != 0 }
    }

    pub fn current_point(self) -> (f64, f64) {
        let (mut x, mut y) = (0.0, 0.0);
        unsafe { cairo_get_current_point(self.0, &mut x, &mut y) };
        (x, y)
    }

    pub fn copy_path(self) -> Path {
        Path(unsafe { cairo_copy_path(self.0) })
    }

    pub fn append_path(self, path: &Path) {
        if path.is_ok() {
            unsafe { cairo_append_path(self.0, path.0) };
        }
    }
}

impl crate::path::PathSink for Context {
    fn move_to(&mut self, x: f64, y: f64) {
        Context::move_to(*self, x, y);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        Context::line_to(*self, x, y);
    }

    fn curve_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        Context::curve_to(*self, (x1, y1), (x2, y2), (x, y));
    }

    fn close_path(&mut self) {
        Context::close_path(*self);
    }

    fn unit_arc(
        &mut self,
        center: (f64, f64),
        phi: f64,
        radii: (f64, f64),
        angles: (f64, f64),
        sweep: bool,
    ) {
        self.save();
        self.translate(center.0, center.1);
        self.rotate(phi);
        self.scale(radii.0, radii.1);
        self.arc((0.0, 0.0), 1.0, angles, !sweep);
        self.restore();
    }
}
