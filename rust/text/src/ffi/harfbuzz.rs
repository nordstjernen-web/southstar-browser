//! Southstar text — HarfBuzz faces, fonts and buffers: loading and sizing a face, reading its metrics, and shaping through a sub-font with our own glyph callbacks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr::NonNull;

#[repr(C)]
pub struct RawHbFace {
    _private: [u8; 0],
}

#[repr(C)]
pub struct RawHbFont {
    _private: [u8; 0],
}

#[repr(C)]
struct RawHbBlob {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Feature {
    pub tag: u32,
    pub value: u32,
    pub start: c_uint,
    pub end: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct HbVariation {
    tag: u32,
    value: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AxisInfo {
    pub axis_index: c_uint,
    pub tag: u32,
    pub name_id: c_uint,
    pub flags: c_uint,
    pub min_value: f32,
    pub default_value: f32,
    pub max_value: f32,
    pub reserved: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FontExtents {
    pub ascender: i32,
    pub descender: i32,
    pub line_gap: i32,
    reserved: [i32; 9],
}

const DIRECTION_LTR: c_int = 4;

pub(crate) const fn tag(bytes: &[u8; 4]) -> u32 {
    ((bytes[0] as u32) << 24)
        | ((bytes[1] as u32) << 16)
        | ((bytes[2] as u32) << 8)
        | bytes[3] as u32
}

pub(crate) const METRICS_UNDERLINE_SIZE: u32 = tag(b"unds");
pub(crate) const METRICS_UNDERLINE_OFFSET: u32 = tag(b"undo");
pub(crate) const METRICS_STRIKEOUT_SIZE: u32 = tag(b"strs");
pub(crate) const METRICS_STRIKEOUT_OFFSET: u32 = tag(b"stro");

#[link(name = "harfbuzz")]
unsafe extern "C" {
    fn hb_blob_create_from_file(file_name: *const c_char) -> *mut RawHbBlob;
    fn hb_blob_destroy(blob: *mut RawHbBlob);
    fn hb_face_create(blob: *mut RawHbBlob, index: c_uint) -> *mut RawHbFace;
    fn hb_face_destroy(face: *mut RawHbFace);
    fn hb_face_reference(face: *mut RawHbFace) -> *mut RawHbFace;
    fn hb_font_create(face: *mut RawHbFace) -> *mut RawHbFont;
    fn hb_font_destroy(font: *mut RawHbFont);
    fn hb_font_make_immutable(font: *mut RawHbFont);
    fn hb_font_set_scale(font: *mut RawHbFont, x_scale: c_int, y_scale: c_int);
    fn hb_font_set_ptem(font: *mut RawHbFont, ptem: f32);
    fn hb_font_set_synthetic_slant(font: *mut RawHbFont, slant: f32);
    fn hb_font_set_synthetic_bold(
        font: *mut RawHbFont,
        x_embolden: f32,
        y_embolden: f32,
        in_place: c_int,
    );
    fn hb_font_set_var_coords_design(font: *mut RawHbFont, coords: *const f32, length: c_uint);
    fn hb_font_get_extents_for_direction(
        font: *mut RawHbFont,
        direction: c_int,
        extents: *mut FontExtents,
    );
    fn hb_font_get_h_extents(font: *mut RawHbFont, extents: *mut FontExtents) -> c_int;
    fn hb_font_get_nominal_glyph(font: *mut RawHbFont, unicode: u32, glyph: *mut u32) -> c_int;
    fn hb_ot_metrics_get_position(font: *mut RawHbFont, tag: u32, position: *mut i32) -> c_int;
    fn hb_ot_var_get_axis_infos(
        face: *mut RawHbFace,
        start_offset: c_uint,
        axes_count: *mut c_uint,
        axes_array: *mut AxisInfo,
    ) -> c_uint;
    fn hb_ot_var_named_instance_get_design_coords(
        face: *mut RawHbFace,
        instance_index: c_uint,
        coords_length: *mut c_uint,
        coords: *mut f32,
    ) -> c_uint;
    fn hb_feature_from_string(s: *const c_char, len: c_int, feature: *mut Feature) -> c_int;
    fn hb_variation_from_string(s: *const c_char, len: c_int, variation: *mut HbVariation)
    -> c_int;
}

pub struct HbFace(NonNull<RawHbFace>);

// SAFETY: a HarfBuzz face is immutable once created and its reference count is atomic.
unsafe impl Send for HbFace {}
// SAFETY: as above; HarfBuzz faces are documented as safe to share between threads.
unsafe impl Sync for HbFace {}

impl Drop for HbFace {
    fn drop(&mut self) {
        // SAFETY: this HbFace holds one reference, released exactly once here.
        unsafe { hb_face_destroy(self.0.as_ptr()) };
    }
}

impl Clone for HbFace {
    fn clone(&self) -> HbFace {
        // SAFETY: the face is live; the new reference belongs to the clone.
        let raw = unsafe { hb_face_reference(self.0.as_ptr()) };
        HbFace(NonNull::new(raw).expect("hb_face_reference returns its argument"))
    }
}

impl HbFace {
    pub(crate) fn from_file(file_name: &CStr, index: i32) -> HbFace {
        // SAFETY: the file name is NUL-terminated; HarfBuzz returns the empty blob on failure, never NULL.
        let blob = unsafe { hb_blob_create_from_file(file_name.as_ptr()) };
        // SAFETY: blob is live; the face takes its own reference to it.
        let face = unsafe { hb_face_create(blob, index as c_uint) };
        // SAFETY: our reference to blob is released once, the face keeps its own.
        unsafe { hb_blob_destroy(blob) };
        HbFace(NonNull::new(face).expect("hb_face_create returns the empty face on failure"))
    }

    pub fn as_raw(&self) -> *mut RawHbFace {
        self.0.as_ptr()
    }

    pub(crate) fn axis_count(&self) -> u32 {
        // SAFETY: the face is live; NULL count and array ask only for the total.
        unsafe {
            hb_ot_var_get_axis_infos(
                self.as_raw(),
                0,
                core::ptr::null_mut(),
                core::ptr::null_mut(),
            )
        }
    }

    pub(crate) fn axis_infos(&self, n_axes: u32) -> (Vec<AxisInfo>, u32) {
        let mut axes = vec![AxisInfo::default(); n_axes as usize];
        let mut count: c_uint = n_axes;
        // SAFETY: axes has room for count entries and count is a writable local.
        unsafe { hb_ot_var_get_axis_infos(self.as_raw(), 0, &mut count, axes.as_mut_ptr()) };
        (axes, count)
    }

    pub(crate) fn named_instance_design_coords(&self, instance: u32, coords: &mut [f32]) -> u32 {
        let mut length: c_uint = coords.len() as c_uint;
        // SAFETY: coords has room for length floats and length is a writable local.
        unsafe {
            hb_ot_var_named_instance_get_design_coords(
                self.as_raw(),
                instance,
                &mut length,
                coords.as_mut_ptr(),
            )
        };
        length
    }
}

pub(crate) struct HbFontBuilder(NonNull<RawHbFont>);

impl HbFontBuilder {
    pub(crate) fn new(face: &HbFace) -> HbFontBuilder {
        // SAFETY: the face is live; the font takes its own reference and is ours.
        let raw = unsafe { hb_font_create(face.as_raw()) };
        HbFontBuilder(NonNull::new(raw).expect("hb_font_create returns the empty font on failure"))
    }

    fn raw(&self) -> *mut RawHbFont {
        self.0.as_ptr()
    }

    pub(crate) fn set_scale(&mut self, x_scale: i32, y_scale: i32) {
        // SAFETY: the font is uniquely owned by this builder.
        unsafe { hb_font_set_scale(self.raw(), x_scale, y_scale) };
    }

    pub(crate) fn set_ptem(&mut self, ptem: f32) {
        // SAFETY: the font is uniquely owned by this builder.
        unsafe { hb_font_set_ptem(self.raw(), ptem) };
    }

    pub(crate) fn set_synthetic_slant(&mut self, slant: f32) {
        // SAFETY: the font is uniquely owned by this builder.
        unsafe { hb_font_set_synthetic_slant(self.raw(), slant) };
    }

    pub(crate) fn set_synthetic_bold(&mut self, x: f32, y: f32, in_place: bool) {
        // SAFETY: the font is uniquely owned by this builder.
        unsafe { hb_font_set_synthetic_bold(self.raw(), x, y, c_int::from(in_place)) };
    }

    pub(crate) fn set_var_coords_design(&mut self, coords: &[f32], length: u32) {
        let length = length.min(coords.len() as u32);
        // SAFETY: the font is uniquely owned and coords holds at least length floats.
        unsafe { hb_font_set_var_coords_design(self.raw(), coords.as_ptr(), length) };
    }

    pub(crate) fn finish(self) -> HbFont {
        let raw = self.0;
        core::mem::forget(self);
        // SAFETY: the font is uniquely owned; once immutable it may be shared between threads.
        unsafe { hb_font_make_immutable(raw.as_ptr()) };
        HbFont(raw)
    }
}

impl Drop for HbFontBuilder {
    fn drop(&mut self) {
        // SAFETY: the builder owns one reference, released exactly once here.
        unsafe { hb_font_destroy(self.raw()) };
    }
}

pub struct HbFont(NonNull<RawHbFont>);

// SAFETY: the font was made immutable before it became an HbFont, which HarfBuzz allows sharing.
unsafe impl Send for HbFont {}
// SAFETY: as above.
unsafe impl Sync for HbFont {}

impl Drop for HbFont {
    fn drop(&mut self) {
        // SAFETY: this HbFont holds one reference, released exactly once here.
        unsafe { hb_font_destroy(self.0.as_ptr()) };
    }
}

impl HbFont {
    pub fn as_raw(&self) -> *mut RawHbFont {
        self.0.as_ptr()
    }

    pub(crate) fn extents_ltr(&self) -> FontExtents {
        let mut extents = FontExtents::default();
        // SAFETY: the font is live and extents is a writable local.
        unsafe { hb_font_get_extents_for_direction(self.as_raw(), DIRECTION_LTR, &mut extents) };
        extents
    }

    pub(crate) fn h_extents(&self) -> FontExtents {
        let mut extents = FontExtents::default();
        // SAFETY: the font is live and extents is a writable local.
        unsafe { hb_font_get_h_extents(self.as_raw(), &mut extents) };
        extents
    }

    pub(crate) fn ot_metrics_position(&self, metrics_tag: u32) -> Option<i32> {
        let mut position: i32 = 0;
        // SAFETY: the font is live and position is a writable local.
        let found =
            unsafe { hb_ot_metrics_get_position(self.as_raw(), metrics_tag, &mut position) };
        (found != 0).then_some(position)
    }

    pub fn nominal_glyph_into(&self, unicode: u32, glyph: &mut u32) -> bool {
        // SAFETY: the font is live and glyph is a writable u32.
        unsafe { hb_font_get_nominal_glyph(self.as_raw(), unicode, glyph) != 0 }
    }
}

pub(crate) fn feature_from_string(text: &[u8]) -> Option<Feature> {
    let mut feature = Feature::default();
    let len = c_int::try_from(text.len()).ok()?;
    // SAFETY: HarfBuzz reads at most len bytes of text and writes one hb_feature_t into the local.
    let parsed = unsafe { hb_feature_from_string(text.as_ptr().cast(), len, &mut feature) };
    (parsed != 0).then_some(feature)
}

pub(crate) fn variation_from_string(text: &[u8]) -> Option<(u32, f32)> {
    let mut variation = HbVariation::default();
    let len = c_int::try_from(text.len()).ok()?;
    // SAFETY: HarfBuzz reads at most len bytes of text and writes one hb_variation_t into the local.
    let parsed = unsafe { hb_variation_from_string(text.as_ptr().cast(), len, &mut variation) };
    (parsed != 0).then_some((variation.tag, variation.value))
}

#[repr(C)]
pub(crate) struct RawHbBuffer {
    _private: [u8; 0],
}

#[repr(C)]
pub(crate) struct RawHbFontFuncs {
    _private: [u8; 0],
}

#[repr(C)]
struct RawHbLanguage {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BufferGlyphInfo {
    pub codepoint: u32,
    pub mask: u32,
    pub cluster: u32,
    var1: u32,
    var2: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BufferGlyphPosition {
    pub x_advance: i32,
    pub y_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
    var: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GlyphExtents {
    pub x_bearing: i32,
    pub y_bearing: i32,
    pub width: i32,
    pub height: i32,
}

pub(crate) const DIRECTION_LEFT_TO_RIGHT: c_int = DIRECTION_LTR;
pub(crate) const DIRECTION_RIGHT_TO_LEFT: c_int = 5;
pub(crate) const DIRECTION_TOP_TO_BOTTOM: c_int = 6;

pub(crate) const fn direction_reverse(direction: c_int) -> c_int {
    direction ^ 1
}

pub(crate) const BUFFER_FLAG_BOT: u32 = 0x1;
pub(crate) const BUFFER_FLAG_EOT: u32 = 0x2;
pub(crate) const BUFFER_FLAG_PRESERVE_DEFAULT_IGNORABLES: u32 = 0x4;
pub(crate) const BUFFER_FLAG_PRODUCE_UNSAFE_TO_CONCAT: u32 = 0x40;
pub(crate) const GLYPH_FLAG_UNSAFE_TO_CONCAT: u32 = 0x2;
const GLYPH_FLAG_DEFINED: u32 = 0x7;
const BUFFER_CLUSTER_LEVEL_MONOTONE_CHARACTERS: c_int = 1;
pub(crate) const OT_LAYOUT_GLYPH_CLASS_MARK: c_int = 3;
const OT_TAG_GSUB: u32 = tag(b"GSUB");
const OT_MAX_TAGS_PER_SCRIPT: usize = 3;
const OT_MAX_TAGS_PER_LANGUAGE: usize = 3;
pub(crate) const METRICS_SUPERSCRIPT_EM_Y_SIZE: u32 = tag(b"spys");
pub(crate) const METRICS_SUBSCRIPT_EM_Y_SIZE: u32 = tag(b"sbys");
pub(crate) const METRICS_CAP_HEIGHT: u32 = tag(b"cpht");
pub(crate) const METRICS_X_HEIGHT: u32 = tag(b"xhgt");

impl BufferGlyphInfo {
    pub(crate) fn glyph_flags(&self) -> u32 {
        self.mask & GLYPH_FLAG_DEFINED
    }
}

type NominalGlyphFunc = unsafe extern "C" fn(
    font: *mut RawHbFont,
    font_data: *mut c_void,
    unicode: u32,
    glyph: *mut u32,
    user_data: *mut c_void,
) -> c_int;
type AdvanceFunc = unsafe extern "C" fn(
    font: *mut RawHbFont,
    font_data: *mut c_void,
    glyph: u32,
    user_data: *mut c_void,
) -> i32;
type GlyphExtentsFunc = unsafe extern "C" fn(
    font: *mut RawHbFont,
    font_data: *mut c_void,
    glyph: u32,
    extents: *mut GlyphExtents,
    user_data: *mut c_void,
) -> c_int;
type HbDestroyFunc = unsafe extern "C" fn(data: *mut c_void);

#[link(name = "harfbuzz")]
unsafe extern "C" {
    fn hb_buffer_create() -> *mut RawHbBuffer;
    fn hb_buffer_destroy(buffer: *mut RawHbBuffer);
    fn hb_buffer_reset(buffer: *mut RawHbBuffer);
    fn hb_buffer_set_direction(buffer: *mut RawHbBuffer, direction: c_int);
    fn hb_buffer_set_script(buffer: *mut RawHbBuffer, script: u32);
    fn hb_buffer_set_language(buffer: *mut RawHbBuffer, language: *const RawHbLanguage);
    fn hb_buffer_set_cluster_level(buffer: *mut RawHbBuffer, cluster_level: c_int);
    fn hb_buffer_set_flags(buffer: *mut RawHbBuffer, flags: u32);
    fn hb_buffer_set_invisible_glyph(buffer: *mut RawHbBuffer, invisible: u32);
    fn hb_buffer_add_utf8(
        buffer: *mut RawHbBuffer,
        text: *const c_char,
        text_length: c_int,
        item_offset: c_uint,
        item_length: c_int,
    );
    fn hb_buffer_add(buffer: *mut RawHbBuffer, codepoint: u32, cluster: c_uint);
    fn hb_buffer_reverse(buffer: *mut RawHbBuffer);
    fn hb_buffer_get_length(buffer: *mut RawHbBuffer) -> c_uint;
    fn hb_buffer_get_glyph_infos(
        buffer: *mut RawHbBuffer,
        length: *mut c_uint,
    ) -> *mut BufferGlyphInfo;
    fn hb_buffer_get_glyph_positions(
        buffer: *mut RawHbBuffer,
        length: *mut c_uint,
    ) -> *mut BufferGlyphPosition;
    fn hb_language_from_string(text: *const c_char, len: c_int) -> *const RawHbLanguage;
    fn hb_shape(
        font: *mut RawHbFont,
        buffer: *mut RawHbBuffer,
        features: *const Feature,
        num_features: c_uint,
    );
    fn hb_font_create_sub_font(parent: *mut RawHbFont) -> *mut RawHbFont;
    fn hb_font_set_funcs(
        font: *mut RawHbFont,
        klass: *mut RawHbFontFuncs,
        font_data: *mut c_void,
        destroy: Option<HbDestroyFunc>,
    );
    fn hb_font_funcs_create() -> *mut RawHbFontFuncs;
    fn hb_font_funcs_make_immutable(ffuncs: *mut RawHbFontFuncs);
    fn hb_font_funcs_set_nominal_glyph_func(
        ffuncs: *mut RawHbFontFuncs,
        func: NominalGlyphFunc,
        user_data: *mut c_void,
        destroy: Option<HbDestroyFunc>,
    );
    fn hb_font_funcs_set_glyph_h_advance_func(
        ffuncs: *mut RawHbFontFuncs,
        func: AdvanceFunc,
        user_data: *mut c_void,
        destroy: Option<HbDestroyFunc>,
    );
    fn hb_font_funcs_set_glyph_v_advance_func(
        ffuncs: *mut RawHbFontFuncs,
        func: AdvanceFunc,
        user_data: *mut c_void,
        destroy: Option<HbDestroyFunc>,
    );
    fn hb_font_funcs_set_glyph_extents_func(
        ffuncs: *mut RawHbFontFuncs,
        func: GlyphExtentsFunc,
        user_data: *mut c_void,
        destroy: Option<HbDestroyFunc>,
    );
    fn hb_font_get_glyph_h_advance(font: *mut RawHbFont, glyph: u32) -> i32;
    fn hb_font_get_glyph_v_advance(font: *mut RawHbFont, glyph: u32) -> i32;
    fn hb_font_get_glyph_extents(
        font: *mut RawHbFont,
        glyph: u32,
        extents: *mut GlyphExtents,
    ) -> c_int;
    fn hb_font_get_face(font: *mut RawHbFont) -> *mut RawHbFace;
    fn hb_font_get_scale(font: *mut RawHbFont, x_scale: *mut c_int, y_scale: *mut c_int);
    fn hb_face_get_glyph_count(face: *mut RawHbFace) -> c_uint;
    fn hb_ot_color_has_paint(face: *mut RawHbFace) -> c_int;
    fn hb_ot_color_has_layers(face: *mut RawHbFace) -> c_int;
    fn hb_ot_color_has_png(face: *mut RawHbFace) -> c_int;
    fn hb_ot_color_has_svg(face: *mut RawHbFace) -> c_int;
    fn hb_ot_color_glyph_has_paint(face: *mut RawHbFace, glyph: u32) -> c_int;
    fn hb_ot_color_glyph_get_layers(
        face: *mut RawHbFace,
        glyph: u32,
        start_offset: c_uint,
        layer_count: *mut c_uint,
        layers: *mut c_void,
    ) -> c_uint;
    fn hb_ot_color_glyph_reference_png(font: *mut RawHbFont, glyph: u32) -> *mut RawHbBlob;
    fn hb_ot_color_glyph_reference_svg(face: *mut RawHbFace, glyph: u32) -> *mut RawHbBlob;
    fn hb_blob_get_length(blob: *mut RawHbBlob) -> c_uint;
    fn hb_ot_layout_get_glyph_class(face: *mut RawHbFace, glyph: u32) -> c_int;
    fn hb_ot_layout_get_ligature_carets(
        font: *mut RawHbFont,
        direction: c_int,
        glyph: u32,
        start_offset: c_uint,
        caret_count: *mut c_uint,
        caret_array: *mut i32,
    ) -> c_uint;
    fn hb_ot_tags_from_script_and_language(
        script: u32,
        language: *const RawHbLanguage,
        script_count: *mut c_uint,
        script_tags: *mut u32,
        language_count: *mut c_uint,
        language_tags: *mut u32,
    );
    fn hb_ot_layout_table_select_script(
        face: *mut RawHbFace,
        table_tag: u32,
        script_count: c_uint,
        script_tags: *const u32,
        script_index: *mut c_uint,
        chosen_script: *mut u32,
    ) -> c_int;
    fn hb_ot_layout_script_select_language(
        face: *mut RawHbFace,
        table_tag: u32,
        script_index: c_uint,
        language_count: c_uint,
        language_tags: *const u32,
        language_index: *mut c_uint,
    ) -> c_int;
    fn hb_ot_layout_language_find_feature(
        face: *mut RawHbFace,
        table_tag: u32,
        script_index: c_uint,
        language_index: c_uint,
        feature_tag: u32,
        feature_index: *mut c_uint,
    ) -> c_int;
}

fn language_from_str(language: Option<&str>) -> *const RawHbLanguage {
    let Some(language) = language else {
        // SAFETY: a NULL string asks HarfBuzz for HB_LANGUAGE_INVALID.
        return unsafe { hb_language_from_string(core::ptr::null(), -1) };
    };
    let len = c_int::try_from(language.len()).unwrap_or(c_int::MAX);
    // SAFETY: HarfBuzz reads at most len bytes of the string and returns an interned language.
    unsafe { hb_language_from_string(language.as_ptr().cast(), len) }
}

fn face_of(font: *mut RawHbFont) -> *mut RawHbFace {
    // SAFETY: callers pass a live font; the face is borrowed for the font's lifetime.
    unsafe { hb_font_get_face(font) }
}

fn blob_length_and_destroy(blob: *mut RawHbBlob) -> u32 {
    if blob.is_null() {
        return 0;
    }
    // SAFETY: blob is a live reference returned by HarfBuzz.
    let length = unsafe { hb_blob_get_length(blob) };
    // SAFETY: the reference returned to us is released exactly once.
    unsafe { hb_blob_destroy(blob) };
    length
}

fn font_has_color_raw(font: *mut RawHbFont) -> bool {
    let face = face_of(font);
    // SAFETY: face is live for the font's lifetime.
    if unsafe { hb_ot_color_has_paint(face) } != 0 {
        return true;
    }
    // SAFETY: as above.
    let layers = unsafe { hb_ot_color_has_layers(face) } != 0;
    // SAFETY: as above.
    let png = unsafe { hb_ot_color_has_png(face) } != 0;
    // SAFETY: as above.
    let svg = unsafe { hb_ot_color_has_svg(face) } != 0;
    layers || png || svg
}

fn glyph_has_color_raw(font: *mut RawHbFont, glyph: u32) -> bool {
    let face = face_of(font);
    // SAFETY: face is live for the font's lifetime.
    if unsafe { hb_ot_color_glyph_has_paint(face, glyph) } != 0 {
        return true;
    }
    // SAFETY: NULL count and array ask only for the number of layers.
    let layers = unsafe {
        hb_ot_color_glyph_get_layers(face, glyph, 0, core::ptr::null_mut(), core::ptr::null_mut())
    };
    if layers > 0 {
        return true;
    }
    // SAFETY: face is live.
    if unsafe { hb_ot_color_has_png(face) } != 0 {
        // SAFETY: font is live; the returned blob reference is ours.
        let blob = unsafe { hb_ot_color_glyph_reference_png(font, glyph) };
        if blob_length_and_destroy(blob) > 0 {
            return true;
        }
    }
    // SAFETY: face is live.
    if unsafe { hb_ot_color_has_svg(face) } != 0 {
        // SAFETY: face is live; the returned blob reference is ours.
        let blob = unsafe { hb_ot_color_glyph_reference_svg(face, glyph) };
        if blob_length_and_destroy(blob) > 0 {
            return true;
        }
    }
    false
}

impl HbFont {
    pub(crate) fn glyph_h_advance(&self, glyph: u32) -> i32 {
        // SAFETY: the font is live.
        unsafe { hb_font_get_glyph_h_advance(self.as_raw(), glyph) }
    }

    pub(crate) fn glyph_v_advance(&self, glyph: u32) -> i32 {
        // SAFETY: the font is live.
        unsafe { hb_font_get_glyph_v_advance(self.as_raw(), glyph) }
    }

    pub(crate) fn glyph_extents(&self, glyph: u32) -> Option<GlyphExtents> {
        let mut extents = GlyphExtents::default();
        // SAFETY: the font is live and extents is a writable local.
        let found = unsafe { hb_font_get_glyph_extents(self.as_raw(), glyph, &mut extents) };
        (found != 0).then_some(extents)
    }

    pub(crate) fn face_glyph_count(&self) -> u32 {
        // SAFETY: the face is live for the font's lifetime.
        unsafe { hb_face_get_glyph_count(face_of(self.as_raw())) }
    }

    pub(crate) fn y_scale(&self) -> i32 {
        let mut x_scale: c_int = 0;
        let mut y_scale: c_int = 0;
        // SAFETY: the font is live and both outputs are writable locals.
        unsafe { hb_font_get_scale(self.as_raw(), &mut x_scale, &mut y_scale) };
        y_scale
    }

    pub(crate) fn has_color(&self) -> bool {
        font_has_color_raw(self.as_raw())
    }

    pub(crate) fn glyph_has_color(&self, glyph: u32) -> bool {
        glyph_has_color_raw(self.as_raw(), glyph)
    }

    pub(crate) fn glyph_class(&self, glyph: u32) -> i32 {
        // SAFETY: the face is live for the font's lifetime.
        unsafe { hb_ot_layout_get_glyph_class(face_of(self.as_raw()), glyph) }
    }

    pub(crate) fn ligature_caret(
        &self,
        direction: c_int,
        glyph: u32,
        start_offset: u32,
    ) -> (u32, u32, i32) {
        let mut caret_count: c_uint = 1;
        let mut caret: i32 = 0;
        // SAFETY: the font is live; room for caret_count (1) carets is provided.
        let num_carets = unsafe {
            hb_ot_layout_get_ligature_carets(
                self.as_raw(),
                direction,
                glyph,
                start_offset,
                &mut caret_count,
                &mut caret,
            )
        };
        (num_carets, caret_count, caret)
    }

    pub(crate) fn gsub_features_supported(
        &self,
        script_tag: u32,
        language: Option<&str>,
        features: &[u32],
    ) -> bool {
        let face = face_of(self.as_raw());
        let hb_language = language_from_str(language);
        let mut script_count = OT_MAX_TAGS_PER_SCRIPT as c_uint;
        let mut script_tags = [0u32; OT_MAX_TAGS_PER_SCRIPT];
        let mut language_count = OT_MAX_TAGS_PER_LANGUAGE as c_uint;
        let mut language_tags = [0u32; OT_MAX_TAGS_PER_LANGUAGE];
        // SAFETY: the counts describe the sizes of the writable tag arrays.
        unsafe {
            hb_ot_tags_from_script_and_language(
                script_tag,
                hb_language,
                &mut script_count,
                script_tags.as_mut_ptr(),
                &mut language_count,
                language_tags.as_mut_ptr(),
            )
        };
        let mut script_index: c_uint = 0;
        let mut chosen_script: u32 = 0;
        // SAFETY: script_tags holds script_count tags; the outputs are writable locals.
        unsafe {
            hb_ot_layout_table_select_script(
                face,
                OT_TAG_GSUB,
                script_count,
                script_tags.as_ptr(),
                &mut script_index,
                &mut chosen_script,
            )
        };
        let mut language_index: c_uint = 0;
        // SAFETY: language_tags holds language_count tags; the output is a writable local.
        unsafe {
            hb_ot_layout_script_select_language(
                face,
                OT_TAG_GSUB,
                script_index,
                language_count,
                language_tags.as_ptr(),
                &mut language_index,
            )
        };
        features.iter().all(|&feature| {
            let mut index: c_uint = 0;
            // SAFETY: the face is live and index is a writable local.
            unsafe {
                hb_ot_layout_language_find_feature(
                    face,
                    OT_TAG_GSUB,
                    script_index,
                    language_index,
                    feature,
                    &mut index,
                ) != 0
            }
        })
    }
}

pub(crate) struct HbBuffer(NonNull<RawHbBuffer>);

impl Drop for HbBuffer {
    fn drop(&mut self) {
        // SAFETY: this HbBuffer owns its buffer and releases it exactly once.
        unsafe { hb_buffer_destroy(self.raw()) };
    }
}

impl HbBuffer {
    pub(crate) fn new() -> HbBuffer {
        // SAFETY: hb_buffer_create returns a new buffer (the empty buffer on failure, never NULL).
        let raw = unsafe { hb_buffer_create() };
        HbBuffer(NonNull::new(raw).expect("hb_buffer_create never returns NULL"))
    }

    fn raw(&self) -> *mut RawHbBuffer {
        self.0.as_ptr()
    }

    pub(crate) fn reset(&mut self) {
        // SAFETY: the buffer is uniquely owned.
        unsafe { hb_buffer_reset(self.raw()) };
    }

    pub(crate) fn set_direction(&mut self, direction: c_int) {
        // SAFETY: the buffer is uniquely owned.
        unsafe { hb_buffer_set_direction(self.raw(), direction) };
    }

    pub(crate) fn set_script(&mut self, script_tag: u32) {
        // SAFETY: the buffer is uniquely owned.
        unsafe { hb_buffer_set_script(self.raw(), script_tag) };
    }

    pub(crate) fn set_language(&mut self, language: Option<&str>) {
        let hb_language = language_from_str(language);
        // SAFETY: the buffer is uniquely owned and the language is interned by HarfBuzz.
        unsafe { hb_buffer_set_language(self.raw(), hb_language) };
    }

    pub(crate) fn set_cluster_level_monotone_characters(&mut self) {
        // SAFETY: the buffer is uniquely owned.
        unsafe {
            hb_buffer_set_cluster_level(self.raw(), BUFFER_CLUSTER_LEVEL_MONOTONE_CHARACTERS)
        };
    }

    pub(crate) fn set_flags(&mut self, flags: u32) {
        // SAFETY: the buffer is uniquely owned.
        unsafe { hb_buffer_set_flags(self.raw(), flags) };
    }

    pub(crate) fn set_invisible_glyph(&mut self, glyph: u32) {
        // SAFETY: the buffer is uniquely owned.
        unsafe { hb_buffer_set_invisible_glyph(self.raw(), glyph) };
    }

    pub(crate) fn add_utf8(
        &mut self,
        text: &[u8],
        text_length: i64,
        item_offset: i64,
        item_length: i64,
    ) {
        let text_length = if text_length == -1 {
            text.iter().position(|&b| b == 0).unwrap_or(text.len()) as i64
        } else {
            text_length
        };
        let item_length = if item_length == -1 {
            text_length - item_offset
        } else {
            item_length
        };
        if item_length < 0 || item_offset < 0 || text_length < 0 {
            return;
        }
        let available = text.len() as i64;
        let text_length = text_length.min(available);
        let item_offset = item_offset.min(available);
        let item_length = item_length.min(available - item_offset);
        // SAFETY: HarfBuzz reads the first text_length bytes and the item_length bytes at item_offset, all inside text.
        unsafe {
            hb_buffer_add_utf8(
                self.raw(),
                text.as_ptr().cast(),
                text_length as c_int,
                item_offset as c_uint,
                item_length as c_int,
            )
        };
    }

    pub(crate) fn add(&mut self, codepoint: u32, cluster: u32) {
        // SAFETY: the buffer is uniquely owned.
        unsafe { hb_buffer_add(self.raw(), codepoint, cluster) };
    }

    pub(crate) fn reverse(&mut self) {
        // SAFETY: the buffer is uniquely owned.
        unsafe { hb_buffer_reverse(self.raw()) };
    }

    pub(crate) fn glyph_infos(&self) -> &[BufferGlyphInfo] {
        let mut length: c_uint = 0;
        // SAFETY: the buffer is live; HarfBuzz returns its info array and its length.
        let infos = unsafe { hb_buffer_get_glyph_infos(self.raw(), &mut length) };
        if infos.is_null() || length == 0 {
            return &[];
        }
        // SAFETY: the array holds length infos and lives until the buffer is next modified.
        unsafe { core::slice::from_raw_parts(infos, length as usize) }
    }

    pub(crate) fn glyph_positions(&self) -> &[BufferGlyphPosition] {
        // SAFETY: the buffer is live.
        let length = unsafe { hb_buffer_get_length(self.raw()) };
        if length == 0 {
            return &[];
        }
        // SAFETY: the buffer is live; positions are created on demand for its length.
        let positions = unsafe { hb_buffer_get_glyph_positions(self.raw(), core::ptr::null_mut()) };
        if positions.is_null() {
            return &[];
        }
        // SAFETY: the array holds length positions and lives until the buffer is next modified.
        unsafe { core::slice::from_raw_parts(positions, length as usize) }
    }
}

pub(crate) trait ShapeFontHandler {
    fn nominal_glyph(&mut self, parent: &HbFont, unicode: u32) -> (bool, u32);
    fn h_advance(&mut self, parent: &HbFont, glyph: u32) -> i32;
    fn v_advance(&mut self, parent: &HbFont, glyph: u32) -> i32;
    fn glyph_extents(&mut self, parent: &HbFont, glyph: u32) -> Option<GlyphExtents>;
}

struct SubFontData<'a> {
    parent: &'a HbFont,
    handler: core::cell::RefCell<&'a mut dyn ShapeFontHandler>,
}

pub(crate) struct SubFont<'a> {
    raw: NonNull<RawHbFont>,
    _data: Box<SubFontData<'a>>,
}

struct FontFuncs(NonNull<RawHbFontFuncs>);

// SAFETY: the font funcs are made immutable before they are shared, which HarfBuzz allows.
unsafe impl Send for FontFuncs {}
// SAFETY: as above.
unsafe impl Sync for FontFuncs {}

fn sub_font_data<'a>(font_data: *mut c_void) -> &'a SubFontData<'a> {
    // SAFETY: font_data is the boxed SubFontData installed by SubFont::new, alive while the sub font is.
    unsafe { &*font_data.cast::<SubFontData<'a>>() }
}

unsafe extern "C" fn nominal_glyph_trampoline(
    _font: *mut RawHbFont,
    font_data: *mut c_void,
    unicode: u32,
    glyph: *mut u32,
    _user_data: *mut c_void,
) -> c_int {
    let data = sub_font_data(font_data);
    let (found, value) = data
        .handler
        .borrow_mut()
        .nominal_glyph(data.parent, unicode);
    // SAFETY: HarfBuzz passes a valid out pointer for the glyph.
    unsafe { *glyph = value };
    c_int::from(found)
}

unsafe extern "C" fn h_advance_trampoline(
    _font: *mut RawHbFont,
    font_data: *mut c_void,
    glyph: u32,
    _user_data: *mut c_void,
) -> i32 {
    let data = sub_font_data(font_data);
    data.handler.borrow_mut().h_advance(data.parent, glyph)
}

unsafe extern "C" fn v_advance_trampoline(
    _font: *mut RawHbFont,
    font_data: *mut c_void,
    glyph: u32,
    _user_data: *mut c_void,
) -> i32 {
    let data = sub_font_data(font_data);
    data.handler.borrow_mut().v_advance(data.parent, glyph)
}

unsafe extern "C" fn glyph_extents_trampoline(
    _font: *mut RawHbFont,
    font_data: *mut c_void,
    glyph: u32,
    extents: *mut GlyphExtents,
    _user_data: *mut c_void,
) -> c_int {
    let data = sub_font_data(font_data);
    let Some(value) = data.handler.borrow_mut().glyph_extents(data.parent, glyph) else {
        return 0;
    };
    // SAFETY: HarfBuzz passes a valid out pointer for the extents.
    unsafe { *extents = value };
    1
}

fn create_shape_font_funcs() -> FontFuncs {
    // SAFETY: hb_font_funcs_create returns a new funcs object (never NULL).
    let funcs = unsafe { hb_font_funcs_create() };
    // SAFETY: funcs is ours and not yet immutable.
    unsafe {
        hb_font_funcs_set_nominal_glyph_func(
            funcs,
            nominal_glyph_trampoline,
            core::ptr::null_mut(),
            None,
        )
    };
    // SAFETY: as above.
    unsafe {
        hb_font_funcs_set_glyph_h_advance_func(
            funcs,
            h_advance_trampoline,
            core::ptr::null_mut(),
            None,
        )
    };
    // SAFETY: as above.
    unsafe {
        hb_font_funcs_set_glyph_v_advance_func(
            funcs,
            v_advance_trampoline,
            core::ptr::null_mut(),
            None,
        )
    };
    // SAFETY: as above.
    unsafe {
        hb_font_funcs_set_glyph_extents_func(
            funcs,
            glyph_extents_trampoline,
            core::ptr::null_mut(),
            None,
        )
    };
    // SAFETY: funcs is complete; after this it is shared read-only.
    unsafe { hb_font_funcs_make_immutable(funcs) };
    FontFuncs(NonNull::new(funcs).expect("hb_font_funcs_create never returns NULL"))
}

fn shape_font_funcs() -> *mut RawHbFontFuncs {
    static FUNCS: std::sync::OnceLock<FontFuncs> = std::sync::OnceLock::new();
    FUNCS.get_or_init(create_shape_font_funcs).0.as_ptr()
}

impl Drop for SubFont<'_> {
    fn drop(&mut self) {
        // SAFETY: the sub font is ours and released exactly once, before its boxed data.
        unsafe { hb_font_destroy(self.raw.as_ptr()) };
    }
}

impl<'a> SubFont<'a> {
    pub(crate) fn new(parent: &'a HbFont, handler: &'a mut dyn ShapeFontHandler) -> SubFont<'a> {
        let funcs = shape_font_funcs();
        let data = Box::new(SubFontData {
            parent,
            handler: core::cell::RefCell::new(handler),
        });
        // SAFETY: the parent is live; the sub font takes its own reference to it.
        let raw = unsafe { hb_font_create_sub_font(parent.as_raw()) };
        let raw = NonNull::new(raw).expect("hb_font_create_sub_font never returns NULL");
        let data_ptr: *const SubFontData<'a> = &*data;
        // SAFETY: the data box outlives the sub font, which Drop destroys first.
        unsafe { hb_font_set_funcs(raw.as_ptr(), funcs, data_ptr.cast_mut().cast(), None) };
        SubFont { raw, _data: data }
    }

    pub(crate) fn nominal_glyph(&self, unicode: u32) -> Option<u32> {
        let mut glyph: u32 = 0;
        // SAFETY: the sub font is live and glyph is a writable local.
        let found = unsafe { hb_font_get_nominal_glyph(self.raw.as_ptr(), unicode, &mut glyph) };
        (found != 0).then_some(glyph)
    }

    pub(crate) fn shape(&self, buffer: &mut HbBuffer, features: &[Feature]) {
        // SAFETY: the sub font and buffer are live; features holds num_features entries.
        unsafe {
            hb_shape(
                self.raw.as_ptr(),
                buffer.raw(),
                features.as_ptr(),
                features.len() as c_uint,
            )
        };
    }
}
