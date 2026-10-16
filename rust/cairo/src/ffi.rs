//! Southstar — the libcairo declarations, made once for every drawing crate, and the safe wrappers over them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod context;
mod font_options;
mod matrix;
mod path;
mod pattern;
mod pixels;
mod surface;

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};

pub use context::{Context, ContextRef};
pub use font_options::{FontOptions, FontOptionsRef};
pub use path::Path;
pub(crate) use path::PathData;
#[cfg(test)]
pub(crate) use path::{PathHeader, decode, transform_points};
pub use pattern::Pattern;
pub use surface::{Surface, SurfaceRef, encode_png};

use crate::types::{Error, Matrix, Rectangle, TextExtents};

#[repr(C)]
struct Cairo {
    _private: [u8; 0],
}

#[repr(C)]
struct RawSurface {
    _private: [u8; 0],
}

#[repr(C)]
struct RawPattern {
    _private: [u8; 0],
}

#[repr(C)]
struct RawFontOptions {
    _private: [u8; 0],
}

#[repr(C)]
struct RawPath {
    status: c_int,
    data: *mut PathData,
    num_data: c_int,
}

type WriteFunc =
    unsafe extern "C" fn(closure: *mut c_void, data: *const u8, length: c_uint) -> c_int;

unsafe extern "C" {
    fn cairo_create(target: *mut RawSurface) -> *mut Cairo;
    fn cairo_reference(cr: *mut Cairo) -> *mut Cairo;
    fn cairo_destroy(cr: *mut Cairo);
    fn cairo_status(cr: *mut Cairo) -> c_int;
    fn cairo_save(cr: *mut Cairo);
    fn cairo_restore(cr: *mut Cairo);
    fn cairo_get_target(cr: *mut Cairo) -> *mut RawSurface;
    fn cairo_get_group_target(cr: *mut Cairo) -> *mut RawSurface;
    fn cairo_push_group(cr: *mut Cairo);
    fn cairo_push_group_with_content(cr: *mut Cairo, content: c_int);
    fn cairo_pop_group(cr: *mut Cairo) -> *mut RawPattern;
    fn cairo_pop_group_to_source(cr: *mut Cairo);
    fn cairo_set_operator(cr: *mut Cairo, op: c_int);
    fn cairo_get_operator(cr: *mut Cairo) -> c_int;
    fn cairo_set_source(cr: *mut Cairo, source: *mut RawPattern);
    fn cairo_get_source(cr: *mut Cairo) -> *mut RawPattern;
    fn cairo_set_source_rgb(cr: *mut Cairo, r: c_double, g: c_double, b: c_double);
    fn cairo_set_source_rgba(cr: *mut Cairo, r: c_double, g: c_double, b: c_double, a: c_double);
    fn cairo_set_source_surface(cr: *mut Cairo, surface: *mut RawSurface, x: c_double, y: c_double);
    fn cairo_set_tolerance(cr: *mut Cairo, tolerance: c_double);
    fn cairo_set_antialias(cr: *mut Cairo, antialias: c_int);
    fn cairo_set_fill_rule(cr: *mut Cairo, rule: c_int);
    fn cairo_get_fill_rule(cr: *mut Cairo) -> c_int;
    fn cairo_set_line_width(cr: *mut Cairo, width: c_double);
    fn cairo_set_line_cap(cr: *mut Cairo, cap: c_int);
    fn cairo_set_line_join(cr: *mut Cairo, join: c_int);
    fn cairo_set_dash(cr: *mut Cairo, dashes: *const c_double, num: c_int, offset: c_double);
    fn cairo_set_miter_limit(cr: *mut Cairo, limit: c_double);
    fn cairo_translate(cr: *mut Cairo, tx: c_double, ty: c_double);
    fn cairo_scale(cr: *mut Cairo, sx: c_double, sy: c_double);
    fn cairo_rotate(cr: *mut Cairo, angle: c_double);
    fn cairo_transform(cr: *mut Cairo, matrix: *const Matrix);
    fn cairo_set_matrix(cr: *mut Cairo, matrix: *const Matrix);
    fn cairo_identity_matrix(cr: *mut Cairo);
    fn cairo_get_matrix(cr: *mut Cairo, matrix: *mut Matrix);
    fn cairo_user_to_device(cr: *mut Cairo, x: *mut c_double, y: *mut c_double);
    fn cairo_device_to_user(cr: *mut Cairo, x: *mut c_double, y: *mut c_double);
    fn cairo_device_to_user_distance(cr: *mut Cairo, dx: *mut c_double, dy: *mut c_double);
    fn cairo_new_path(cr: *mut Cairo);
    fn cairo_new_sub_path(cr: *mut Cairo);
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
    fn cairo_rectangle(cr: *mut Cairo, x: c_double, y: c_double, w: c_double, h: c_double);
    fn cairo_close_path(cr: *mut Cairo);
    fn cairo_path_extents(
        cr: *mut Cairo,
        x1: *mut c_double,
        y1: *mut c_double,
        x2: *mut c_double,
        y2: *mut c_double,
    );
    fn cairo_has_current_point(cr: *mut Cairo) -> c_int;
    fn cairo_get_current_point(cr: *mut Cairo, x: *mut c_double, y: *mut c_double);
    fn cairo_copy_path(cr: *mut Cairo) -> *mut RawPath;
    fn cairo_append_path(cr: *mut Cairo, path: *const RawPath);
    fn cairo_path_destroy(path: *mut RawPath);
    fn cairo_paint(cr: *mut Cairo);
    fn cairo_paint_with_alpha(cr: *mut Cairo, alpha: c_double);
    fn cairo_mask(cr: *mut Cairo, pattern: *mut RawPattern);
    fn cairo_mask_surface(cr: *mut Cairo, surface: *mut RawSurface, x: c_double, y: c_double);
    fn cairo_stroke(cr: *mut Cairo);
    fn cairo_stroke_preserve(cr: *mut Cairo);
    fn cairo_fill(cr: *mut Cairo);
    fn cairo_fill_preserve(cr: *mut Cairo);
    fn cairo_in_stroke(cr: *mut Cairo, x: c_double, y: c_double) -> c_int;
    fn cairo_in_fill(cr: *mut Cairo, x: c_double, y: c_double) -> c_int;
    fn cairo_reset_clip(cr: *mut Cairo);
    fn cairo_clip(cr: *mut Cairo);
    fn cairo_clip_preserve(cr: *mut Cairo);
    fn cairo_clip_extents(
        cr: *mut Cairo,
        x1: *mut c_double,
        y1: *mut c_double,
        x2: *mut c_double,
        y2: *mut c_double,
    );
    fn cairo_show_page(cr: *mut Cairo);
    fn cairo_set_font_size(cr: *mut Cairo, size: c_double);
    fn cairo_text_extents(cr: *mut Cairo, utf8: *const c_char, extents: *mut TextExtents);
    fn cairo_show_text(cr: *mut Cairo, utf8: *const c_char);

    fn cairo_status_to_string(status: c_int) -> *const c_char;

    fn cairo_surface_reference(surface: *mut RawSurface) -> *mut RawSurface;
    fn cairo_surface_destroy(surface: *mut RawSurface);
    fn cairo_surface_status(surface: *mut RawSurface) -> c_int;
    fn cairo_surface_flush(surface: *mut RawSurface);
    fn cairo_surface_mark_dirty(surface: *mut RawSurface);
    fn cairo_surface_get_device_offset(
        surface: *mut RawSurface,
        x_offset: *mut c_double,
        y_offset: *mut c_double,
    );
    fn cairo_surface_write_to_png(surface: *mut RawSurface, filename: *const c_char) -> c_int;
    fn cairo_surface_write_to_png_stream(
        surface: *mut RawSurface,
        write: WriteFunc,
        closure: *mut c_void,
    ) -> c_int;
    fn cairo_image_surface_create(format: c_int, width: c_int, height: c_int) -> *mut RawSurface;
    fn cairo_image_surface_create_for_data(
        data: *mut u8,
        format: c_int,
        width: c_int,
        height: c_int,
        stride: c_int,
    ) -> *mut RawSurface;
    fn cairo_image_surface_create_from_png(filename: *const c_char) -> *mut RawSurface;
    fn cairo_image_surface_get_data(surface: *mut RawSurface) -> *mut u8;
    fn cairo_image_surface_get_width(surface: *mut RawSurface) -> c_int;
    fn cairo_image_surface_get_height(surface: *mut RawSurface) -> c_int;
    fn cairo_image_surface_get_stride(surface: *mut RawSurface) -> c_int;
    fn cairo_recording_surface_create(content: c_int, extents: *const Rectangle)
    -> *mut RawSurface;
    fn cairo_pdf_surface_create(
        filename: *const c_char,
        width_in_points: c_double,
        height_in_points: c_double,
    ) -> *mut RawSurface;
    fn cairo_pdf_surface_set_metadata(
        surface: *mut RawSurface,
        metadata: c_int,
        utf8: *const c_char,
    );

    fn cairo_pattern_create_for_surface(surface: *mut RawSurface) -> *mut RawPattern;
    fn cairo_pattern_create_linear(
        x0: c_double,
        y0: c_double,
        x1: c_double,
        y1: c_double,
    ) -> *mut RawPattern;
    fn cairo_pattern_create_radial(
        cx0: c_double,
        cy0: c_double,
        radius0: c_double,
        cx1: c_double,
        cy1: c_double,
        radius1: c_double,
    ) -> *mut RawPattern;
    fn cairo_pattern_create_mesh() -> *mut RawPattern;
    fn cairo_pattern_reference(pattern: *mut RawPattern) -> *mut RawPattern;
    fn cairo_pattern_destroy(pattern: *mut RawPattern);
    fn cairo_pattern_status(pattern: *mut RawPattern) -> c_int;
    fn cairo_pattern_set_extend(pattern: *mut RawPattern, extend: c_int);
    fn cairo_pattern_set_filter(pattern: *mut RawPattern, filter: c_int);
    fn cairo_pattern_set_matrix(pattern: *mut RawPattern, matrix: *const Matrix);
    fn cairo_pattern_add_color_stop_rgba(
        pattern: *mut RawPattern,
        offset: c_double,
        r: c_double,
        g: c_double,
        b: c_double,
        a: c_double,
    );
    fn cairo_mesh_pattern_begin_patch(pattern: *mut RawPattern);
    fn cairo_mesh_pattern_end_patch(pattern: *mut RawPattern);
    fn cairo_mesh_pattern_move_to(pattern: *mut RawPattern, x: c_double, y: c_double);
    fn cairo_mesh_pattern_line_to(pattern: *mut RawPattern, x: c_double, y: c_double);
    fn cairo_mesh_pattern_set_corner_color_rgba(
        pattern: *mut RawPattern,
        corner: c_uint,
        r: c_double,
        g: c_double,
        b: c_double,
        a: c_double,
    );

    fn cairo_matrix_init_rotate(matrix: *mut Matrix, radians: c_double);
    fn cairo_matrix_translate(matrix: *mut Matrix, tx: c_double, ty: c_double);
    fn cairo_matrix_scale(matrix: *mut Matrix, sx: c_double, sy: c_double);
    fn cairo_matrix_rotate(matrix: *mut Matrix, radians: c_double);
    fn cairo_matrix_invert(matrix: *mut Matrix) -> c_int;
    fn cairo_matrix_multiply(result: *mut Matrix, a: *const Matrix, b: *const Matrix);
    fn cairo_matrix_transform_point(matrix: *const Matrix, x: *mut c_double, y: *mut c_double);

    fn cairo_font_options_create() -> *mut RawFontOptions;
    fn cairo_font_options_destroy(options: *mut RawFontOptions);
    fn cairo_font_options_merge(options: *mut RawFontOptions, other: *const RawFontOptions);
    fn cairo_font_options_set_antialias(options: *mut RawFontOptions, antialias: c_int);
    fn cairo_font_options_set_subpixel_order(options: *mut RawFontOptions, order: c_int);
    fn cairo_font_options_set_hint_metrics(options: *mut RawFontOptions, metrics: c_int);
    #[cfg(target_os = "macos")]
    fn cairo_font_options_set_hint_style(options: *mut RawFontOptions, style: c_int);
}

impl Error {
    pub fn description(self) -> &'static CStr {
        // SAFETY: cairo_status_to_string accepts any status value and returns a static string.
        let text = unsafe { cairo_status_to_string(self.0) };
        if text.is_null() {
            return c"";
        }
        // SAFETY: the string is NUL-terminated and static.
        unsafe { CStr::from_ptr(text) }
    }
}
