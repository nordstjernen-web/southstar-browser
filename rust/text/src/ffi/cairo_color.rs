//! Southstar text — the cairo calls color glyphs need: patterns, image surfaces, groups, clips and the user font's color callback.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{c_double, c_int, c_uint, c_ulong};
use core::ptr::NonNull;

use super::{
    CairoContext, CairoMatrix, RawCairo, RawFontFace, RawScaledFont, RawSurface, STATUS_SUCCESS,
    STATUS_USER_FONT_ERROR, TextExtents, UserGlyph, handler_of,
};

#[repr(C)]
struct RawCairoPattern {
    _private: [u8; 0],
}

const FORMAT_ARGB32: c_int = 0;
const STATUS_USER_FONT_NOT_IMPLEMENTED: c_int = 33;

pub(super) type RenderColorGlyphFunc = unsafe extern "C" fn(
    scaled_font: *mut RawScaledFont,
    glyph: c_ulong,
    cr: *mut RawCairo,
    extents: *mut TextExtents,
) -> c_int;

#[link(name = "cairo")]
unsafe extern "C" {
    pub(super) fn cairo_user_font_face_set_render_color_glyph_func(
        face: *mut RawFontFace,
        func: RenderColorGlyphFunc,
    );
    fn cairo_user_scaled_font_get_foreground_marker(
        font: *mut RawScaledFont,
    ) -> *mut RawCairoPattern;
    fn cairo_user_scaled_font_get_foreground_source(
        font: *mut RawScaledFont,
    ) -> *mut RawCairoPattern;
    fn cairo_transform(cr: *mut RawCairo, matrix: *const CairoMatrix);
    fn cairo_clip(cr: *mut RawCairo);
    fn cairo_clip_extents(
        cr: *mut RawCairo,
        x1: *mut c_double,
        y1: *mut c_double,
        x2: *mut c_double,
        y2: *mut c_double,
    );
    fn cairo_paint(cr: *mut RawCairo);
    fn cairo_paint_with_alpha(cr: *mut RawCairo, alpha: c_double);
    fn cairo_push_group(cr: *mut RawCairo);
    fn cairo_pop_group_to_source(cr: *mut RawCairo);
    fn cairo_set_operator(cr: *mut RawCairo, op: c_int);
    fn cairo_set_source(cr: *mut RawCairo, pattern: *mut RawCairoPattern);
    fn cairo_set_source_surface(
        cr: *mut RawCairo,
        surface: *mut RawSurface,
        x: c_double,
        y: c_double,
    );
    fn cairo_pattern_create_linear(
        x0: c_double,
        y0: c_double,
        x1: c_double,
        y1: c_double,
    ) -> *mut RawCairoPattern;
    fn cairo_pattern_create_radial(
        cx0: c_double,
        cy0: c_double,
        radius0: c_double,
        cx1: c_double,
        cy1: c_double,
        radius1: c_double,
    ) -> *mut RawCairoPattern;
    fn cairo_pattern_create_mesh() -> *mut RawCairoPattern;
    fn cairo_pattern_reference(pattern: *mut RawCairoPattern) -> *mut RawCairoPattern;
    fn cairo_pattern_destroy(pattern: *mut RawCairoPattern);
    fn cairo_pattern_status(pattern: *mut RawCairoPattern) -> c_int;
    fn cairo_pattern_add_color_stop_rgba(
        pattern: *mut RawCairoPattern,
        offset: c_double,
        red: c_double,
        green: c_double,
        blue: c_double,
        alpha: c_double,
    );
    fn cairo_pattern_set_extend(pattern: *mut RawCairoPattern, extend: c_int);
    fn cairo_mesh_pattern_begin_patch(pattern: *mut RawCairoPattern);
    fn cairo_mesh_pattern_end_patch(pattern: *mut RawCairoPattern);
    fn cairo_mesh_pattern_move_to(pattern: *mut RawCairoPattern, x: c_double, y: c_double);
    fn cairo_mesh_pattern_line_to(pattern: *mut RawCairoPattern, x: c_double, y: c_double);
    fn cairo_mesh_pattern_set_corner_color_rgba(
        pattern: *mut RawCairoPattern,
        corner: c_uint,
        red: c_double,
        green: c_double,
        blue: c_double,
        alpha: c_double,
    );
    fn cairo_image_surface_create(format: c_int, width: c_int, height: c_int) -> *mut RawSurface;
    fn cairo_image_surface_get_data(surface: *mut RawSurface) -> *mut u8;
    fn cairo_image_surface_get_stride(surface: *mut RawSurface) -> c_int;
    fn cairo_surface_flush(surface: *mut RawSurface);
    fn cairo_surface_mark_dirty(surface: *mut RawSurface);
    fn cairo_surface_status(surface: *mut RawSurface) -> c_int;
    fn cairo_surface_destroy(surface: *mut RawSurface);
}

pub(crate) struct CairoPattern(NonNull<RawCairoPattern>);

// SAFETY: cairo patterns are reference counted atomically; this one is only used by one painter.
unsafe impl Send for CairoPattern {}

impl Drop for CairoPattern {
    fn drop(&mut self) {
        // SAFETY: this CairoPattern holds one reference, released exactly once here.
        unsafe { cairo_pattern_destroy(self.0.as_ptr()) };
    }
}

impl CairoPattern {
    fn from_owned(raw: *mut RawCairoPattern) -> Option<CairoPattern> {
        let pattern = CairoPattern(NonNull::new(raw)?);
        // SAFETY: the pattern is live.
        (unsafe { cairo_pattern_status(pattern.0.as_ptr()) } == STATUS_SUCCESS).then_some(pattern)
    }

    fn from_borrowed(raw: *mut RawCairoPattern) -> Option<CairoPattern> {
        let raw = NonNull::new(raw)?;
        // SAFETY: cairo handed us a live pattern; the new reference belongs to the CairoPattern.
        unsafe { cairo_pattern_reference(raw.as_ptr()) };
        Some(CairoPattern(raw))
    }

    fn raw(&self) -> *mut RawCairoPattern {
        self.0.as_ptr()
    }

    pub(crate) fn linear(x0: f64, y0: f64, x1: f64, y1: f64) -> Option<CairoPattern> {
        // SAFETY: plain values in, a new pattern we own out.
        CairoPattern::from_owned(unsafe { cairo_pattern_create_linear(x0, y0, x1, y1) })
    }

    pub(crate) fn radial(
        cx0: f64,
        cy0: f64,
        r0: f64,
        cx1: f64,
        cy1: f64,
        r1: f64,
    ) -> Option<CairoPattern> {
        // SAFETY: plain values in, a new pattern we own out.
        CairoPattern::from_owned(unsafe { cairo_pattern_create_radial(cx0, cy0, r0, cx1, cy1, r1) })
    }

    pub(crate) fn mesh() -> Option<CairoPattern> {
        // SAFETY: cairo_pattern_create_mesh takes nothing and returns a new pattern we own.
        CairoPattern::from_owned(unsafe { cairo_pattern_create_mesh() })
    }

    pub(crate) fn add_color_stop(&self, offset: f64, red: f64, green: f64, blue: f64, alpha: f64) {
        // SAFETY: the pattern is live.
        unsafe { cairo_pattern_add_color_stop_rgba(self.raw(), offset, red, green, blue, alpha) };
    }

    pub(crate) fn set_extend(&self, extend: i32) {
        // SAFETY: the pattern is live.
        unsafe { cairo_pattern_set_extend(self.raw(), extend) };
    }

    pub(crate) fn solid_rgba(&self) -> Option<(f64, f64, f64, f64)> {
        let mut rgba = (0.0, 0.0, 0.0, 0.0);
        // SAFETY: the pattern is live and the four outputs are writable locals.
        let status = unsafe {
            super::cairo_pattern_get_rgba(
                self.raw().cast(),
                &mut rgba.0,
                &mut rgba.1,
                &mut rgba.2,
                &mut rgba.3,
            )
        };
        (status == STATUS_SUCCESS).then_some(rgba)
    }

    pub(crate) fn mesh_patch(
        &self,
        center: (f64, f64),
        from: (f64, f64),
        to: (f64, f64),
        from_color: (f64, f64, f64, f64),
        to_color: (f64, f64, f64, f64),
    ) {
        let raw = self.raw();
        // SAFETY: the pattern is a live mesh pattern with no patch open.
        unsafe { cairo_mesh_pattern_begin_patch(raw) };
        // SAFETY: a patch is open on the live mesh pattern.
        unsafe { cairo_mesh_pattern_move_to(raw, center.0, center.1) };
        for (x, y) in [from, to] {
            // SAFETY: a patch with a current point is open on the live mesh pattern.
            unsafe { cairo_mesh_pattern_line_to(raw, x, y) };
        }
        for (corner, (red, green, blue, alpha)) in [from_color, from_color, to_color, to_color]
            .into_iter()
            .enumerate()
        {
            // SAFETY: a patch is open on the live mesh pattern and the corner index is below four.
            unsafe {
                cairo_mesh_pattern_set_corner_color_rgba(
                    raw,
                    corner as c_uint,
                    red,
                    green,
                    blue,
                    alpha,
                )
            };
        }
        // SAFETY: a patch with three sides is open on the live mesh pattern.
        unsafe { cairo_mesh_pattern_end_patch(raw) };
    }
}

pub(crate) struct ImageSurface(NonNull<RawSurface>);

impl Drop for ImageSurface {
    fn drop(&mut self) {
        // SAFETY: this ImageSurface holds one reference, released exactly once here.
        unsafe { cairo_surface_destroy(self.0.as_ptr()) };
    }
}

impl ImageSurface {
    pub(crate) fn from_argb(width: u32, height: u32, pixels: &[u32]) -> Option<ImageSurface> {
        let (w, h) = (c_int::try_from(width).ok()?, c_int::try_from(height).ok()?);
        if pixels.len() != (width as usize) * (height as usize) {
            return None;
        }
        // SAFETY: plain values in, a new image surface we own out.
        let surface = ImageSurface(NonNull::new(unsafe {
            cairo_image_surface_create(FORMAT_ARGB32, w, h)
        })?);
        let raw = surface.0.as_ptr();
        // SAFETY: the surface is live.
        if unsafe { cairo_surface_status(raw) } != STATUS_SUCCESS {
            return None;
        }
        // SAFETY: the surface is live.
        unsafe { cairo_surface_flush(raw) };
        // SAFETY: the surface is a live image surface.
        let stride = usize::try_from(unsafe { cairo_image_surface_get_stride(raw) }).ok()?;
        // SAFETY: the surface is a live image surface.
        let data = unsafe { cairo_image_surface_get_data(raw) };
        if data.is_null() || stride < width as usize * 4 {
            return None;
        }
        // SAFETY: cairo allocated stride * height bytes for this ARGB32 surface and nothing else uses them yet.
        let bytes = unsafe { core::slice::from_raw_parts_mut(data, stride * height as usize) };
        for (row, line) in bytes
            .chunks_exact_mut(stride)
            .zip(pixels.chunks_exact(width as usize))
        {
            for (out, pixel) in row.chunks_exact_mut(4).zip(line) {
                out.copy_from_slice(&pixel.to_ne_bytes());
            }
        }
        // SAFETY: the surface is live and its pixels were just written directly.
        unsafe { cairo_surface_mark_dirty(raw) };
        Some(surface)
    }
}

impl CairoContext {
    pub(crate) fn transform(&self, matrix: &CairoMatrix) {
        // SAFETY: the context is live and matrix is a valid matrix.
        unsafe { cairo_transform(self.raw(), matrix) };
    }

    pub(crate) fn clip(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_clip(self.raw()) };
    }

    pub(crate) fn clip_extents(&self) -> (f64, f64, f64, f64) {
        let mut extents = (0.0, 0.0, 0.0, 0.0);
        // SAFETY: the context is live and the four outputs are writable locals.
        unsafe {
            cairo_clip_extents(
                self.raw(),
                &mut extents.0,
                &mut extents.1,
                &mut extents.2,
                &mut extents.3,
            )
        };
        extents
    }

    pub(crate) fn paint(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_paint(self.raw()) };
    }

    pub(crate) fn paint_with_alpha(&self, alpha: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_paint_with_alpha(self.raw(), alpha) };
    }

    pub(crate) fn push_group(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_push_group(self.raw()) };
    }

    pub(crate) fn pop_group_to_source(&self) {
        // SAFETY: the context is live and every pop follows a push_group of the same painter.
        unsafe { cairo_pop_group_to_source(self.raw()) };
    }

    pub(crate) fn set_operator(&self, op: i32) {
        // SAFETY: the context is live; cairo ignores operators it does not know.
        unsafe { cairo_set_operator(self.raw(), op) };
    }

    pub(crate) fn set_source(&self, pattern: &CairoPattern) {
        // SAFETY: the context and pattern are live; cairo takes its own reference.
        unsafe { cairo_set_source(self.raw(), pattern.raw()) };
    }

    pub(crate) fn set_source_surface(&self, surface: &ImageSurface, x: f64, y: f64) {
        // SAFETY: the context and surface are live; cairo takes its own reference.
        unsafe { cairo_set_source_surface(self.raw(), surface.0.as_ptr(), x, y) };
    }
}

pub(crate) struct ColorForeground(NonNull<RawScaledFont>);

impl ColorForeground {
    pub(crate) fn marker(&self) -> Option<CairoPattern> {
        // SAFETY: the scaled font is live for the color glyph callback this value was made for.
        CairoPattern::from_borrowed(unsafe {
            cairo_user_scaled_font_get_foreground_marker(self.0.as_ptr())
        })
    }

    pub(crate) fn rgba(&self) -> Option<(f64, f64, f64, f64)> {
        // SAFETY: the scaled font is live for the color glyph callback this value was made for.
        let source = unsafe { cairo_user_scaled_font_get_foreground_source(self.0.as_ptr()) };
        CairoPattern::from_borrowed(source)?.solid_rgba()
    }
}

pub(super) unsafe extern "C" fn render_color_glyph_trampoline(
    scaled_font: *mut RawScaledFont,
    glyph: c_ulong,
    cr: *mut RawCairo,
    extents: *mut TextExtents,
) -> c_int {
    let Some(handler) = handler_of(scaled_font) else {
        return STATUS_USER_FONT_ERROR;
    };
    let Some(context) = NonNull::new(cr).map(CairoContext::from_borrowed) else {
        return STATUS_USER_FONT_ERROR;
    };
    let Some(foreground) = NonNull::new(scaled_font).map(ColorForeground) else {
        return STATUS_USER_FONT_ERROR;
    };
    // SAFETY: cairo passes a valid extents struct for this call to fill in.
    let Some(extents) = (unsafe { extents.as_mut() }) else {
        return STATUS_USER_FONT_ERROR;
    };
    match handler.render_color_glyph(glyph as UserGlyph, &context, extents, &foreground) {
        Some(true) => STATUS_SUCCESS,
        Some(false) => STATUS_USER_FONT_ERROR,
        None => STATUS_USER_FONT_NOT_IMPLEMENTED,
    }
}
