//! Southstar — the cairo calls painting makes, behind a copyable context handle, owned and borrowed surfaces and patterns, and matrices.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};

pub const FORMAT_ARGB32: c_int = 0;
pub const FORMAT_A8: c_int = 2;
pub const STATUS_SUCCESS: c_int = 0;
pub const CONTENT_ALPHA: c_int = 0x2000;

pub const OPERATOR_CLEAR: c_int = 0;
pub const OPERATOR_SOURCE: c_int = 1;
pub const OPERATOR_OVER: c_int = 2;
pub const OPERATOR_IN: c_int = 3;
pub const OPERATOR_OUT: c_int = 4;
pub const OPERATOR_XOR: c_int = 11;
pub const OPERATOR_MULTIPLY: c_int = 14;
pub const OPERATOR_SCREEN: c_int = 15;
pub const OPERATOR_OVERLAY: c_int = 16;
pub const OPERATOR_DARKEN: c_int = 17;
pub const OPERATOR_LIGHTEN: c_int = 18;
pub const OPERATOR_COLOR_DODGE: c_int = 19;
pub const OPERATOR_COLOR_BURN: c_int = 20;
pub const OPERATOR_HARD_LIGHT: c_int = 21;
pub const OPERATOR_SOFT_LIGHT: c_int = 22;
pub const OPERATOR_DIFFERENCE: c_int = 23;
pub const OPERATOR_EXCLUSION: c_int = 24;
pub const OPERATOR_HSL_HUE: c_int = 25;
pub const OPERATOR_HSL_SATURATION: c_int = 26;
pub const OPERATOR_HSL_COLOR: c_int = 27;
pub const OPERATOR_HSL_LUMINOSITY: c_int = 28;

pub const FILL_RULE_WINDING: c_int = 0;
pub const FILL_RULE_EVEN_ODD: c_int = 1;
pub const LINE_CAP_BUTT: c_int = 0;
pub const LINE_CAP_ROUND: c_int = 1;
pub const EXTEND_NONE: c_int = 0;
pub const EXTEND_REPEAT: c_int = 1;
pub const EXTEND_PAD: c_int = 3;
pub const FILTER_FAST: c_int = 0;
pub const FILTER_GOOD: c_int = 1;
pub const FILTER_NEAREST: c_int = 3;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix {
    pub xx: c_double,
    pub yx: c_double,
    pub xy: c_double,
    pub yy: c_double,
    pub x0: c_double,
    pub y0: c_double,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct TextExtents {
    pub x_bearing: c_double,
    pub y_bearing: c_double,
    pub width: c_double,
    pub height: c_double,
    pub x_advance: c_double,
    pub y_advance: c_double,
}

unsafe extern "C" {
    fn cairo_create(target: *mut c_void) -> *mut c_void;
    fn cairo_destroy(cr: *mut c_void);
    fn cairo_save(cr: *mut c_void);
    fn cairo_restore(cr: *mut c_void);
    fn cairo_get_target(cr: *mut c_void) -> *mut c_void;
    fn cairo_get_group_target(cr: *mut c_void) -> *mut c_void;
    fn cairo_push_group(cr: *mut c_void);
    fn cairo_push_group_with_content(cr: *mut c_void, content: c_int);
    fn cairo_pop_group(cr: *mut c_void) -> *mut c_void;
    fn cairo_pop_group_to_source(cr: *mut c_void);
    fn cairo_set_operator(cr: *mut c_void, op: c_int);
    fn cairo_get_operator(cr: *mut c_void) -> c_int;
    fn cairo_set_source(cr: *mut c_void, pattern: *mut c_void);
    fn cairo_get_source(cr: *mut c_void) -> *mut c_void;
    fn cairo_set_source_rgb(cr: *mut c_void, r: c_double, g: c_double, b: c_double);
    fn cairo_set_source_rgba(cr: *mut c_void, r: c_double, g: c_double, b: c_double, a: c_double);
    fn cairo_set_source_surface(cr: *mut c_void, surface: *mut c_void, x: c_double, y: c_double);
    fn cairo_set_line_width(cr: *mut c_void, width: c_double);
    fn cairo_set_line_cap(cr: *mut c_void, cap: c_int);
    fn cairo_set_fill_rule(cr: *mut c_void, rule: c_int);
    fn cairo_set_dash(cr: *mut c_void, dashes: *const c_double, n: c_int, offset: c_double);
    fn cairo_translate(cr: *mut c_void, tx: c_double, ty: c_double);
    fn cairo_scale(cr: *mut c_void, sx: c_double, sy: c_double);
    fn cairo_rotate(cr: *mut c_void, angle: c_double);
    fn cairo_transform(cr: *mut c_void, matrix: *const Matrix);
    fn cairo_set_matrix(cr: *mut c_void, matrix: *const Matrix);
    fn cairo_get_matrix(cr: *mut c_void, matrix: *mut Matrix);
    fn cairo_user_to_device(cr: *mut c_void, x: *mut c_double, y: *mut c_double);
    fn cairo_device_to_user(cr: *mut c_void, x: *mut c_double, y: *mut c_double);
    fn cairo_device_to_user_distance(cr: *mut c_void, dx: *mut c_double, dy: *mut c_double);
    fn cairo_new_path(cr: *mut c_void);
    fn cairo_new_sub_path(cr: *mut c_void);
    fn cairo_move_to(cr: *mut c_void, x: c_double, y: c_double);
    fn cairo_line_to(cr: *mut c_void, x: c_double, y: c_double);
    fn cairo_arc(
        cr: *mut c_void,
        xc: c_double,
        yc: c_double,
        r: c_double,
        a1: c_double,
        a2: c_double,
    );
    fn cairo_rectangle(cr: *mut c_void, x: c_double, y: c_double, w: c_double, h: c_double);
    fn cairo_close_path(cr: *mut c_void);
    fn cairo_paint(cr: *mut c_void);
    fn cairo_paint_with_alpha(cr: *mut c_void, alpha: c_double);
    fn cairo_mask(cr: *mut c_void, pattern: *mut c_void);
    fn cairo_mask_surface(cr: *mut c_void, surface: *mut c_void, x: c_double, y: c_double);
    fn cairo_stroke(cr: *mut c_void);
    fn cairo_fill(cr: *mut c_void);
    fn cairo_fill_preserve(cr: *mut c_void);
    fn cairo_clip(cr: *mut c_void);
    fn cairo_clip_extents(
        cr: *mut c_void,
        x1: *mut c_double,
        y1: *mut c_double,
        x2: *mut c_double,
        y2: *mut c_double,
    );
    fn cairo_set_font_size(cr: *mut c_void, size: c_double);
    fn cairo_text_extents(cr: *mut c_void, text: *const c_char, extents: *mut TextExtents);
    fn cairo_show_text(cr: *mut c_void, text: *const c_char);
    fn cairo_image_surface_create(format: c_int, width: c_int, height: c_int) -> *mut c_void;
    fn cairo_image_surface_get_data(surface: *mut c_void) -> *mut u8;
    fn cairo_image_surface_get_stride(surface: *mut c_void) -> c_int;
    fn cairo_image_surface_get_width(surface: *mut c_void) -> c_int;
    fn cairo_image_surface_get_height(surface: *mut c_void) -> c_int;
    fn cairo_surface_status(surface: *mut c_void) -> c_int;
    fn cairo_surface_destroy(surface: *mut c_void);
    fn cairo_surface_reference(surface: *mut c_void) -> *mut c_void;
    fn cairo_surface_flush(surface: *mut c_void);
    fn cairo_surface_mark_dirty(surface: *mut c_void);
    fn cairo_pattern_create_for_surface(surface: *mut c_void) -> *mut c_void;
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
    fn cairo_pattern_create_mesh() -> *mut c_void;
    fn cairo_pattern_reference(pattern: *mut c_void) -> *mut c_void;
    fn cairo_pattern_destroy(pattern: *mut c_void);
    fn cairo_pattern_set_extend(pattern: *mut c_void, extend: c_int);
    fn cairo_pattern_set_filter(pattern: *mut c_void, filter: c_int);
    fn cairo_pattern_set_matrix(pattern: *mut c_void, matrix: *const Matrix);
    fn cairo_pattern_add_color_stop_rgba(
        pattern: *mut c_void,
        offset: c_double,
        r: c_double,
        g: c_double,
        b: c_double,
        a: c_double,
    );
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
    fn cairo_matrix_init(
        m: *mut Matrix,
        xx: c_double,
        yx: c_double,
        xy: c_double,
        yy: c_double,
        x0: c_double,
        y0: c_double,
    );
    fn cairo_matrix_init_identity(m: *mut Matrix);
    fn cairo_matrix_init_scale(m: *mut Matrix, sx: c_double, sy: c_double);
    fn cairo_matrix_translate(m: *mut Matrix, tx: c_double, ty: c_double);
    fn cairo_matrix_scale(m: *mut Matrix, sx: c_double, sy: c_double);
    fn cairo_matrix_invert(m: *mut Matrix) -> c_int;
    fn cairo_matrix_multiply(result: *mut Matrix, a: *const Matrix, b: *const Matrix);
}

impl Matrix {
    pub fn new(xx: f64, yx: f64, xy: f64, yy: f64, x0: f64, y0: f64) -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_matrix_init(&mut m, xx, yx, xy, yy, x0, y0) };
        m
    }

    pub fn zero() -> Matrix {
        Matrix {
            xx: 0.0,
            yx: 0.0,
            xy: 0.0,
            yy: 0.0,
            x0: 0.0,
            y0: 0.0,
        }
    }

    pub fn identity() -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_matrix_init_identity(&mut m) };
        m
    }

    pub fn scaling(sx: f64, sy: f64) -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_matrix_init_scale(&mut m, sx, sy) };
        m
    }

    pub fn translate(&mut self, tx: f64, ty: f64) {
        unsafe { cairo_matrix_translate(self, tx, ty) };
    }

    pub fn scale(&mut self, sx: f64, sy: f64) {
        unsafe { cairo_matrix_scale(self, sx, sy) };
    }

    pub fn invert(&mut self) -> bool {
        unsafe { cairo_matrix_invert(self) == STATUS_SUCCESS }
    }

    pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
        let mut r = Matrix::zero();
        unsafe { cairo_matrix_multiply(&mut r, a, b) };
        r
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Cr(*mut c_void);

impl Cr {
    pub unsafe fn from_raw(cr: *mut c_void) -> Cr {
        Cr(cr)
    }

    pub fn raw(self) -> *mut c_void {
        self.0
    }

    pub fn save(self) {
        unsafe { cairo_save(self.0) };
    }

    pub fn restore(self) {
        unsafe { cairo_restore(self.0) };
    }

    pub fn target(self) -> *mut c_void {
        unsafe { cairo_get_target(self.0) }
    }

    pub fn group_target(self) -> *mut c_void {
        unsafe { cairo_get_group_target(self.0) }
    }

    pub fn push_group(self) {
        unsafe { cairo_push_group(self.0) };
    }

    pub fn push_group_alpha(self) {
        unsafe { cairo_push_group_with_content(self.0, CONTENT_ALPHA) };
    }

    pub fn pop_group(self) -> Pattern {
        Pattern(unsafe { cairo_pop_group(self.0) })
    }

    pub fn pop_group_to_source(self) {
        unsafe { cairo_pop_group_to_source(self.0) };
    }

    pub fn set_operator(self, op: c_int) {
        unsafe { cairo_set_operator(self.0, op) };
    }

    pub fn operator(self) -> c_int {
        unsafe { cairo_get_operator(self.0) }
    }

    pub fn set_source(self, pattern: &Pattern) {
        unsafe { cairo_set_source(self.0, pattern.0) };
    }

    pub fn source(self) -> Pattern {
        Pattern(unsafe { cairo_pattern_reference(cairo_get_source(self.0)) })
    }

    pub fn set_source_filter(self, filter: c_int) {
        unsafe { cairo_pattern_set_filter(cairo_get_source(self.0), filter) };
    }

    pub fn set_source_extend(self, extend: c_int) {
        unsafe { cairo_pattern_set_extend(cairo_get_source(self.0), extend) };
    }

    pub fn set_source_rgb(self, r: f64, g: f64, b: f64) {
        unsafe { cairo_set_source_rgb(self.0, r, g, b) };
    }

    pub fn set_source_rgba(self, r: f64, g: f64, b: f64, a: f64) {
        unsafe { cairo_set_source_rgba(self.0, r, g, b, a) };
    }

    pub fn set_source_surface(self, surface: SurfaceRef, x: f64, y: f64) {
        unsafe { cairo_set_source_surface(self.0, surface.0, x, y) };
    }

    pub fn set_line_width(self, width: f64) {
        unsafe { cairo_set_line_width(self.0, width) };
    }

    pub fn set_line_cap(self, cap: c_int) {
        unsafe { cairo_set_line_cap(self.0, cap) };
    }

    pub fn set_fill_rule(self, rule: c_int) {
        unsafe { cairo_set_fill_rule(self.0, rule) };
    }

    pub fn set_dash(self, dashes: &[f64]) {
        unsafe { cairo_set_dash(self.0, dashes.as_ptr(), dashes.len() as c_int, 0.0) };
    }

    pub fn translate(self, tx: f64, ty: f64) {
        unsafe { cairo_translate(self.0, tx, ty) };
    }

    pub fn scale(self, sx: f64, sy: f64) {
        unsafe { cairo_scale(self.0, sx, sy) };
    }

    pub fn rotate(self, angle: f64) {
        unsafe { cairo_rotate(self.0, angle) };
    }

    pub fn transform(self, m: &Matrix) {
        unsafe { cairo_transform(self.0, m) };
    }

    pub fn set_matrix(self, m: &Matrix) {
        unsafe { cairo_set_matrix(self.0, m) };
    }

    pub fn matrix(self) -> Matrix {
        let mut m = Matrix::zero();
        unsafe { cairo_get_matrix(self.0, &mut m) };
        m
    }

    pub fn user_to_device(self, x: f64, y: f64) -> (f64, f64) {
        let (mut x, mut y) = (x, y);
        unsafe { cairo_user_to_device(self.0, &mut x, &mut y) };
        (x, y)
    }

    pub fn device_to_user(self, x: f64, y: f64) -> (f64, f64) {
        let (mut x, mut y) = (x, y);
        unsafe { cairo_device_to_user(self.0, &mut x, &mut y) };
        (x, y)
    }

    pub fn device_to_user_distance(self, dx: f64, dy: f64) -> (f64, f64) {
        let (mut dx, mut dy) = (dx, dy);
        unsafe { cairo_device_to_user_distance(self.0, &mut dx, &mut dy) };
        (dx, dy)
    }

    pub fn new_path(self) {
        unsafe { cairo_new_path(self.0) };
    }

    pub fn new_sub_path(self) {
        unsafe { cairo_new_sub_path(self.0) };
    }

    pub fn move_to(self, x: f64, y: f64) {
        unsafe { cairo_move_to(self.0, x, y) };
    }

    pub fn line_to(self, x: f64, y: f64) {
        unsafe { cairo_line_to(self.0, x, y) };
    }

    pub fn arc(self, xc: f64, yc: f64, r: f64, a1: f64, a2: f64) {
        unsafe { cairo_arc(self.0, xc, yc, r, a1, a2) };
    }

    pub fn rectangle(self, x: f64, y: f64, w: f64, h: f64) {
        unsafe { cairo_rectangle(self.0, x, y, w, h) };
    }

    pub fn close_path(self) {
        unsafe { cairo_close_path(self.0) };
    }

    pub fn paint(self) {
        unsafe { cairo_paint(self.0) };
    }

    pub fn paint_with_alpha(self, alpha: f64) {
        unsafe { cairo_paint_with_alpha(self.0, alpha) };
    }

    pub fn mask(self, pattern: &Pattern) {
        unsafe { cairo_mask(self.0, pattern.0) };
    }

    pub fn mask_surface(self, surface: SurfaceRef, x: f64, y: f64) {
        unsafe { cairo_mask_surface(self.0, surface.0, x, y) };
    }

    pub fn stroke(self) {
        unsafe { cairo_stroke(self.0) };
    }

    pub fn fill(self) {
        unsafe { cairo_fill(self.0) };
    }

    pub fn fill_preserve(self) {
        unsafe { cairo_fill_preserve(self.0) };
    }

    pub fn clip(self) {
        unsafe { cairo_clip(self.0) };
    }

    pub fn clip_extents(self) -> (f64, f64, f64, f64) {
        let (mut x1, mut y1, mut x2, mut y2) = (0.0, 0.0, 0.0, 0.0);
        unsafe { cairo_clip_extents(self.0, &mut x1, &mut y1, &mut x2, &mut y2) };
        (x1, y1, x2, y2)
    }

    pub fn set_font_size(self, size: f64) {
        unsafe { cairo_set_font_size(self.0, size) };
    }

    pub fn text_extents(self, text: &CStr) -> TextExtents {
        let mut ext = TextExtents::default();
        unsafe { cairo_text_extents(self.0, text.as_ptr(), &mut ext) };
        ext
    }

    pub fn show_text(self, text: &CStr) {
        unsafe { cairo_show_text(self.0, text.as_ptr()) };
    }
}

pub struct Context(Cr);

impl Context {
    pub fn new(surface: SurfaceRef) -> Context {
        Context(Cr(unsafe { cairo_create(surface.0) }))
    }

    pub fn cr(&self) -> Cr {
        self.0
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe { cairo_destroy(self.0.0) };
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SurfaceRef(*mut c_void);

impl SurfaceRef {
    pub unsafe fn from_raw(surface: *mut c_void) -> Option<SurfaceRef> {
        (!surface.is_null()).then_some(SurfaceRef(surface))
    }

    pub fn raw(self) -> *mut c_void {
        self.0
    }

    pub fn width(self) -> i32 {
        unsafe { cairo_image_surface_get_width(self.0) }
    }

    pub fn height(self) -> i32 {
        unsafe { cairo_image_surface_get_height(self.0) }
    }

    pub fn stride(self) -> i32 {
        unsafe { cairo_image_surface_get_stride(self.0) }
    }

    pub fn flush(self) {
        unsafe { cairo_surface_flush(self.0) };
    }

    pub fn mark_dirty(self) {
        unsafe { cairo_surface_mark_dirty(self.0) };
    }

    pub fn data_mut(&mut self) -> &mut [u8] {
        let data = unsafe { cairo_image_surface_get_data(self.0) };
        let len = (self.stride().max(0) as usize) * (self.height().max(0) as usize);
        if data.is_null() || len == 0 {
            return &mut [];
        }
        unsafe { core::slice::from_raw_parts_mut(data, len) }
    }

    pub fn data(&self) -> &[u8] {
        let data = unsafe { cairo_image_surface_get_data(self.0) };
        let len = (self.stride().max(0) as usize) * (self.height().max(0) as usize);
        if data.is_null() || len == 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(data, len) }
    }

    pub fn new_ref(self) -> Surface {
        Surface(unsafe { cairo_surface_reference(self.0) })
    }
}

pub struct Surface(*mut c_void);

impl Surface {
    pub fn image(format: c_int, width: i32, height: i32) -> Option<Surface> {
        let surface = unsafe { cairo_image_surface_create(format, width, height) };
        if unsafe { cairo_surface_status(surface) } != STATUS_SUCCESS {
            unsafe { cairo_surface_destroy(surface) };
            return None;
        }
        Some(Surface(surface))
    }

    pub fn image_unchecked(format: c_int, width: i32, height: i32) -> Surface {
        Surface(unsafe { cairo_image_surface_create(format, width, height) })
    }

    pub unsafe fn from_raw_full(surface: *mut c_void) -> Surface {
        Surface(surface)
    }

    pub fn as_ref(&self) -> SurfaceRef {
        SurfaceRef(self.0)
    }

    pub fn into_raw(self) -> *mut c_void {
        let raw = self.0;
        core::mem::forget(self);
        raw
    }
}

impl Clone for Surface {
    fn clone(&self) -> Surface {
        Surface(unsafe { cairo_surface_reference(self.0) })
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe { cairo_surface_destroy(self.0) };
    }
}

pub struct Pattern(*mut c_void);

impl Pattern {
    pub fn for_surface(surface: SurfaceRef) -> Pattern {
        Pattern(unsafe { cairo_pattern_create_for_surface(surface.0) })
    }

    pub fn linear(x0: f64, y0: f64, x1: f64, y1: f64) -> Pattern {
        Pattern(unsafe { cairo_pattern_create_linear(x0, y0, x1, y1) })
    }

    pub fn radial(cx0: f64, cy0: f64, r0: f64, cx1: f64, cy1: f64, r1: f64) -> Pattern {
        Pattern(unsafe { cairo_pattern_create_radial(cx0, cy0, r0, cx1, cy1, r1) })
    }

    pub fn mesh() -> Pattern {
        Pattern(unsafe { cairo_pattern_create_mesh() })
    }

    pub fn raw(&self) -> *mut c_void {
        self.0
    }

    pub fn into_raw(self) -> *mut c_void {
        let raw = self.0;
        core::mem::forget(self);
        raw
    }

    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    pub fn set_extend(&self, extend: c_int) {
        unsafe { cairo_pattern_set_extend(self.0, extend) };
    }

    pub fn set_filter(&self, filter: c_int) {
        unsafe { cairo_pattern_set_filter(self.0, filter) };
    }

    pub fn set_matrix(&self, m: &Matrix) {
        unsafe { cairo_pattern_set_matrix(self.0, m) };
    }

    pub fn add_color_stop_rgba(&self, offset: f64, r: f64, g: f64, b: f64, a: f64) {
        unsafe { cairo_pattern_add_color_stop_rgba(self.0, offset, r, g, b, a) };
    }

    pub fn mesh_begin_patch(&self) {
        unsafe { cairo_mesh_pattern_begin_patch(self.0) };
    }

    pub fn mesh_end_patch(&self) {
        unsafe { cairo_mesh_pattern_end_patch(self.0) };
    }

    pub fn mesh_move_to(&self, x: f64, y: f64) {
        unsafe { cairo_mesh_pattern_move_to(self.0, x, y) };
    }

    pub fn mesh_line_to(&self, x: f64, y: f64) {
        unsafe { cairo_mesh_pattern_line_to(self.0, x, y) };
    }

    pub fn mesh_corner_rgba(&self, corner: u32, r: f64, g: f64, b: f64, a: f64) {
        unsafe { cairo_mesh_pattern_set_corner_color_rgba(self.0, corner, r, g, b, a) };
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { cairo_pattern_destroy(self.0) };
        }
    }
}
