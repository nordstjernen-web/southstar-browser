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
struct Matrix {
    xx: c_double,
    yx: c_double,
    xy: c_double,
    yy: c_double,
    x0: c_double,
    y0: c_double,
}

impl Matrix {
    fn from(m: [f64; 6]) -> Matrix {
        Matrix {
            xx: m[0],
            yx: m[1],
            xy: m[2],
            yy: m[3],
            x0: m[4],
            y0: m[5],
        }
    }
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
    fn cairo_set_source(cr: *mut Cairo, pattern: *mut c_void);
    fn cairo_set_line_width(cr: *mut Cairo, width: c_double);
    fn cairo_set_fill_rule(cr: *mut Cairo, rule: c_int);
    fn cairo_get_fill_rule(cr: *mut Cairo) -> c_int;
    fn cairo_fill(cr: *mut Cairo);
    fn cairo_stroke(cr: *mut Cairo);
    fn cairo_fill_preserve(cr: *mut Cairo);
    fn cairo_stroke_preserve(cr: *mut Cairo);
    fn cairo_set_line_cap(cr: *mut Cairo, cap: c_int);
    fn cairo_set_line_join(cr: *mut Cairo, join: c_int);
    fn cairo_set_miter_limit(cr: *mut Cairo, limit: c_double);
    fn cairo_set_dash(cr: *mut Cairo, dashes: *const c_double, num: c_int, offset: c_double);
    fn cairo_transform(cr: *mut Cairo, matrix: *const Matrix);
    fn cairo_set_matrix(cr: *mut Cairo, matrix: *const Matrix);
    fn cairo_get_matrix(cr: *mut Cairo, matrix: *mut Matrix);
    fn cairo_identity_matrix(cr: *mut Cairo);
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

unsafe extern "C" {
    fn cairo_pattern_create_linear(
        x0: c_double,
        y0: c_double,
        x1: c_double,
        y1: c_double,
    ) -> *mut c_void;
    fn cairo_pattern_create_radial(
        cx0: c_double,
        cy0: c_double,
        r0: c_double,
        cx1: c_double,
        cy1: c_double,
        r1: c_double,
    ) -> *mut c_void;
    fn cairo_pattern_create_for_surface(surface: *mut c_void) -> *mut c_void;
    fn cairo_pattern_create_mesh() -> *mut c_void;
    fn cairo_pattern_destroy(pattern: *mut c_void);
    fn cairo_pattern_set_extend(pattern: *mut c_void, extend: c_int);
    fn cairo_pattern_set_matrix(pattern: *mut c_void, matrix: *const Matrix);
    fn cairo_pattern_add_color_stop_rgba(
        pattern: *mut c_void,
        offset: c_double,
        r: c_double,
        g: c_double,
        b: c_double,
        a: c_double,
    );
    fn cairo_matrix_invert(matrix: *mut Matrix) -> c_int;
    fn cairo_mesh_pattern_begin_patch(pattern: *mut c_void);
    fn cairo_mesh_pattern_end_patch(pattern: *mut c_void);
    fn cairo_mesh_pattern_move_to(pattern: *mut c_void, x: c_double, y: c_double);
    fn cairo_mesh_pattern_line_to(pattern: *mut c_void, x: c_double, y: c_double);
    fn cairo_mesh_pattern_set_corner_color_rgba(
        pattern: *mut c_void,
        corner: c_uint,
        r: c_double,
        g: c_double,
        b: c_double,
        a: c_double,
    );
    fn cairo_clip(cr: *mut Cairo);
    fn cairo_paint_with_alpha(cr: *mut Cairo, alpha: c_double);
    fn cairo_get_source(cr: *mut Cairo) -> *mut c_void;
    fn cairo_pattern_set_filter(pattern: *mut c_void, filter: c_int);
    fn cairo_clip_preserve(cr: *mut Cairo);
    fn cairo_reset_clip(cr: *mut Cairo);
    fn cairo_in_fill(cr: *mut Cairo, x: c_double, y: c_double) -> c_int;
    fn cairo_in_stroke(cr: *mut Cairo, x: c_double, y: c_double) -> c_int;
}

pub(crate) struct Pattern(*mut c_void);

impl Pattern {
    pub fn linear(p0: (f64, f64), p1: (f64, f64)) -> Pattern {
        Pattern(unsafe { cairo_pattern_create_linear(p0.0, p0.1, p1.0, p1.1) })
    }

    pub fn radial(c0: (f64, f64, f64), c1: (f64, f64, f64)) -> Pattern {
        Pattern(unsafe { cairo_pattern_create_radial(c0.0, c0.1, c0.2, c1.0, c1.1, c1.2) })
    }

    pub fn for_surface(surface: &Surface) -> Pattern {
        Pattern(unsafe { cairo_pattern_create_for_surface(surface.0) })
    }

    pub fn conic(center: (f64, f64), sectors: &[crate::raster::Sector]) -> Pattern {
        let pattern = unsafe { cairo_pattern_create_mesh() };
        for sector in sectors {
            unsafe {
                cairo_mesh_pattern_begin_patch(pattern);
                cairo_mesh_pattern_move_to(pattern, center.0, center.1);
                cairo_mesh_pattern_line_to(pattern, sector.edges[0].0, sector.edges[0].1);
                cairo_mesh_pattern_line_to(pattern, sector.edges[1].0, sector.edges[1].1);
                cairo_mesh_pattern_line_to(pattern, center.0, center.1);
                for (corner, color) in [(0, 0), (1, 0), (2, 1), (3, 1)] {
                    let [r, g, b, a] = sector.colors[color];
                    cairo_mesh_pattern_set_corner_color_rgba(pattern, corner, r, g, b, a);
                }
                cairo_mesh_pattern_end_patch(pattern);
            }
        }
        Pattern(pattern)
    }

    pub fn set_extend(&self, extend: i32) {
        unsafe { cairo_pattern_set_extend(self.0, extend) };
    }

    pub fn set_inverse_matrix(&self, m: [f64; 6]) {
        let mut matrix = Matrix::from(m);
        if unsafe { cairo_matrix_invert(&mut matrix) } == 0 {
            unsafe { cairo_pattern_set_matrix(self.0, &matrix) };
        }
    }

    pub fn add_color_stop(&self, offset: f64, rgba: [f64; 4]) {
        unsafe {
            cairo_pattern_add_color_stop_rgba(self.0, offset, rgba[0], rgba[1], rgba[2], rgba[3])
        };
    }

    pub fn into_raw(self) -> *mut c_void {
        let raw = self.0;
        core::mem::forget(self);
        raw
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        unsafe { cairo_pattern_destroy(self.0) };
    }
}

impl Context {
    pub fn clip(self) {
        unsafe { cairo_clip(self.0) };
    }

    pub fn paint_with_alpha(self, alpha: f64) {
        unsafe { cairo_paint_with_alpha(self.0, alpha) };
    }

    pub fn set_source_filter(self, filter: i32) {
        unsafe { cairo_pattern_set_filter(cairo_get_source(self.0), filter) };
    }

    pub fn clip_preserve(self) {
        unsafe { cairo_clip_preserve(self.0) };
    }

    pub fn reset_clip(self) {
        unsafe { cairo_reset_clip(self.0) };
    }

    pub fn in_fill(self, x: f64, y: f64) -> bool {
        unsafe { cairo_in_fill(self.0, x, y) != 0 }
    }

    pub fn in_stroke(self, x: f64, y: f64) -> bool {
        unsafe { cairo_in_stroke(self.0, x, y) != 0 }
    }
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

    pub fn read_pixels<R>(&self, f: impl FnOnce(&[u8], usize, (i32, i32)) -> R) -> Option<R> {
        unsafe { cairo_surface_flush(self.0) };
        let data = unsafe { cairo_image_surface_get_data(self.0) };
        let stride = unsafe { cairo_image_surface_get_stride(self.0) };
        let size = self.size();
        if data.is_null() || stride <= 0 || size.1 <= 0 {
            return None;
        }
        let len = stride as usize * size.1 as usize;
        Some(f(
            unsafe { core::slice::from_raw_parts(data, len) },
            stride as usize,
            size,
        ))
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

pub(crate) struct OwnedContext(Context);

impl OwnedContext {
    pub fn on(surface: &Surface) -> OwnedContext {
        OwnedContext(Context(unsafe { cairo_create(surface.0) }))
    }

    pub fn context(&self) -> Context {
        self.0
    }
}

impl Drop for OwnedContext {
    fn drop(&mut self) {
        unsafe { cairo_destroy(self.0.0) };
    }
}

impl Path {
    pub unsafe fn from_raw(path: *mut CairoPathData) -> Option<Path> {
        (!path.is_null()).then_some(Path(path))
    }

    pub fn into_raw(self) -> *mut CairoPathData {
        let raw = self.0;
        core::mem::forget(self);
        raw
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

    pub fn raw(self) -> *mut Cairo {
        self.0
    }

    pub fn set_source_pattern(self, pattern: *mut c_void) {
        unsafe { cairo_set_source(self.0, pattern) };
    }

    pub fn set_source_rgba(self, rgba: [f64; 4]) {
        unsafe { cairo_set_source_rgba(self.0, rgba[0], rgba[1], rgba[2], rgba[3]) };
    }

    pub fn set_source_surface(self, surface: &Surface, x: f64, y: f64) {
        unsafe { cairo_set_source_surface(self.0, surface.0, x, y) };
    }

    pub fn paint(self) {
        unsafe { cairo_paint(self.0) };
    }

    pub fn set_line_width(self, width: f64) {
        unsafe { cairo_set_line_width(self.0, width) };
    }

    pub fn set_fill_rule(self, rule: i32) {
        unsafe { cairo_set_fill_rule(self.0, rule) };
    }

    pub fn fill_rule(self) -> i32 {
        unsafe { cairo_get_fill_rule(self.0) }
    }

    pub fn fill(self) {
        unsafe { cairo_fill(self.0) };
    }

    pub fn stroke(self) {
        unsafe { cairo_stroke(self.0) };
    }

    pub fn fill_preserve(self) {
        unsafe { cairo_fill_preserve(self.0) };
    }

    pub fn stroke_preserve(self) {
        unsafe { cairo_stroke_preserve(self.0) };
    }

    pub fn set_operator(self, op: i32) {
        unsafe { cairo_set_operator(self.0, op) };
    }

    pub fn set_line_cap(self, cap: i32) {
        unsafe { cairo_set_line_cap(self.0, cap) };
    }

    pub fn set_line_join(self, join: i32) {
        unsafe { cairo_set_line_join(self.0, join) };
    }

    pub fn set_miter_limit(self, limit: f64) {
        unsafe { cairo_set_miter_limit(self.0, limit) };
    }

    pub fn set_dash(self, dashes: &[f64], offset: f64) {
        let ptr = if dashes.is_empty() {
            core::ptr::null()
        } else {
            dashes.as_ptr()
        };
        unsafe { cairo_set_dash(self.0, ptr, dashes.len() as c_int, offset) };
    }

    pub fn transform(self, m: [f64; 6]) {
        unsafe { cairo_transform(self.0, &Matrix::from(m)) };
    }

    pub fn set_matrix(self, m: [f64; 6]) {
        unsafe { cairo_set_matrix(self.0, &Matrix::from(m)) };
    }

    pub fn matrix(self) -> [f64; 6] {
        let mut m = Matrix::from([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        unsafe { cairo_get_matrix(self.0, &mut m) };
        [m.xx, m.yx, m.xy, m.yy, m.x0, m.y0]
    }

    pub fn identity_matrix(self) {
        unsafe { cairo_identity_matrix(self.0) };
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
