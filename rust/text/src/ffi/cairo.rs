//! Southstar text — cairo for text: font options, matrices, FreeType-backed and user scaled fonts, and the drawing calls glyphs need.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_ulong, c_void};
use core::ptr::NonNull;

use super::fontconfig::{PatternBuilder, RawPattern};
use crate::ffi::fontconfig::Pattern;

#[repr(C)]
struct RawFontOptions {
    _private: [u8; 0],
}

#[repr(C)]
struct RawScaledFont {
    _private: [u8; 0],
}

#[repr(C)]
struct RawFontFace {
    _private: [u8; 0],
}

#[repr(C)]
struct RawCairo {
    _private: [u8; 0],
}

#[repr(C)]
struct RawSurface {
    _private: [u8; 0],
}

#[repr(C)]
struct UserDataKey {
    unused: c_int,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CairoMatrix {
    pub xx: f64,
    pub yx: f64,
    pub xy: f64,
    pub yy: f64,
    pub x0: f64,
    pub y0: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextExtents {
    pub x_bearing: f64,
    pub y_bearing: f64,
    pub width: f64,
    pub height: f64,
    pub x_advance: f64,
    pub y_advance: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FontExtents {
    pub ascent: f64,
    pub descent: f64,
    pub height: f64,
    pub max_x_advance: f64,
    pub max_y_advance: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CairoGlyph {
    pub index: c_ulong,
    pub x: c_double,
    pub y: c_double,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextCluster {
    pub num_bytes: c_int,
    pub num_glyphs: c_int,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum Antialias {
    Default = 0,
    None = 1,
    Gray = 2,
    Subpixel = 3,
    Fast = 4,
    Good = 5,
    Best = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum SubpixelOrder {
    Default = 0,
    Rgb = 1,
    Bgr = 2,
    Vrgb = 3,
    Vbgr = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum HintStyle {
    Default = 0,
    None = 1,
    Slight = 2,
    Medium = 3,
    Full = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum HintMetrics {
    Default = 0,
    Off = 1,
    On = 2,
}

const STATUS_SUCCESS: c_int = 0;
const STATUS_USER_FONT_ERROR: c_int = 27;
const FILL_RULE_EVEN_ODD: c_int = 1;

type RenderGlyphFunc = unsafe extern "C" fn(
    scaled_font: *mut RawScaledFont,
    glyph: c_ulong,
    cr: *mut RawCairo,
    extents: *mut TextExtents,
) -> c_int;

type UnicodeToGlyphFunc = unsafe extern "C" fn(
    scaled_font: *mut RawScaledFont,
    unicode: c_ulong,
    glyph_index: *mut c_ulong,
) -> c_int;

type DestroyFunc = unsafe extern "C" fn(data: *mut c_void);

#[link(name = "cairo")]
unsafe extern "C" {
    fn cairo_font_options_create() -> *mut RawFontOptions;
    fn cairo_font_options_copy(original: *const RawFontOptions) -> *mut RawFontOptions;
    fn cairo_font_options_destroy(options: *mut RawFontOptions);
    fn cairo_font_options_merge(options: *mut RawFontOptions, other: *const RawFontOptions);
    fn cairo_font_options_equal(a: *const RawFontOptions, b: *const RawFontOptions) -> c_int;
    fn cairo_font_options_hash(options: *const RawFontOptions) -> c_ulong;
    fn cairo_font_options_set_antialias(options: *mut RawFontOptions, antialias: c_int);
    fn cairo_font_options_get_antialias(options: *const RawFontOptions) -> c_int;
    fn cairo_font_options_set_subpixel_order(options: *mut RawFontOptions, order: c_int);
    fn cairo_font_options_get_subpixel_order(options: *const RawFontOptions) -> c_int;
    fn cairo_font_options_set_hint_style(options: *mut RawFontOptions, style: c_int);
    fn cairo_font_options_get_hint_style(options: *const RawFontOptions) -> c_int;
    fn cairo_font_options_set_hint_metrics(options: *mut RawFontOptions, metrics: c_int);
    fn cairo_font_options_get_hint_metrics(options: *const RawFontOptions) -> c_int;

    fn cairo_matrix_init(
        matrix: *mut CairoMatrix,
        xx: c_double,
        yx: c_double,
        xy: c_double,
        yy: c_double,
        x0: c_double,
        y0: c_double,
    );
    fn cairo_matrix_init_rotate(matrix: *mut CairoMatrix, radians: c_double);
    fn cairo_matrix_scale(matrix: *mut CairoMatrix, sx: c_double, sy: c_double);
    fn cairo_matrix_multiply(
        result: *mut CairoMatrix,
        a: *const CairoMatrix,
        b: *const CairoMatrix,
    );
    fn cairo_matrix_invert(matrix: *mut CairoMatrix) -> c_int;
    fn cairo_matrix_transform_distance(
        matrix: *const CairoMatrix,
        dx: *mut c_double,
        dy: *mut c_double,
    );

    fn cairo_font_face_destroy(face: *mut RawFontFace);
    fn cairo_font_face_status(face: *mut RawFontFace) -> c_int;
    fn cairo_font_face_set_user_data(
        face: *mut RawFontFace,
        key: *const UserDataKey,
        data: *mut c_void,
        destroy: DestroyFunc,
    ) -> c_int;
    fn cairo_font_face_get_user_data(
        face: *mut RawFontFace,
        key: *const UserDataKey,
    ) -> *mut c_void;
    fn cairo_user_font_face_create() -> *mut RawFontFace;
    fn cairo_user_font_face_set_render_glyph_func(face: *mut RawFontFace, func: RenderGlyphFunc);
    fn cairo_user_font_face_set_unicode_to_glyph_func(
        face: *mut RawFontFace,
        func: UnicodeToGlyphFunc,
    );

    fn cairo_scaled_font_create(
        face: *mut RawFontFace,
        font_matrix: *const CairoMatrix,
        ctm: *const CairoMatrix,
        options: *const RawFontOptions,
    ) -> *mut RawScaledFont;
    fn cairo_scaled_font_reference(font: *mut RawScaledFont) -> *mut RawScaledFont;
    fn cairo_scaled_font_destroy(font: *mut RawScaledFont);
    fn cairo_scaled_font_status(font: *mut RawScaledFont) -> c_int;
    fn cairo_scaled_font_get_font_face(font: *mut RawScaledFont) -> *mut RawFontFace;
    fn cairo_scaled_font_extents(font: *mut RawScaledFont, extents: *mut FontExtents);
    fn cairo_scaled_font_glyph_extents(
        font: *mut RawScaledFont,
        glyphs: *const CairoGlyph,
        num_glyphs: c_int,
        extents: *mut TextExtents,
    );
    fn cairo_scaled_font_text_extents(
        font: *mut RawScaledFont,
        utf8: *const c_char,
        extents: *mut TextExtents,
    );
    fn cairo_scaled_font_get_font_matrix(font: *mut RawScaledFont, matrix: *mut CairoMatrix);
    fn cairo_scaled_font_get_ctm(font: *mut RawScaledFont, matrix: *mut CairoMatrix);
    fn cairo_scaled_font_get_font_options(font: *mut RawScaledFont, options: *mut RawFontOptions);

    fn cairo_reference(cr: *mut RawCairo) -> *mut RawCairo;
    fn cairo_destroy(cr: *mut RawCairo);
    fn cairo_save(cr: *mut RawCairo);
    fn cairo_restore(cr: *mut RawCairo);
    fn cairo_get_matrix(cr: *mut RawCairo, matrix: *mut CairoMatrix);
    fn cairo_identity_matrix(cr: *mut RawCairo);
    fn cairo_set_scaled_font(cr: *mut RawCairo, font: *const RawScaledFont);
    fn cairo_glyph_path(cr: *mut RawCairo, glyphs: *const CairoGlyph, num_glyphs: c_int);
    fn cairo_text_path(cr: *mut RawCairo, utf8: *const c_char);
    fn cairo_set_fill_rule(cr: *mut RawCairo, rule: c_int);
    fn cairo_rectangle(cr: *mut RawCairo, x: c_double, y: c_double, w: c_double, h: c_double);
    fn cairo_new_sub_path(cr: *mut RawCairo);
    fn cairo_move_to(cr: *mut RawCairo, x: c_double, y: c_double);
    fn cairo_line_to(cr: *mut RawCairo, x: c_double, y: c_double);
    fn cairo_close_path(cr: *mut RawCairo);
    fn cairo_arc(
        cr: *mut RawCairo,
        xc: c_double,
        yc: c_double,
        radius: c_double,
        angle1: c_double,
        angle2: c_double,
    );
    fn cairo_fill(cr: *mut RawCairo);
    fn cairo_get_target(cr: *mut RawCairo) -> *mut RawSurface;
    fn cairo_get_font_options(cr: *mut RawCairo, options: *mut RawFontOptions);
    fn cairo_surface_get_font_options(surface: *mut RawSurface, options: *mut RawFontOptions);
}

#[link(name = "cairo")]
unsafe extern "C" {
    fn cairo_ft_font_face_create_for_pattern(pattern: *mut RawPattern) -> *mut RawFontFace;
    fn cairo_ft_font_options_substitute(options: *const RawFontOptions, pattern: *mut RawPattern);
}

pub struct FontOptions(NonNull<RawFontOptions>);

// SAFETY: font options are a plain value object; a shared FontOptions is never modified.
unsafe impl Send for FontOptions {}
// SAFETY: as above; the getters only read.
unsafe impl Sync for FontOptions {}

impl FontOptions {
    pub fn new() -> FontOptions {
        // SAFETY: cairo_font_options_create takes nothing and returns options we own (a static error object on failure).
        let raw = unsafe { cairo_font_options_create() };
        FontOptions(NonNull::new(raw).expect("cairo returns font options, never null"))
    }

    fn raw(&self) -> *mut RawFontOptions {
        self.0.as_ptr()
    }

    pub fn merge(&mut self, other: &FontOptions) {
        // SAFETY: both option sets are live, self is borrowed mutably.
        unsafe { cairo_font_options_merge(self.raw(), other.raw()) };
    }

    pub fn hash_value(&self) -> u64 {
        // SAFETY: the options are live and only read.
        let hash = unsafe { cairo_font_options_hash(self.raw()) };
        hash as FontOptionsHash
    }

    pub fn set_antialias(&mut self, antialias: Antialias) {
        // SAFETY: the options are live and borrowed mutably.
        unsafe { cairo_font_options_set_antialias(self.raw(), antialias as c_int) };
    }

    pub fn antialias(&self) -> i32 {
        // SAFETY: the options are live and only read.
        unsafe { cairo_font_options_get_antialias(self.raw()) }
    }

    pub fn set_subpixel_order(&mut self, order: SubpixelOrder) {
        // SAFETY: the options are live and borrowed mutably.
        unsafe { cairo_font_options_set_subpixel_order(self.raw(), order as c_int) };
    }

    pub fn subpixel_order(&self) -> i32 {
        // SAFETY: the options are live and only read.
        unsafe { cairo_font_options_get_subpixel_order(self.raw()) }
    }

    pub fn set_hint_style(&mut self, style: HintStyle) {
        // SAFETY: the options are live and borrowed mutably.
        unsafe { cairo_font_options_set_hint_style(self.raw(), style as c_int) };
    }

    pub fn hint_style(&self) -> i32 {
        // SAFETY: the options are live and only read.
        unsafe { cairo_font_options_get_hint_style(self.raw()) }
    }

    pub fn set_hint_metrics(&mut self, metrics: HintMetrics) {
        // SAFETY: the options are live and borrowed mutably.
        unsafe { cairo_font_options_set_hint_metrics(self.raw(), metrics as c_int) };
    }

    pub fn hint_metrics(&self) -> i32 {
        // SAFETY: the options are live and only read.
        unsafe { cairo_font_options_get_hint_metrics(self.raw()) }
    }

    pub fn hint_metrics_off(&self) -> bool {
        self.hint_metrics() == HintMetrics::Off as i32
    }

    pub(crate) fn substitute_into(&self, pattern: &mut PatternBuilder) {
        // SAFETY: the options are live and only read; the pattern is uniquely owned by the builder.
        unsafe { cairo_ft_font_options_substitute(self.raw(), pattern.raw()) };
    }
}

impl Default for FontOptions {
    fn default() -> FontOptions {
        FontOptions::new()
    }
}

impl Clone for FontOptions {
    fn clone(&self) -> FontOptions {
        // SAFETY: the options are live; the copy is new options we own.
        let raw = unsafe { cairo_font_options_copy(self.raw()) };
        FontOptions(NonNull::new(raw).expect("cairo returns font options, never null"))
    }
}

impl Drop for FontOptions {
    fn drop(&mut self) {
        // SAFETY: the FontOptions owns its options and frees them once, here.
        unsafe { cairo_font_options_destroy(self.raw()) };
    }
}

impl PartialEq for FontOptions {
    fn eq(&self, other: &FontOptions) -> bool {
        // SAFETY: both option sets are live and only read.
        unsafe { cairo_font_options_equal(self.raw(), other.raw()) != 0 }
    }
}

impl Eq for FontOptions {}

impl core::hash::Hash for FontOptions {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.hash_value().hash(state);
    }
}

impl core::fmt::Debug for FontOptions {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FontOptions")
            .field("antialias", &self.antialias())
            .field("subpixel_order", &self.subpixel_order())
            .field("hint_style", &self.hint_style())
            .field("hint_metrics", &self.hint_metrics())
            .finish()
    }
}

impl CairoMatrix {
    pub fn new(xx: f64, yx: f64, xy: f64, yy: f64, x0: f64, y0: f64) -> CairoMatrix {
        let mut matrix = CairoMatrix::default();
        // SAFETY: matrix is a writable local.
        unsafe { cairo_matrix_init(&mut matrix, xx, yx, xy, yy, x0, y0) };
        matrix
    }

    pub fn identity() -> CairoMatrix {
        CairoMatrix::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
    }

    pub fn rotation(radians: f64) -> CairoMatrix {
        let mut matrix = CairoMatrix::default();
        // SAFETY: matrix is a writable local.
        unsafe { cairo_matrix_init_rotate(&mut matrix, radians) };
        matrix
    }

    pub fn scale(&mut self, sx: f64, sy: f64) {
        // SAFETY: self is a valid, exclusively borrowed matrix.
        unsafe { cairo_matrix_scale(self, sx, sy) };
    }

    pub fn multiply(a: &CairoMatrix, b: &CairoMatrix) -> CairoMatrix {
        let mut result = CairoMatrix::default();
        // SAFETY: a and b are valid matrices and result is a writable local.
        unsafe { cairo_matrix_multiply(&mut result, a, b) };
        result
    }

    pub fn invert(&mut self) -> bool {
        // SAFETY: self is a valid, exclusively borrowed matrix.
        unsafe { cairo_matrix_invert(self) == STATUS_SUCCESS }
    }

    pub fn transform_distance(&self, dx: &mut f64, dy: &mut f64) {
        // SAFETY: self is a valid matrix and dx, dy are writable doubles.
        unsafe { cairo_matrix_transform_distance(self, dx, dy) };
    }
}

pub struct ScaledFont(NonNull<RawScaledFont>);

// SAFETY: cairo scaled fonts are reference counted atomically and lock their internal caches.
unsafe impl Send for ScaledFont {}
// SAFETY: as above; cairo documents scaled fonts as usable from several threads.
unsafe impl Sync for ScaledFont {}

impl Clone for ScaledFont {
    fn clone(&self) -> ScaledFont {
        // SAFETY: the scaled font is live; the new reference belongs to the clone.
        let raw = unsafe { cairo_scaled_font_reference(self.raw()) };
        ScaledFont(NonNull::new(raw).expect("cairo_scaled_font_reference returns its argument"))
    }
}

impl Drop for ScaledFont {
    fn drop(&mut self) {
        // SAFETY: this ScaledFont holds one reference, released exactly once here.
        unsafe { cairo_scaled_font_destroy(self.raw()) };
    }
}

impl core::fmt::Debug for ScaledFont {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "ScaledFont({:p})", self.0)
    }
}

impl ScaledFont {
    fn raw(&self) -> *mut RawScaledFont {
        self.0.as_ptr()
    }

    pub fn as_raw(&self) -> *mut c_void {
        self.raw().cast()
    }

    pub(crate) fn for_pattern(
        pattern: &Pattern,
        font_matrix: &CairoMatrix,
        ctm: &CairoMatrix,
        options: &FontOptions,
    ) -> Option<ScaledFont> {
        // SAFETY: the pattern is live; cairo takes its own reference and returns a face we own.
        let face = unsafe { cairo_ft_font_face_create_for_pattern(pattern.raw()) };
        if face.is_null() {
            return None;
        }
        // SAFETY: face, matrices and options are live; the scaled font is ours (an error object on failure).
        let raw = unsafe { cairo_scaled_font_create(face, font_matrix, ctm, options.raw()) };
        // SAFETY: our reference to face is released once; the scaled font keeps its own.
        unsafe { cairo_font_face_destroy(face) };
        NonNull::new(raw).map(ScaledFont)
    }

    pub(crate) fn user_font(
        handler: Box<dyn UserFontHandler>,
        font_matrix: &CairoMatrix,
        ctm: &CairoMatrix,
        options: &FontOptions,
    ) -> Option<ScaledFont> {
        // SAFETY: cairo_user_font_face_create takes nothing and returns a face we own.
        let face = unsafe { cairo_user_font_face_create() };
        // SAFETY: face is live.
        if unsafe { cairo_font_face_status(face) } != STATUS_SUCCESS {
            // SAFETY: our reference to face is released once.
            unsafe { cairo_font_face_destroy(face) };
            return None;
        }
        // SAFETY: face is a live user font face that is not yet immutable.
        unsafe { cairo_user_font_face_set_render_glyph_func(face, render_glyph_trampoline) };
        // SAFETY: as above.
        unsafe {
            cairo_user_font_face_set_unicode_to_glyph_func(face, unicode_to_glyph_trampoline)
        };
        let data: *mut Box<dyn UserFontHandler> = Box::into_raw(Box::new(handler));
        // SAFETY: face is live; cairo owns data from here and frees it through destroy_handler.
        let status = unsafe {
            cairo_font_face_set_user_data(face, &USER_FONT_KEY, data.cast(), destroy_handler)
        };
        if status != STATUS_SUCCESS {
            // SAFETY: cairo refused data, so it is still ours to free, once.
            unsafe { destroy_handler(data.cast()) };
            // SAFETY: our reference to face is released once.
            unsafe { cairo_font_face_destroy(face) };
            return None;
        }
        // SAFETY: face, matrices and options are live; the scaled font is ours.
        let raw = unsafe { cairo_scaled_font_create(face, font_matrix, ctm, options.raw()) };
        // SAFETY: our reference to face is released once; the scaled font keeps its own.
        unsafe { cairo_font_face_destroy(face) };
        let font = NonNull::new(raw).map(ScaledFont)?;
        font.is_ok().then_some(font)
    }

    pub fn is_ok(&self) -> bool {
        // SAFETY: the scaled font is live.
        unsafe { cairo_scaled_font_status(self.raw()) == STATUS_SUCCESS }
    }

    pub fn extents(&self) -> FontExtents {
        let mut extents = FontExtents::default();
        // SAFETY: the scaled font is live and extents is a writable local.
        unsafe { cairo_scaled_font_extents(self.raw(), &mut extents) };
        extents
    }

    pub fn glyph_extents(&self, glyph: u32) -> TextExtents {
        let cairo_glyph = CairoGlyph {
            index: glyph as c_ulong,
            x: 0.0,
            y: 0.0,
        };
        let mut extents = TextExtents::default();
        // SAFETY: the scaled font is live, one glyph is passed and extents is a writable local.
        unsafe { cairo_scaled_font_glyph_extents(self.raw(), &cairo_glyph, 1, &mut extents) };
        extents
    }

    pub fn text_extents(&self, text: &CStr) -> TextExtents {
        let mut extents = TextExtents::default();
        // SAFETY: the scaled font is live, text is NUL-terminated and extents is a writable local.
        unsafe { cairo_scaled_font_text_extents(self.raw(), text.as_ptr(), &mut extents) };
        extents
    }

    pub fn font_matrix(&self) -> CairoMatrix {
        let mut matrix = CairoMatrix::default();
        // SAFETY: the scaled font is live and matrix is a writable local.
        unsafe { cairo_scaled_font_get_font_matrix(self.raw(), &mut matrix) };
        matrix
    }

    pub fn ctm(&self) -> CairoMatrix {
        let mut matrix = CairoMatrix::default();
        // SAFETY: the scaled font is live and matrix is a writable local.
        unsafe { cairo_scaled_font_get_ctm(self.raw(), &mut matrix) };
        matrix
    }

    pub fn font_options(&self) -> FontOptions {
        let options = FontOptions::new();
        // SAFETY: the scaled font is live and options are freshly created and exclusively ours.
        unsafe { cairo_scaled_font_get_font_options(self.raw(), options.raw()) };
        options
    }
}

pub(crate) type UserGlyph = u64;
type FontOptionsHash = u64;

pub(crate) trait UserFontHandler: Send + Sync {
    fn render_glyph(&self, glyph: u64, cr: &CairoContext, extents: &mut TextExtents) -> bool;
    fn unicode_to_glyph(&self, unicode: u64) -> Option<u64>;
}

static USER_FONT_KEY: UserDataKey = UserDataKey { unused: 0 };

unsafe extern "C" fn destroy_handler(data: *mut c_void) {
    // SAFETY: data is the Box<Box<dyn UserFontHandler>> leaked in ScaledFont::user_font, freed once by cairo.
    drop(unsafe { Box::from_raw(data.cast::<Box<dyn UserFontHandler>>()) });
}

fn handler_of<'a>(scaled_font: *mut RawScaledFont) -> Option<&'a dyn UserFontHandler> {
    // SAFETY: cairo passes the live scaled font it is calling back for.
    let face = unsafe { cairo_scaled_font_get_font_face(scaled_font) };
    // SAFETY: face is live; the key is the static this module registered data under.
    let data = unsafe { cairo_font_face_get_user_data(face, &USER_FONT_KEY) };
    if data.is_null() {
        return None;
    }
    // SAFETY: data is the boxed handler set in ScaledFont::user_font, alive as long as the face.
    Some(unsafe { &**data.cast::<Box<dyn UserFontHandler>>() })
}

unsafe extern "C" fn render_glyph_trampoline(
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
    // SAFETY: cairo passes a valid extents struct for this call to fill in.
    let Some(extents) = (unsafe { extents.as_mut() }) else {
        return STATUS_USER_FONT_ERROR;
    };
    if handler.render_glyph(glyph as UserGlyph, &context, extents) {
        STATUS_SUCCESS
    } else {
        STATUS_USER_FONT_ERROR
    }
}

unsafe extern "C" fn unicode_to_glyph_trampoline(
    scaled_font: *mut RawScaledFont,
    unicode: c_ulong,
    glyph_index: *mut c_ulong,
) -> c_int {
    let Some(handler) = handler_of(scaled_font) else {
        return STATUS_USER_FONT_ERROR;
    };
    let Some(glyph) = handler.unicode_to_glyph(unicode as UserGlyph) else {
        return STATUS_USER_FONT_ERROR;
    };
    // SAFETY: cairo passes a valid out pointer for the glyph index.
    unsafe { *glyph_index = glyph as c_ulong };
    STATUS_SUCCESS
}

pub struct CairoContext(NonNull<RawCairo>);

impl Drop for CairoContext {
    fn drop(&mut self) {
        // SAFETY: this CairoContext holds one reference, released exactly once here.
        unsafe { cairo_destroy(self.raw()) };
    }
}

impl CairoContext {
    /// # Safety
    ///
    /// `cr` is a live `cairo_t`; the new CairoContext takes its own reference to it.
    pub unsafe fn from_raw(cr: *mut c_void) -> Option<CairoContext> {
        NonNull::new(cr.cast::<RawCairo>()).map(CairoContext::from_borrowed)
    }

    fn from_borrowed(cr: NonNull<RawCairo>) -> CairoContext {
        // SAFETY: callers hand over a live cairo_t; the reference taken here is released in Drop.
        unsafe { cairo_reference(cr.as_ptr()) };
        CairoContext(cr)
    }

    fn raw(&self) -> *mut RawCairo {
        self.0.as_ptr()
    }

    pub fn as_raw(&self) -> *mut c_void {
        self.raw().cast()
    }

    pub fn save(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_save(self.raw()) };
    }

    pub fn restore(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_restore(self.raw()) };
    }

    pub fn matrix(&self) -> CairoMatrix {
        let mut matrix = CairoMatrix::default();
        // SAFETY: the context is live and matrix is a writable local.
        unsafe { cairo_get_matrix(self.raw(), &mut matrix) };
        matrix
    }

    pub fn identity_matrix(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_identity_matrix(self.raw()) };
    }

    pub fn set_scaled_font(&self, font: &ScaledFont) {
        // SAFETY: the context and scaled font are live; cairo takes its own reference.
        unsafe { cairo_set_scaled_font(self.raw(), font.raw()) };
    }

    pub fn glyph_path(&self, glyph: u32) {
        let cairo_glyph = CairoGlyph {
            index: glyph as c_ulong,
            x: 0.0,
            y: 0.0,
        };
        // SAFETY: the context is live and one glyph is passed.
        unsafe { cairo_glyph_path(self.raw(), &cairo_glyph, 1) };
    }

    pub fn text_path(&self, text: &CStr) {
        // SAFETY: the context is live and text is NUL-terminated.
        unsafe { cairo_text_path(self.raw(), text.as_ptr()) };
    }

    pub fn set_fill_rule_even_odd(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_set_fill_rule(self.raw(), FILL_RULE_EVEN_ODD) };
    }

    pub fn rectangle(&self, x: f64, y: f64, width: f64, height: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_rectangle(self.raw(), x, y, width, height) };
    }

    pub fn new_sub_path(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_new_sub_path(self.raw()) };
    }

    pub fn move_to(&self, x: f64, y: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_move_to(self.raw(), x, y) };
    }

    pub fn line_to(&self, x: f64, y: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_line_to(self.raw(), x, y) };
    }

    pub fn close_path(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_close_path(self.raw()) };
    }

    pub fn arc(&self, xc: f64, yc: f64, radius: f64, angle1: f64, angle2: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_arc(self.raw(), xc, yc, radius, angle1, angle2) };
    }

    pub fn fill(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_fill(self.raw()) };
    }

    pub fn font_options(&self) -> FontOptions {
        let options = FontOptions::new();
        // SAFETY: the context is live and options are freshly created and exclusively ours.
        unsafe { cairo_get_font_options(self.raw(), options.raw()) };
        options
    }

    pub fn target_font_options(&self) -> FontOptions {
        // SAFETY: the context is live; the target stays owned by it.
        let target = unsafe { cairo_get_target(self.raw()) };
        let options = FontOptions::new();
        // SAFETY: the target is live and options are freshly created and exclusively ours.
        unsafe { cairo_surface_get_font_options(target, options.raw()) };
        options
    }
}

const PATTERN_TYPE_SOLID: c_int = 0;
const LINE_CAP_BUTT: c_int = 0;
const LINE_JOIN_MITER: c_int = 0;
const TEXT_CLUSTER_FLAG_BACKWARD: c_int = 1;

#[link(name = "cairo")]
unsafe extern "C" {
    fn cairo_show_glyphs(cr: *mut RawCairo, glyphs: *const CairoGlyph, num_glyphs: c_int);
    fn cairo_show_text_glyphs(
        cr: *mut RawCairo,
        utf8: *const c_char,
        utf8_len: c_int,
        glyphs: *const CairoGlyph,
        num_glyphs: c_int,
        clusters: *const TextCluster,
        num_clusters: c_int,
        cluster_flags: c_int,
    );
    fn cairo_show_text(cr: *mut RawCairo, utf8: *const c_char);
    fn cairo_set_source_rgba(
        cr: *mut RawCairo,
        red: c_double,
        green: c_double,
        blue: c_double,
        alpha: c_double,
    );
    fn cairo_get_source(cr: *mut RawCairo) -> *mut c_void;
    fn cairo_pattern_get_type(pattern: *mut c_void) -> c_int;
    fn cairo_pattern_get_rgba(
        pattern: *mut c_void,
        red: *mut c_double,
        green: *mut c_double,
        blue: *mut c_double,
        alpha: *mut c_double,
    ) -> c_int;
    fn cairo_set_line_width(cr: *mut RawCairo, width: c_double);
    fn cairo_set_line_join(cr: *mut RawCairo, join: c_int);
    fn cairo_set_line_cap(cr: *mut RawCairo, cap: c_int);
    fn cairo_set_miter_limit(cr: *mut RawCairo, limit: c_double);
    fn cairo_stroke(cr: *mut RawCairo);
    fn cairo_rel_line_to(cr: *mut RawCairo, dx: c_double, dy: c_double);
    fn cairo_new_path(cr: *mut RawCairo);
    fn cairo_translate(cr: *mut RawCairo, tx: c_double, ty: c_double);
    fn cairo_user_to_device_distance(cr: *mut RawCairo, dx: *mut c_double, dy: *mut c_double);
    fn cairo_has_current_point(cr: *mut RawCairo) -> c_int;
    fn cairo_get_current_point(cr: *mut RawCairo, x: *mut c_double, y: *mut c_double);
    fn cairo_surface_has_show_text_glyphs(surface: *mut RawSurface) -> c_int;
}

fn glyph_count(glyphs: &[CairoGlyph]) -> c_int {
    c_int::try_from(glyphs.len()).unwrap_or(c_int::MAX)
}

impl CairoContext {
    pub fn show_glyphs(&self, glyphs: &[CairoGlyph]) {
        // SAFETY: the context is live and the count never exceeds the slice length.
        unsafe { cairo_show_glyphs(self.raw(), glyphs.as_ptr(), glyph_count(glyphs)) };
    }

    pub fn glyphs_path(&self, glyphs: &[CairoGlyph]) {
        // SAFETY: the context is live and the count never exceeds the slice length.
        unsafe { cairo_glyph_path(self.raw(), glyphs.as_ptr(), glyph_count(glyphs)) };
    }

    pub fn show_text_glyphs(
        &self,
        text: &[u8],
        glyphs: &[CairoGlyph],
        clusters: &[TextCluster],
        backward: bool,
    ) {
        let text_len = c_int::try_from(text.len()).unwrap_or(c_int::MAX);
        let num_clusters = c_int::try_from(clusters.len()).unwrap_or(c_int::MAX);
        let flags = if backward {
            TEXT_CLUSTER_FLAG_BACKWARD
        } else {
            0
        };
        // SAFETY: the context is live and every pointer comes with the length of its slice.
        unsafe {
            cairo_show_text_glyphs(
                self.raw(),
                text.as_ptr().cast(),
                text_len,
                glyphs.as_ptr(),
                glyph_count(glyphs),
                clusters.as_ptr(),
                num_clusters,
                flags,
            )
        };
    }

    pub fn show_text(&self, text: &CStr) {
        // SAFETY: the context is live and text is NUL-terminated.
        unsafe { cairo_show_text(self.raw(), text.as_ptr()) };
    }

    pub fn set_source_rgba(&self, red: f64, green: f64, blue: f64, alpha: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_set_source_rgba(self.raw(), red, green, blue, alpha) };
    }

    pub fn solid_source_rgba(&self) -> Option<(f64, f64, f64, f64)> {
        // SAFETY: the context is live; the source pattern stays owned by it.
        let pattern = unsafe { cairo_get_source(self.raw()) };
        if pattern.is_null() {
            return None;
        }
        // SAFETY: pattern is the context's live source pattern.
        if unsafe { cairo_pattern_get_type(pattern) } != PATTERN_TYPE_SOLID {
            return None;
        }
        let mut rgba = (0.0, 0.0, 0.0, 0.0);
        // SAFETY: pattern is a live solid pattern and the four outputs are writable locals.
        unsafe {
            cairo_pattern_get_rgba(pattern, &mut rgba.0, &mut rgba.1, &mut rgba.2, &mut rgba.3)
        };
        Some(rgba)
    }

    pub fn set_line_width(&self, width: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_set_line_width(self.raw(), width) };
    }

    pub fn set_line_join_miter(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_set_line_join(self.raw(), LINE_JOIN_MITER) };
    }

    pub fn set_line_cap_butt(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_set_line_cap(self.raw(), LINE_CAP_BUTT) };
    }

    pub fn set_miter_limit(&self, limit: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_set_miter_limit(self.raw(), limit) };
    }

    pub fn stroke(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_stroke(self.raw()) };
    }

    pub fn rel_line_to(&self, dx: f64, dy: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_rel_line_to(self.raw(), dx, dy) };
    }

    pub fn new_path(&self) {
        // SAFETY: the context is live.
        unsafe { cairo_new_path(self.raw()) };
    }

    pub fn translate(&self, tx: f64, ty: f64) {
        // SAFETY: the context is live.
        unsafe { cairo_translate(self.raw(), tx, ty) };
    }

    pub fn user_to_device_distance(&self, dx: f64, dy: f64) -> (f64, f64) {
        let mut distance = (dx, dy);
        // SAFETY: the context is live and both outputs are writable locals.
        unsafe { cairo_user_to_device_distance(self.raw(), &mut distance.0, &mut distance.1) };
        distance
    }

    pub fn has_current_point(&self) -> bool {
        // SAFETY: the context is live.
        unsafe { cairo_has_current_point(self.raw()) != 0 }
    }

    pub fn current_point(&self) -> (f64, f64) {
        let mut point = (0.0, 0.0);
        // SAFETY: the context is live and both outputs are writable locals.
        unsafe { cairo_get_current_point(self.raw(), &mut point.0, &mut point.1) };
        point
    }

    pub fn target_has_show_text_glyphs(&self) -> bool {
        // SAFETY: the context is live; the target stays owned by it.
        let target = unsafe { cairo_get_target(self.raw()) };
        // SAFETY: target is the context's live surface.
        unsafe { cairo_surface_has_show_text_glyphs(target) != 0 }
    }
}
