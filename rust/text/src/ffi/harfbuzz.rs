//! Southstar text — HarfBuzz faces and fonts: loading a face from a font file, sizing and varying a font, and reading its metrics.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint};
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
