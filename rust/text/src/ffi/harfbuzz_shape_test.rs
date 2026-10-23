//! Southstar text — HarfBuzz shaping the way web-text-lib's shape.c drives it, as the reference the HarfRust shaper is tested against.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::cell::RefCell;

use crate::ffi::harfbuzz::RawHbFont;
use crate::font::Font;
use crate::rust_shaper::{
    ShapeDirection, ShapeRequest, ShapedGlyph, is_default_ignorable, script_to_iso15924,
};
use crate::types::{GLYPH_EMPTY, GLYPH_UNKNOWN_FLAG, get_unknown_glyph, show_flags};
use crate::unicode::{UnicodeType, unichar_type};

#[repr(C)]
struct RawHbBuffer {
    _private: [u8; 0],
}

#[repr(C)]
struct RawHbFace {
    _private: [u8; 0],
}

#[repr(C)]
struct RawHbFontFuncs {
    _private: [u8; 0],
}

#[repr(C)]
struct RawHbLanguage {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HbGlyphInfo {
    codepoint: u32,
    mask: u32,
    cluster: u32,
    var1: u32,
    var2: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HbGlyphPosition {
    x_advance: i32,
    y_advance: i32,
    x_offset: i32,
    y_offset: i32,
    var: u32,
}

#[repr(C)]
struct HbGlyphExtents {
    x_bearing: i32,
    y_bearing: i32,
    width: i32,
    height: i32,
}

type NominalGlyphFunc =
    unsafe extern "C" fn(*mut RawHbFont, *mut c_void, u32, *mut u32, *mut c_void) -> c_int;
type AdvanceFunc = unsafe extern "C" fn(*mut RawHbFont, *mut c_void, u32, *mut c_void) -> i32;
type ExtentsFunc = unsafe extern "C" fn(
    *mut RawHbFont,
    *mut c_void,
    u32,
    *mut HbGlyphExtents,
    *mut c_void,
) -> c_int;

const DIRECTION_LTR: c_int = 4;
const DIRECTION_RTL: c_int = 5;
const DIRECTION_TTB: c_int = 6;
const DIRECTION_BTT: c_int = 7;
const CLUSTER_LEVEL_MONOTONE_CHARACTERS: c_int = 1;
const GLYPH_FLAG_DEFINED: u32 = 0x7;

#[link(name = "harfbuzz")]
unsafe extern "C" {
    fn hb_version_string() -> *const c_char;
    fn hb_buffer_create() -> *mut RawHbBuffer;
    fn hb_buffer_destroy(buffer: *mut RawHbBuffer);
    fn hb_buffer_set_direction(buffer: *mut RawHbBuffer, direction: c_int);
    fn hb_buffer_set_script(buffer: *mut RawHbBuffer, script: u32);
    fn hb_buffer_set_language(buffer: *mut RawHbBuffer, language: *const RawHbLanguage);
    fn hb_buffer_set_cluster_level(buffer: *mut RawHbBuffer, level: c_int);
    fn hb_buffer_set_flags(buffer: *mut RawHbBuffer, flags: c_uint);
    fn hb_buffer_set_invisible_glyph(buffer: *mut RawHbBuffer, glyph: u32);
    fn hb_buffer_add_utf8(
        buffer: *mut RawHbBuffer,
        text: *const c_char,
        text_length: c_int,
        item_offset: c_uint,
        item_length: c_int,
    );
    fn hb_buffer_add(buffer: *mut RawHbBuffer, codepoint: u32, cluster: c_uint);
    fn hb_buffer_get_length(buffer: *mut RawHbBuffer) -> c_uint;
    fn hb_buffer_get_glyph_infos(buffer: *mut RawHbBuffer, length: *mut c_uint)
    -> *mut HbGlyphInfo;
    fn hb_buffer_get_glyph_positions(
        buffer: *mut RawHbBuffer,
        length: *mut c_uint,
    ) -> *mut HbGlyphPosition;
    fn hb_language_from_string(text: *const c_char, len: c_int) -> *const RawHbLanguage;
    fn hb_shape(
        font: *mut RawHbFont,
        buffer: *mut RawHbBuffer,
        features: *const crate::font::Feature,
        num_features: c_uint,
    );
    fn hb_font_get_face(font: *mut RawHbFont) -> *mut RawHbFace;
    fn hb_face_get_glyph_count(face: *mut RawHbFace) -> c_uint;
    fn hb_font_create_sub_font(parent: *mut RawHbFont) -> *mut RawHbFont;
    fn hb_font_destroy(font: *mut RawHbFont);
    fn hb_font_set_funcs(
        font: *mut RawHbFont,
        klass: *mut RawHbFontFuncs,
        font_data: *mut c_void,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn hb_font_funcs_create() -> *mut RawHbFontFuncs;
    fn hb_font_funcs_destroy(funcs: *mut RawHbFontFuncs);
    fn hb_font_funcs_set_nominal_glyph_func(
        funcs: *mut RawHbFontFuncs,
        func: NominalGlyphFunc,
        user_data: *mut c_void,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn hb_font_funcs_set_glyph_h_advance_func(
        funcs: *mut RawHbFontFuncs,
        func: AdvanceFunc,
        user_data: *mut c_void,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn hb_font_funcs_set_glyph_v_advance_func(
        funcs: *mut RawHbFontFuncs,
        func: AdvanceFunc,
        user_data: *mut c_void,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn hb_font_funcs_set_glyph_extents_func(
        funcs: *mut RawHbFontFuncs,
        func: ExtentsFunc,
        user_data: *mut c_void,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn hb_font_get_nominal_glyph(font: *mut RawHbFont, unicode: u32, glyph: *mut u32) -> c_int;
    fn hb_font_get_glyph_h_advance(font: *mut RawHbFont, glyph: u32) -> i32;
    fn hb_font_get_glyph_v_advance(font: *mut RawHbFont, glyph: u32) -> i32;
    fn hb_font_get_glyph_extents(
        font: *mut RawHbFont,
        glyph: u32,
        extents: *mut HbGlyphExtents,
    ) -> c_int;
    fn hb_font_get_var_coords_normalized(font: *mut RawHbFont, length: *mut c_uint)
    -> *const c_int;
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
}

#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_unicode_script_to_iso15924(script: c_int) -> u32;
}

pub(crate) fn version() -> String {
    // SAFETY: hb_version_string returns a static NUL-terminated string.
    let raw = unsafe { hb_version_string() };
    // SAFETY: raw points at that static string.
    unsafe { CStr::from_ptr(raw) }
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn glib_script_to_iso15924(script: i32) -> u32 {
    // SAFETY: a pure table lookup defined for every int.
    unsafe { g_unicode_script_to_iso15924(script) }
}

struct Context<'a> {
    font: &'a Font,
    parent: *mut RawHbFont,
    show_flags: u32,
    stand_in_base: u32,
    stand_ins: RefCell<Vec<u32>>,
}

impl Context<'_> {
    fn stand_in_for(&self, made_up: u32) -> u32 {
        let mut list = self.stand_ins.borrow_mut();
        if let Some(index) = list.iter().position(|&glyph| glyph == made_up) {
            return self.stand_in_base + index as u32;
        }
        let stand_in = self.stand_in_base + list.len() as u32;
        if stand_in >= 1 << 14 {
            return made_up;
        }
        list.push(made_up);
        stand_in
    }

    fn made_up_for(&self, glyph: u32) -> u32 {
        let list = self.stand_ins.borrow();
        if glyph >= self.stand_in_base && ((glyph - self.stand_in_base) as usize) < list.len() {
            return list[(glyph - self.stand_in_base) as usize];
        }
        glyph
    }

    fn parent_nominal(&self, unicode: u32, glyph: &mut u32) -> bool {
        // SAFETY: the parent font outlives the shaping call and glyph is a writable u32.
        unsafe { hb_font_get_nominal_glyph(self.parent, unicode, glyph) != 0 }
    }

    fn nominal_or_unknown(&self, unicode: u32, glyph: &mut u32) -> bool {
        let mut unicode = unicode;
        if self.show_flags != 0 {
            if self.show_flags & show_flags::SPACES != 0
                && unichar_type(unicode) == UnicodeType::SpaceSeparator
            {
                if unicode == 0x20 {
                    unicode = 0x2423;
                } else {
                    *glyph = get_unknown_glyph(unicode);
                    return true;
                }
            }
            if self.show_flags & show_flags::IGNORABLES != 0 && is_default_ignorable(unicode) {
                *glyph = if crate::font::get_ignorable(unicode).is_some() {
                    get_unknown_glyph(unicode)
                } else {
                    GLYPH_EMPTY
                };
                return true;
            }
            if self.show_flags & show_flags::LINE_BREAKS != 0 && unicode == 0x2028 {
                *glyph = get_unknown_glyph(unicode);
                return true;
            }
        }
        if self.parent_nominal(unicode, glyph) {
            return true;
        }
        if unichar_type(unicode) == UnicodeType::SpaceSeparator {
            if unicode == 0x20 {
                *glyph = get_unknown_glyph(0x20);
                return true;
            }
            return false;
        }
        *glyph = get_unknown_glyph(unicode);
        unicode > 0x10FFFF
    }
}

fn context_of<'a>(font_data: *mut c_void) -> &'a Context<'a> {
    // SAFETY: font_data is the Context that shape_like_web_text_lib keeps alive for the whole hb_shape call.
    unsafe { &*font_data.cast::<Context<'a>>() }
}

unsafe extern "C" fn nominal_glyph(
    _font: *mut RawHbFont,
    font_data: *mut c_void,
    unicode: u32,
    glyph: *mut u32,
    _user_data: *mut c_void,
) -> c_int {
    let context = context_of(font_data);
    // SAFETY: HarfBuzz passes a writable glyph slot.
    let mut value = unsafe { *glyph };
    let found = context.nominal_or_unknown(unicode, &mut value);
    if value > 0xFFFF {
        value = context.stand_in_for(value);
    }
    // SAFETY: as above.
    unsafe { *glyph = value };
    c_int::from(found)
}

unsafe extern "C" fn h_advance(
    _font: *mut RawHbFont,
    font_data: *mut c_void,
    glyph: u32,
    _user_data: *mut c_void,
) -> i32 {
    let context = context_of(font_data);
    let glyph = context.made_up_for(glyph);
    if glyph & GLYPH_UNKNOWN_FLAG != 0 {
        return context.font.glyph_extents(glyph).1.width;
    }
    // SAFETY: the parent font outlives the shaping call.
    unsafe { hb_font_get_glyph_h_advance(context.parent, glyph) }
}

unsafe extern "C" fn v_advance(
    _font: *mut RawHbFont,
    font_data: *mut c_void,
    glyph: u32,
    _user_data: *mut c_void,
) -> i32 {
    let context = context_of(font_data);
    let glyph = context.made_up_for(glyph);
    if glyph & GLYPH_UNKNOWN_FLAG != 0 {
        return context.font.glyph_extents(glyph).1.height;
    }
    // SAFETY: the parent font outlives the shaping call.
    unsafe { hb_font_get_glyph_v_advance(context.parent, glyph) }
}

unsafe extern "C" fn glyph_extents(
    _font: *mut RawHbFont,
    font_data: *mut c_void,
    glyph: u32,
    extents: *mut HbGlyphExtents,
    _user_data: *mut c_void,
) -> c_int {
    let context = context_of(font_data);
    let glyph = context.made_up_for(glyph);
    if glyph & GLYPH_UNKNOWN_FLAG != 0 {
        let ink = context.font.glyph_extents(glyph).0;
        let value = HbGlyphExtents {
            x_bearing: ink.x,
            y_bearing: ink.y,
            width: ink.width,
            height: ink.height,
        };
        // SAFETY: HarfBuzz passes a writable extents struct.
        unsafe { extents.write(value) };
        return 1;
    }
    // SAFETY: the parent font outlives the call and extents is writable.
    unsafe { hb_font_get_glyph_extents(context.parent, glyph, extents) }
}

struct FontFuncs(*mut RawHbFontFuncs);

impl FontFuncs {
    fn new() -> FontFuncs {
        // SAFETY: creates a fresh, owned funcs object.
        let funcs = unsafe { hb_font_funcs_create() };
        // SAFETY: funcs is ours and still mutable.
        unsafe {
            hb_font_funcs_set_nominal_glyph_func(funcs, nominal_glyph, core::ptr::null_mut(), None)
        };
        // SAFETY: as above.
        unsafe {
            hb_font_funcs_set_glyph_h_advance_func(funcs, h_advance, core::ptr::null_mut(), None)
        };
        // SAFETY: as above.
        unsafe {
            hb_font_funcs_set_glyph_v_advance_func(funcs, v_advance, core::ptr::null_mut(), None)
        };
        // SAFETY: as above.
        unsafe {
            hb_font_funcs_set_glyph_extents_func(funcs, glyph_extents, core::ptr::null_mut(), None)
        };
        FontFuncs(funcs)
    }
}

impl Drop for FontFuncs {
    fn drop(&mut self) {
        // SAFETY: releases the reference FontFuncs::new created.
        unsafe { hb_font_funcs_destroy(self.0) };
    }
}

struct Buffer(*mut RawHbBuffer);

impl Drop for Buffer {
    fn drop(&mut self) {
        // SAFETY: releases the buffer shape_like_web_text_lib created.
        unsafe { hb_buffer_destroy(self.0) };
    }
}

struct SubFont(*mut RawHbFont);

impl Drop for SubFont {
    fn drop(&mut self) {
        // SAFETY: releases the sub-font shape_like_web_text_lib created.
        unsafe { hb_font_destroy(self.0) };
    }
}

fn add_utf8(buffer: &Buffer, text: &str, end: usize, offset: usize, length: usize) {
    // SAFETY: text is valid for end bytes and offset + length <= end.
    unsafe {
        hb_buffer_add_utf8(
            buffer.0,
            text.as_ptr().cast(),
            end as c_int,
            offset as c_uint,
            length as c_int,
        )
    };
}

fn add(buffer: &Buffer, ch: u32, cluster: u32) {
    // SAFETY: the buffer is live.
    unsafe { hb_buffer_add(buffer.0, ch, cluster) };
}

pub(crate) fn shape_like_web_text_lib(font: &Font, request: &ShapeRequest<'_>) -> Vec<ShapedGlyph> {
    let parent = font.hb_font().as_raw();
    // SAFETY: parent is live for the font's lifetime.
    let face = unsafe { hb_font_get_face(parent) };
    let context = Context {
        font,
        parent,
        show_flags: request.show_flags,
        // SAFETY: face belongs to parent.
        stand_in_base: unsafe { hb_face_get_glyph_count(face) },
        stand_ins: RefCell::new(Vec::new()),
    };
    let funcs = FontFuncs::new();
    // SAFETY: parent is live; the sub-font is ours.
    let sub_font = SubFont(unsafe { hb_font_create_sub_font(parent) });
    let context_ptr: *const Context<'_> = &context;
    // SAFETY: sub_font is ours; context outlives every use of sub_font below.
    unsafe { hb_font_set_funcs(sub_font.0, funcs.0, context_ptr.cast_mut().cast(), None) };
    // SAFETY: creates a fresh, owned buffer.
    let buffer = Buffer(unsafe { hb_buffer_create() });
    let direction = match request.direction {
        ShapeDirection::LeftToRight => DIRECTION_LTR,
        ShapeDirection::RightToLeft => DIRECTION_RTL,
        ShapeDirection::TopToBottom => DIRECTION_TTB,
        ShapeDirection::BottomToTop => DIRECTION_BTT,
    };
    // SAFETY: the buffer is live.
    unsafe { hb_buffer_set_direction(buffer.0, direction) };
    // SAFETY: the buffer is live.
    unsafe { hb_buffer_set_script(buffer.0, script_to_iso15924(request.script)) };
    let language = match request.language {
        // SAFETY: HarfBuzz reads len bytes of the language tag.
        Some(tag) => unsafe { hb_language_from_string(tag.as_ptr().cast(), tag.len() as c_int) },
        None => core::ptr::null(),
    };
    // SAFETY: the buffer is live; languages are interned and static.
    unsafe { hb_buffer_set_language(buffer.0, language) };
    // SAFETY: the buffer is live.
    unsafe { hb_buffer_set_cluster_level(buffer.0, CLUSTER_LEVEL_MONOTONE_CHARACTERS) };
    // SAFETY: the buffer is live.
    unsafe { hb_buffer_set_flags(buffer.0, request.flags) };
    // SAFETY: the buffer is live.
    unsafe { hb_buffer_set_invisible_glyph(buffer.0, GLYPH_EMPTY) };
    let paragraph = request.paragraph;
    add_utf8(
        &buffer,
        paragraph,
        request.item_start,
        request.item_start,
        0,
    );
    match request.transformed {
        None => add_utf8(
            &buffer,
            paragraph,
            request.item_end,
            request.item_start,
            request.item_end - request.item_start,
        ),
        Some(chars) => chars
            .iter()
            .for_each(|&(ch, cluster)| add(&buffer, ch, cluster)),
    }
    add_utf8(&buffer, paragraph, paragraph.len(), request.item_end, 0);
    if let Some((ch, cluster)) = request.hyphen {
        add(&buffer, ch, cluster);
    }
    // SAFETY: sub_font and buffer are live; features holds num_features entries laid out as hb_feature_t.
    unsafe {
        hb_shape(
            sub_font.0,
            buffer.0,
            request.features.as_ptr(),
            request.features.len() as c_uint,
        )
    };
    // SAFETY: the buffer is live.
    let length = unsafe { hb_buffer_get_length(buffer.0) } as usize;
    // SAFETY: the buffer is live; the array holds length infos.
    let infos = unsafe { hb_buffer_get_glyph_infos(buffer.0, core::ptr::null_mut()) };
    // SAFETY: as above, for positions.
    let positions = unsafe { hb_buffer_get_glyph_positions(buffer.0, core::ptr::null_mut()) };
    let infos = if length == 0 {
        &[][..]
    } else {
        // SAFETY: infos points at length initialized entries owned by the live buffer.
        unsafe { core::slice::from_raw_parts(infos, length) }
    };
    let positions = if length == 0 {
        &[][..]
    } else {
        // SAFETY: as above, for positions.
        unsafe { core::slice::from_raw_parts(positions, length) }
    };
    infos
        .iter()
        .zip(positions)
        .map(|(info, position)| ShapedGlyph {
            glyph: context.made_up_for(info.codepoint),
            cluster: info.cluster,
            flags: info.mask & GLYPH_FLAG_DEFINED,
            x_advance: position.x_advance,
            y_advance: position.y_advance,
            x_offset: position.x_offset,
            y_offset: position.y_offset,
        })
        .collect()
}

pub(crate) fn nominal_glyph_of(font: &Font, ch: u32) -> Option<u32> {
    let mut glyph = 0;
    // SAFETY: the font is live and glyph is a writable u32.
    let found = unsafe { hb_font_get_nominal_glyph(font.hb_font().as_raw(), ch, &mut glyph) };
    (found != 0).then_some(glyph)
}

pub(crate) fn glyph_h_advance_of(font: &Font, glyph: u32) -> i32 {
    // SAFETY: the font is live.
    unsafe { hb_font_get_glyph_h_advance(font.hb_font().as_raw(), glyph) }
}

pub(crate) fn glyph_v_advance_of(font: &Font, glyph: u32) -> i32 {
    // SAFETY: the font is live.
    unsafe { hb_font_get_glyph_v_advance(font.hb_font().as_raw(), glyph) }
}

pub(crate) fn glyph_extents_of(font: &Font, glyph: u32) -> Option<(i32, i32, i32, i32)> {
    let mut extents = HbGlyphExtents {
        x_bearing: 0,
        y_bearing: 0,
        width: 0,
        height: 0,
    };
    // SAFETY: the font is live and extents is writable.
    let found = unsafe { hb_font_get_glyph_extents(font.hb_font().as_raw(), glyph, &mut extents) };
    (found != 0).then_some((
        extents.x_bearing,
        extents.y_bearing,
        extents.width,
        extents.height,
    ))
}

fn face_of(font: &Font) -> *mut RawHbFace {
    // SAFETY: the font is live.
    unsafe { hb_font_get_face(font.hb_font().as_raw()) }
}

pub(crate) fn font_has_color(font: &Font) -> bool {
    let face = face_of(font);
    // SAFETY: the face is live.
    let paint = unsafe { hb_ot_color_has_paint(face) } != 0;
    // SAFETY: the face is live.
    let layers = unsafe { hb_ot_color_has_layers(face) } != 0;
    // SAFETY: the face is live.
    let png = unsafe { hb_ot_color_has_png(face) } != 0;
    // SAFETY: the face is live.
    let svg = unsafe { hb_ot_color_has_svg(face) } != 0;
    paint || layers || png || svg
}

pub(crate) fn glyph_has_paint_or_layers(font: &Font, glyph: u32) -> bool {
    let face = face_of(font);
    // SAFETY: the face is live.
    let paint = unsafe { hb_ot_color_glyph_has_paint(face, glyph) } != 0;
    // SAFETY: the face is live; NULL count and array ask for the total only.
    let layers = unsafe {
        hb_ot_color_glyph_get_layers(face, glyph, 0, core::ptr::null_mut(), core::ptr::null_mut())
    };
    paint || layers > 0
}

pub(crate) fn normalized_coords_of(font: &Font) -> Vec<i32> {
    let mut length: c_uint = 0;
    // SAFETY: the font is live and length is a writable local.
    let coords = unsafe { hb_font_get_var_coords_normalized(font.hb_font().as_raw(), &mut length) };
    if coords.is_null() || length == 0 {
        return Vec::new();
    }
    // SAFETY: HarfBuzz returns length coordinates owned by the live font.
    unsafe { core::slice::from_raw_parts(coords, length as usize) }.to_vec()
}
