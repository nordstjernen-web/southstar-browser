//! Southstar — cairo drawing contexts: an owning reference that destroys its cairo_t, and a copyable borrowed handle through which all drawing goes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_void};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

use super::{Cairo, Path, Pattern, RawPath, Surface, SurfaceRef};
use crate::types::{
    Antialias, Content, Error, Extend, FillRule, Filter, LineCap, LineJoin, Matrix, Operator,
    TextExtents,
};

pub struct Context(NonNull<Cairo>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContextRef<'a> {
    raw: NonNull<Cairo>,
    _context: PhantomData<&'a Context>,
}

fn context(raw: *mut Cairo) -> NonNull<Cairo> {
    NonNull::new(raw).expect("cairo returns a context, never null")
}

impl Context {
    pub fn new(target: SurfaceRef<'_>) -> Context {
        // SAFETY: target is a live surface; cairo_create takes its own reference to it.
        Context(context(unsafe { super::cairo_create(target.raw()) }))
    }

    /// # Safety
    ///
    /// `cr` is null or a live cairo_t whose reference the caller hands over to the returned
    /// Context, and it is only used on this thread from now on.
    pub unsafe fn from_raw_full(cr: *mut c_void) -> Option<Context> {
        NonNull::new(cr.cast()).map(Context)
    }

    pub fn as_ref(&self) -> ContextRef<'_> {
        ContextRef {
            raw: self.0,
            _context: PhantomData,
        }
    }

    pub fn as_raw(&self) -> *mut c_void {
        self.0.as_ptr().cast()
    }

    pub fn into_raw(self) -> *mut c_void {
        let raw = self.as_raw();
        core::mem::forget(self);
        raw
    }
}

impl Clone for Context {
    fn clone(&self) -> Context {
        self.as_ref().reference()
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: the Context owns one reference to its live cairo_t and releases it once, here.
        unsafe { super::cairo_destroy(self.0.as_ptr()) };
    }
}

impl<'a> ContextRef<'a> {
    /// # Safety
    ///
    /// `cr` is null or a cairo_t that stays alive for `'a` and is only used on this thread.
    pub unsafe fn from_raw(cr: *mut c_void) -> Option<ContextRef<'a>> {
        NonNull::new(cr.cast()).map(|raw| ContextRef {
            raw,
            _context: PhantomData,
        })
    }

    fn raw(&self) -> *mut Cairo {
        self.raw.as_ptr()
    }

    pub fn as_raw(&self) -> *mut c_void {
        self.raw().cast()
    }

    pub fn reference(&self) -> Context {
        // SAFETY: the handle is a live cairo_t; cairo_reference adds the reference the new Context owns.
        Context(context(unsafe { super::cairo_reference(self.raw()) }))
    }

    pub fn status(&self) -> Result<(), Error> {
        // SAFETY: the handle is a live cairo_t.
        Error::check(unsafe { super::cairo_status(self.raw()) })
    }

    pub fn save(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_save(self.raw()) };
    }

    pub fn restore(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_restore(self.raw()) };
    }

    pub fn target(&self) -> SurfaceRef<'a> {
        // SAFETY: the handle is a live cairo_t.
        let target = unsafe { super::cairo_get_target(self.raw()) };
        // SAFETY: the target is fixed when the context is created and the context keeps a reference to it until it is destroyed, so it lives for 'a.
        unsafe { SurfaceRef::from_raw(target.cast()) }.expect("cairo returns a target, never null")
    }

    pub fn group_target(&self) -> Surface {
        // SAFETY: the handle is a live cairo_t.
        let target = unsafe { super::cairo_get_group_target(self.raw()) };
        // SAFETY: the group target is live until the group is popped, and is referenced here before that can happen.
        unsafe { Surface::borrowed(target) }
    }

    pub fn push_group(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_push_group(self.raw()) };
    }

    pub fn push_group_with_content(&self, content: Content) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_push_group_with_content(self.raw(), content as c_int) };
    }

    pub fn pop_group(&self) -> Pattern {
        // SAFETY: the handle is a live cairo_t.
        let group = unsafe { super::cairo_pop_group(self.raw()) };
        // SAFETY: cairo_pop_group returns a new pattern reference, which the Pattern takes over.
        unsafe { Pattern::own(group) }
    }

    pub fn pop_group_to_source(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_pop_group_to_source(self.raw()) };
    }

    pub fn set_operator(&self, op: Operator) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_operator(self.raw(), op as c_int) };
    }

    pub fn operator(&self) -> Operator {
        // SAFETY: the handle is a live cairo_t.
        Operator::from_raw(unsafe { super::cairo_get_operator(self.raw()) })
    }

    pub fn set_source(&self, pattern: &Pattern) {
        // SAFETY: the handle is a live cairo_t and the pattern is live; cairo takes its own reference to it.
        unsafe { super::cairo_set_source(self.raw(), pattern.raw()) };
    }

    pub fn source(&self) -> Pattern {
        // SAFETY: the handle is a live cairo_t.
        let source = unsafe { super::cairo_get_source(self.raw()) };
        // SAFETY: the current source is live until the source changes, and is referenced here before that can happen.
        unsafe { Pattern::borrowed(source) }
    }

    pub fn set_source_filter(&self, filter: Filter) {
        // SAFETY: the handle is a live cairo_t.
        let source = unsafe { super::cairo_get_source(self.raw()) };
        // SAFETY: the current source is live until the source changes, which nothing does in between.
        unsafe { super::cairo_pattern_set_filter(source, filter as c_int) };
    }

    pub fn set_source_extend(&self, extend: Extend) {
        // SAFETY: the handle is a live cairo_t.
        let source = unsafe { super::cairo_get_source(self.raw()) };
        // SAFETY: the current source is live until the source changes, which nothing does in between.
        unsafe { super::cairo_pattern_set_extend(source, extend as c_int) };
    }

    pub fn set_source_rgb(&self, r: f64, g: f64, b: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_source_rgb(self.raw(), r, g, b) };
    }

    pub fn set_source_rgba(&self, r: f64, g: f64, b: f64, a: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_source_rgba(self.raw(), r, g, b, a) };
    }

    pub fn set_source_surface(&self, surface: SurfaceRef<'_>, x: f64, y: f64) {
        // SAFETY: the handle is a live cairo_t and the surface is live; cairo takes its own reference to it.
        unsafe { super::cairo_set_source_surface(self.raw(), surface.raw(), x, y) };
    }

    pub fn set_tolerance(&self, tolerance: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_tolerance(self.raw(), tolerance) };
    }

    pub fn set_antialias(&self, antialias: Antialias) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_antialias(self.raw(), antialias as c_int) };
    }

    pub fn set_fill_rule(&self, rule: FillRule) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_fill_rule(self.raw(), rule as c_int) };
    }

    pub fn fill_rule(&self) -> FillRule {
        // SAFETY: the handle is a live cairo_t.
        FillRule::from_raw(unsafe { super::cairo_get_fill_rule(self.raw()) })
    }

    pub fn set_line_width(&self, width: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_line_width(self.raw(), width) };
    }

    pub fn set_line_cap(&self, cap: LineCap) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_line_cap(self.raw(), cap as c_int) };
    }

    pub fn set_line_join(&self, join: LineJoin) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_line_join(self.raw(), join as c_int) };
    }

    pub fn set_miter_limit(&self, limit: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_miter_limit(self.raw(), limit) };
    }

    pub fn set_dash(&self, dashes: &[f64], offset: f64) {
        let n = c_int::try_from(dashes.len()).unwrap_or(c_int::MAX);
        let dashes = if dashes.is_empty() {
            ptr::null()
        } else {
            dashes.as_ptr()
        };
        // SAFETY: the handle is a live cairo_t, and cairo copies the n dashes, which the slice holds, before returning.
        unsafe { super::cairo_set_dash(self.raw(), dashes, n, offset) };
    }

    pub fn translate(&self, tx: f64, ty: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_translate(self.raw(), tx, ty) };
    }

    pub fn scale(&self, sx: f64, sy: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_scale(self.raw(), sx, sy) };
    }

    pub fn rotate(&self, angle: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_rotate(self.raw(), angle) };
    }

    pub fn transform(&self, matrix: &Matrix) {
        // SAFETY: the handle is a live cairo_t, and the matrix is read before this returns.
        unsafe { super::cairo_transform(self.raw(), matrix) };
    }

    pub fn set_matrix(&self, matrix: &Matrix) {
        // SAFETY: the handle is a live cairo_t, and the matrix is read before this returns.
        unsafe { super::cairo_set_matrix(self.raw(), matrix) };
    }

    pub fn identity_matrix(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_identity_matrix(self.raw()) };
    }

    pub fn matrix(&self) -> Matrix {
        let mut matrix = Matrix::default();
        // SAFETY: the handle is a live cairo_t, and the matrix is a writable local.
        unsafe { super::cairo_get_matrix(self.raw(), &mut matrix) };
        matrix
    }

    pub fn user_to_device(&self, x: f64, y: f64) -> (f64, f64) {
        let (mut x, mut y) = (x, y);
        // SAFETY: the handle is a live cairo_t, and x and y are writable locals.
        unsafe { super::cairo_user_to_device(self.raw(), &mut x, &mut y) };
        (x, y)
    }

    pub fn device_to_user(&self, x: f64, y: f64) -> (f64, f64) {
        let (mut x, mut y) = (x, y);
        // SAFETY: the handle is a live cairo_t, and x and y are writable locals.
        unsafe { super::cairo_device_to_user(self.raw(), &mut x, &mut y) };
        (x, y)
    }

    pub fn device_to_user_distance(&self, dx: f64, dy: f64) -> (f64, f64) {
        let (mut dx, mut dy) = (dx, dy);
        // SAFETY: the handle is a live cairo_t, and dx and dy are writable locals.
        unsafe { super::cairo_device_to_user_distance(self.raw(), &mut dx, &mut dy) };
        (dx, dy)
    }

    pub fn new_path(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_new_path(self.raw()) };
    }

    pub fn new_sub_path(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_new_sub_path(self.raw()) };
    }

    pub fn move_to(&self, x: f64, y: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_move_to(self.raw(), x, y) };
    }

    pub fn line_to(&self, x: f64, y: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_line_to(self.raw(), x, y) };
    }

    pub fn curve_to(&self, x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_curve_to(self.raw(), x1, y1, x2, y2, x3, y3) };
    }

    pub fn arc(&self, xc: f64, yc: f64, radius: f64, angle1: f64, angle2: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_arc(self.raw(), xc, yc, radius, angle1, angle2) };
    }

    pub fn arc_negative(&self, xc: f64, yc: f64, radius: f64, angle1: f64, angle2: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_arc_negative(self.raw(), xc, yc, radius, angle1, angle2) };
    }

    pub fn rectangle(&self, x: f64, y: f64, width: f64, height: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_rectangle(self.raw(), x, y, width, height) };
    }

    pub fn close_path(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_close_path(self.raw()) };
    }

    pub fn path_extents(&self) -> (f64, f64, f64, f64) {
        let (mut x1, mut y1, mut x2, mut y2) = (0.0, 0.0, 0.0, 0.0);
        // SAFETY: the handle is a live cairo_t, and the four outputs are writable locals.
        unsafe { super::cairo_path_extents(self.raw(), &mut x1, &mut y1, &mut x2, &mut y2) };
        (x1, y1, x2, y2)
    }

    pub fn has_current_point(&self) -> bool {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_has_current_point(self.raw()) != 0 }
    }

    pub fn current_point(&self) -> (f64, f64) {
        let (mut x, mut y) = (0.0, 0.0);
        // SAFETY: the handle is a live cairo_t, and x and y are writable locals.
        unsafe { super::cairo_get_current_point(self.raw(), &mut x, &mut y) };
        (x, y)
    }

    pub fn copy_path(&self) -> Path {
        // SAFETY: the handle is a live cairo_t.
        let path = unsafe { super::cairo_copy_path(self.raw()) };
        // SAFETY: cairo_copy_path returns a path the caller owns, which the Path takes over.
        unsafe { Path::own(path) }
    }

    pub fn append_path(&self, path: &Path) {
        let path: *const RawPath = path.raw();
        // SAFETY: the handle is a live cairo_t, and the path is a live cairo_path_t whose data holds num_data entries.
        unsafe { super::cairo_append_path(self.raw(), path) };
    }

    pub fn paint(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_paint(self.raw()) };
    }

    pub fn paint_with_alpha(&self, alpha: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_paint_with_alpha(self.raw(), alpha) };
    }

    pub fn mask(&self, pattern: &Pattern) {
        // SAFETY: the handle is a live cairo_t and the pattern is live.
        unsafe { super::cairo_mask(self.raw(), pattern.raw()) };
    }

    pub fn mask_surface(&self, surface: SurfaceRef<'_>, x: f64, y: f64) {
        // SAFETY: the handle is a live cairo_t and the surface is live.
        unsafe { super::cairo_mask_surface(self.raw(), surface.raw(), x, y) };
    }

    pub fn stroke(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_stroke(self.raw()) };
    }

    pub fn stroke_preserve(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_stroke_preserve(self.raw()) };
    }

    pub fn fill(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_fill(self.raw()) };
    }

    pub fn fill_preserve(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_fill_preserve(self.raw()) };
    }

    pub fn in_stroke(&self, x: f64, y: f64) -> bool {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_in_stroke(self.raw(), x, y) != 0 }
    }

    pub fn in_fill(&self, x: f64, y: f64) -> bool {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_in_fill(self.raw(), x, y) != 0 }
    }

    pub fn reset_clip(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_reset_clip(self.raw()) };
    }

    pub fn clip(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_clip(self.raw()) };
    }

    pub fn clip_preserve(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_clip_preserve(self.raw()) };
    }

    pub fn clip_extents(&self) -> (f64, f64, f64, f64) {
        let (mut x1, mut y1, mut x2, mut y2) = (0.0, 0.0, 0.0, 0.0);
        // SAFETY: the handle is a live cairo_t, and the four outputs are writable locals.
        unsafe { super::cairo_clip_extents(self.raw(), &mut x1, &mut y1, &mut x2, &mut y2) };
        (x1, y1, x2, y2)
    }

    pub fn show_page(&self) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_show_page(self.raw()) };
    }

    pub fn set_font_size(&self, size: f64) {
        // SAFETY: the handle is a live cairo_t.
        unsafe { super::cairo_set_font_size(self.raw(), size) };
    }

    pub fn text_extents(&self, text: &CStr) -> TextExtents {
        let mut extents = TextExtents::default();
        // SAFETY: the handle is a live cairo_t, the text is NUL-terminated, and extents is a writable local.
        unsafe { super::cairo_text_extents(self.raw(), text.as_ptr(), &mut extents) };
        extents
    }

    pub fn show_text(&self, text: &CStr) {
        // SAFETY: the handle is a live cairo_t, and the text is NUL-terminated.
        unsafe { super::cairo_show_text(self.raw(), text.as_ptr()) };
    }
}
