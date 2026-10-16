//! Southstar — cairo patterns: owned references to linear, radial, surface and mesh patterns and the calls that shape them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};
use core::ptr::NonNull;

use super::{RawPattern, SurfaceRef};
use crate::types::{Error, Extend, Filter, Matrix};

pub struct Pattern(NonNull<RawPattern>);

impl Pattern {
    /// # Safety
    ///
    /// `raw` is a live pattern reference that the returned Pattern takes over.
    pub(super) unsafe fn own(raw: *mut RawPattern) -> Pattern {
        Pattern(NonNull::new(raw).expect("cairo returns a pattern, never null"))
    }

    /// # Safety
    ///
    /// `raw` is a live pattern.
    pub(super) unsafe fn borrowed(raw: *mut RawPattern) -> Pattern {
        // SAFETY: the caller passes a live pattern; cairo_pattern_reference adds the reference the Pattern owns.
        let raw = unsafe { super::cairo_pattern_reference(raw) };
        // SAFETY: that new reference is handed to the Pattern.
        unsafe { Pattern::own(raw) }
    }

    pub fn linear(x0: f64, y0: f64, x1: f64, y1: f64) -> Pattern {
        // SAFETY: cairo_pattern_create_linear takes plain values and returns a new reference.
        let raw = unsafe { super::cairo_pattern_create_linear(x0, y0, x1, y1) };
        // SAFETY: that new reference is handed to the Pattern.
        unsafe { Pattern::own(raw) }
    }

    pub fn radial(cx0: f64, cy0: f64, r0: f64, cx1: f64, cy1: f64, r1: f64) -> Pattern {
        // SAFETY: cairo_pattern_create_radial takes plain values and returns a new reference.
        let raw = unsafe { super::cairo_pattern_create_radial(cx0, cy0, r0, cx1, cy1, r1) };
        // SAFETY: that new reference is handed to the Pattern.
        unsafe { Pattern::own(raw) }
    }

    pub fn for_surface(surface: SurfaceRef<'_>) -> Pattern {
        // SAFETY: the surface is live; the pattern takes its own reference to it and is returned as a new reference.
        let raw = unsafe { super::cairo_pattern_create_for_surface(surface.raw()) };
        // SAFETY: that new reference is handed to the Pattern.
        unsafe { Pattern::own(raw) }
    }

    pub fn mesh() -> Pattern {
        // SAFETY: cairo_pattern_create_mesh takes nothing and returns a new reference.
        let raw = unsafe { super::cairo_pattern_create_mesh() };
        // SAFETY: that new reference is handed to the Pattern.
        unsafe { Pattern::own(raw) }
    }

    pub(super) fn raw(&self) -> *mut RawPattern {
        self.0.as_ptr()
    }

    pub fn as_raw(&self) -> *mut c_void {
        self.raw().cast()
    }

    pub fn status(&self) -> Result<(), Error> {
        // SAFETY: the Pattern holds a live pattern.
        Error::check(unsafe { super::cairo_pattern_status(self.raw()) })
    }

    pub fn set_extend(&self, extend: Extend) {
        // SAFETY: the Pattern holds a live pattern.
        unsafe { super::cairo_pattern_set_extend(self.raw(), extend as c_int) };
    }

    pub fn set_filter(&self, filter: Filter) {
        // SAFETY: the Pattern holds a live pattern.
        unsafe { super::cairo_pattern_set_filter(self.raw(), filter as c_int) };
    }

    pub fn set_matrix(&self, matrix: &Matrix) {
        // SAFETY: the Pattern holds a live pattern, and the matrix is read before this returns.
        unsafe { super::cairo_pattern_set_matrix(self.raw(), matrix) };
    }

    pub fn add_color_stop_rgba(&self, offset: f64, r: f64, g: f64, b: f64, a: f64) {
        // SAFETY: the Pattern holds a live pattern; a non-gradient pattern only records an error.
        unsafe { super::cairo_pattern_add_color_stop_rgba(self.raw(), offset, r, g, b, a) };
    }

    pub fn begin_patch(&self) {
        // SAFETY: the Pattern holds a live pattern; a non-mesh pattern only records an error.
        unsafe { super::cairo_mesh_pattern_begin_patch(self.raw()) };
    }

    pub fn end_patch(&self) {
        // SAFETY: the Pattern holds a live pattern; a non-mesh pattern only records an error.
        unsafe { super::cairo_mesh_pattern_end_patch(self.raw()) };
    }

    pub fn mesh_move_to(&self, x: f64, y: f64) {
        // SAFETY: the Pattern holds a live pattern; a non-mesh pattern only records an error.
        unsafe { super::cairo_mesh_pattern_move_to(self.raw(), x, y) };
    }

    pub fn mesh_line_to(&self, x: f64, y: f64) {
        // SAFETY: the Pattern holds a live pattern; a non-mesh pattern only records an error.
        unsafe { super::cairo_mesh_pattern_line_to(self.raw(), x, y) };
    }

    pub fn set_corner_color_rgba(&self, corner: u32, r: f64, g: f64, b: f64, a: f64) {
        // SAFETY: the Pattern holds a live pattern; a non-mesh pattern or a bad corner only records an error.
        unsafe { super::cairo_mesh_pattern_set_corner_color_rgba(self.raw(), corner, r, g, b, a) };
    }
}

impl Clone for Pattern {
    fn clone(&self) -> Pattern {
        // SAFETY: the Pattern holds a live pattern.
        unsafe { Pattern::borrowed(self.raw()) }
    }
}

impl Drop for Pattern {
    fn drop(&mut self) {
        // SAFETY: the Pattern owns one reference to its live pattern and releases it once, here.
        unsafe { super::cairo_pattern_destroy(self.raw()) };
    }
}
