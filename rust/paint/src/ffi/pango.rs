//! Southstar — the ns-pango calls page text is laid out and painted with, behind owned layouts, attribute lists, attributes, font descriptions and line iterators.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use core::ptr;

use southstar_glib::GBoolean;

use super::cairo::Cr;

pub const SCALE: c_int = 1024;
pub const SCALE_F: f64 = 1024.0;

pub const WEIGHT_THIN: c_int = 100;
pub const WEIGHT_ULTRALIGHT: c_int = 200;
pub const WEIGHT_LIGHT: c_int = 300;
pub const WEIGHT_NORMAL: c_int = 400;
pub const WEIGHT_MEDIUM: c_int = 500;
pub const WEIGHT_SEMIBOLD: c_int = 600;
pub const WEIGHT_BOLD: c_int = 700;
pub const WEIGHT_ULTRABOLD: c_int = 800;
pub const WEIGHT_HEAVY: c_int = 900;

pub const STYLE_OBLIQUE: c_int = 1;
pub const STYLE_ITALIC: c_int = 2;
pub const VARIANT_SMALL_CAPS: c_int = 1;

pub const WRAP_WORD: c_int = 0;
pub const WRAP_CHAR: c_int = 1;
pub const WRAP_WORD_CHAR: c_int = 2;

pub const ALIGN_LEFT: c_int = 0;
pub const ALIGN_CENTER: c_int = 1;
pub const ALIGN_RIGHT: c_int = 2;

pub const ELLIPSIZE_END: c_int = 3;

pub const DIRECTION_LTR: c_int = 0;
pub const DIRECTION_RTL: c_int = 1;
pub const DIRECTION_NEUTRAL: c_int = 6;

pub const UNDERLINE_SINGLE: c_int = 1;
pub const UNDERLINE_DOUBLE: c_int = 2;
pub const UNDERLINE_ERROR: c_int = 4;
pub const OVERLINE_SINGLE: c_int = 1;

pub const TAB_LEFT: c_int = 0;
pub const ATTR_SHAPE: c_int = 14;

const CAIRO_ANTIALIAS_GRAY: c_int = 2;
const CAIRO_SUBPIXEL_ORDER_DEFAULT: c_int = 0;
const CAIRO_HINT_METRICS_OFF: c_int = 1;
#[cfg(target_os = "macos")]
const CAIRO_HINT_STYLE_NONE: c_int = 1;

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Rectangle {
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
}

#[repr(C)]
pub struct GSList {
    pub data: *mut c_void,
    pub next: *mut GSList,
}

#[repr(C)]
pub struct AttrClass {
    pub kind: c_int,
}

#[repr(C)]
pub struct RawAttribute {
    klass: *const AttrClass,
    start_index: c_uint,
    end_index: c_uint,
}

#[repr(C)]
struct Analysis {
    shape_engine: *mut c_void,
    lang_engine: *mut c_void,
    font: *mut c_void,
    level: u8,
    gravity: u8,
    flags: u8,
    script: u8,
    language: *mut c_void,
    extra_attrs: *mut GSList,
}

#[repr(C)]
struct Item {
    offset: c_int,
    length: c_int,
    num_chars: c_int,
    analysis: Analysis,
}

#[repr(C)]
struct GlyphItem {
    item: *mut Item,
    glyphs: *mut c_void,
    y_offset: c_int,
    start_x_offset: c_int,
    end_x_offset: c_int,
}

#[repr(C)]
pub struct RawLine {
    layout: *mut c_void,
    start_index: c_int,
    length: c_int,
    runs: *mut GSList,
    bits: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct LogAttr(c_uint);

impl LogAttr {
    pub fn is_word_start(self) -> bool {
        self.0 & (1 << 5) != 0
    }

    pub fn is_word_end(self) -> bool {
        self.0 & (1 << 6) != 0
    }
}

macro_rules! pango {
    ($(fn $name:ident($($arg:ident: $ty:ty),* $(,)?) $(-> $ret:ty)?;)*) => {
        unsafe extern "C" {
            $(
                #[cfg_attr(feature = "ns-pango", link_name = concat!("ns_", stringify!($name)))]
                fn $name($($arg: $ty),*) $(-> $ret)?;
            )*
        }
    };
}

pango! {
    fn pango_cairo_font_map_get_default() -> *mut c_void;
    fn pango_font_map_create_context(map: *mut c_void) -> *mut c_void;
    fn pango_font_map_get_serial(map: *mut c_void) -> c_uint;
    fn pango_font_map_list_families(map: *mut c_void, families: *mut *mut *mut c_void, n: *mut c_int);
    fn pango_font_family_get_name(family: *mut c_void) -> *const c_char;
    fn pango_cairo_context_get_font_options(context: *mut c_void) -> *const c_void;
    fn pango_cairo_context_set_font_options(context: *mut c_void, options: *const c_void);
    fn pango_cairo_context_set_resolution(context: *mut c_void, dpi: c_double);
    fn pango_context_set_round_glyph_positions(context: *mut c_void, round: GBoolean);
    fn pango_context_get_metrics(context: *mut c_void, desc: *const c_void, language: *mut c_void) -> *mut c_void;
    fn pango_context_get_base_dir(context: *mut c_void) -> c_int;
    fn pango_context_set_base_dir(context: *mut c_void, direction: c_int);
    fn pango_context_get_font_description(context: *mut c_void) -> *const c_void;
    fn pango_font_metrics_get_ascent(metrics: *mut c_void) -> c_int;
    fn pango_font_metrics_get_descent(metrics: *mut c_void) -> c_int;
    fn pango_font_metrics_get_height(metrics: *mut c_void) -> c_int;
    fn pango_font_metrics_get_approximate_char_width(metrics: *mut c_void) -> c_int;
    fn pango_font_metrics_unref(metrics: *mut c_void);
    fn pango_font_description_new() -> *mut c_void;
    fn pango_font_description_from_string(text: *const c_char) -> *mut c_void;
    fn pango_font_description_free(desc: *mut c_void);
    fn pango_font_description_set_family(desc: *mut c_void, family: *const c_char);
    fn pango_font_description_set_weight(desc: *mut c_void, weight: c_int);
    fn pango_font_description_set_style(desc: *mut c_void, style: c_int);
    fn pango_font_description_set_stretch(desc: *mut c_void, stretch: c_int);
    fn pango_font_description_set_absolute_size(desc: *mut c_void, size: c_double);
    fn pango_font_description_set_variations(desc: *mut c_void, variations: *const c_char);
    fn pango_layout_new(context: *mut c_void) -> *mut c_void;
    fn pango_layout_get_context(layout: *mut c_void) -> *mut c_void;
    fn pango_layout_set_font_description(layout: *mut c_void, desc: *const c_void);
    fn pango_layout_get_font_description(layout: *mut c_void) -> *const c_void;
    fn pango_layout_set_text(layout: *mut c_void, text: *const c_char, length: c_int);
    fn pango_layout_get_text(layout: *mut c_void) -> *const c_char;
    fn pango_layout_set_attributes(layout: *mut c_void, attrs: *mut c_void);
    fn pango_layout_set_width(layout: *mut c_void, width: c_int);
    fn pango_layout_get_width(layout: *mut c_void) -> c_int;
    fn pango_layout_set_height(layout: *mut c_void, height: c_int);
    fn pango_layout_set_wrap(layout: *mut c_void, wrap: c_int);
    fn pango_layout_set_indent(layout: *mut c_void, indent: c_int);
    fn pango_layout_set_ellipsize(layout: *mut c_void, ellipsize: c_int);
    fn pango_layout_set_alignment(layout: *mut c_void, alignment: c_int);
    fn pango_layout_get_alignment(layout: *mut c_void) -> c_int;
    fn pango_layout_set_justify(layout: *mut c_void, justify: GBoolean);
    fn pango_layout_set_auto_dir(layout: *mut c_void, auto_dir: GBoolean);
    fn pango_layout_set_tabs(layout: *mut c_void, tabs: *mut c_void);
    fn pango_layout_get_size(layout: *mut c_void, width: *mut c_int, height: *mut c_int);
    fn pango_layout_get_pixel_size(layout: *mut c_void, width: *mut c_int, height: *mut c_int);
    fn pango_layout_get_pixel_extents(layout: *mut c_void, ink: *mut Rectangle, logical: *mut Rectangle);
    fn pango_layout_get_line_count(layout: *mut c_void) -> c_int;
    fn pango_layout_get_line_readonly(layout: *mut c_void, line: c_int) -> *mut RawLine;
    fn pango_layout_index_to_pos(layout: *mut c_void, index: c_int, pos: *mut Rectangle);
    fn pango_layout_xy_to_index(layout: *mut c_void, x: c_int, y: c_int, index: *mut c_int, trailing: *mut c_int) -> GBoolean;
    fn pango_layout_get_log_attrs_readonly(layout: *mut c_void, n_attrs: *mut c_int) -> *const LogAttr;
    fn pango_layout_get_iter(layout: *mut c_void) -> *mut c_void;
    fn pango_layout_iter_free(iter: *mut c_void);
    fn pango_layout_iter_next_line(iter: *mut c_void) -> GBoolean;
    fn pango_layout_iter_get_line_readonly(iter: *mut c_void) -> *mut RawLine;
    fn pango_layout_iter_get_line_extents(iter: *mut c_void, ink: *mut Rectangle, logical: *mut Rectangle);
    fn pango_layout_iter_get_baseline(iter: *mut c_void) -> c_int;
    fn pango_layout_line_index_to_x(line: *mut RawLine, index: c_int, trailing: GBoolean, x: *mut c_int);
    fn pango_layout_line_get_x_ranges(line: *mut RawLine, start: c_int, end: c_int, ranges: *mut *mut c_int, n: *mut c_int);
    fn pango_tab_array_new(size: c_int, in_pixels: GBoolean) -> *mut c_void;
    fn pango_tab_array_set_tab(tabs: *mut c_void, index: c_int, alignment: c_int, location: c_int);
    fn pango_tab_array_free(tabs: *mut c_void);
    fn pango_attr_list_new() -> *mut c_void;
    fn pango_attr_list_unref(list: *mut c_void);
    fn pango_attr_list_insert(list: *mut c_void, attr: *mut RawAttribute);
    fn pango_attribute_copy(attr: *const RawAttribute) -> *mut RawAttribute;
    fn pango_attribute_destroy(attr: *mut RawAttribute);
    fn pango_language_from_string(language: *const c_char) -> *mut c_void;
    fn pango_attr_language_new(language: *mut c_void) -> *mut RawAttribute;
    fn pango_attr_insert_hyphens_new(insert: GBoolean) -> *mut RawAttribute;
    fn pango_attr_font_features_new(features: *const c_char) -> *mut RawAttribute;
    fn pango_attr_font_desc_new(desc: *const c_void) -> *mut RawAttribute;
    fn pango_attr_family_new(family: *const c_char) -> *mut RawAttribute;
    fn pango_attr_weight_new(weight: c_int) -> *mut RawAttribute;
    fn pango_attr_stretch_new(stretch: c_int) -> *mut RawAttribute;
    fn pango_attr_style_new(style: c_int) -> *mut RawAttribute;
    fn pango_attr_variant_new(variant: c_int) -> *mut RawAttribute;
    fn pango_attr_size_new_absolute(size: c_int) -> *mut RawAttribute;
    fn pango_attr_scale_new(scale: c_double) -> *mut RawAttribute;
    fn pango_attr_rise_new(rise: c_int) -> *mut RawAttribute;
    fn pango_attr_letter_spacing_new(spacing: c_int) -> *mut RawAttribute;
    fn pango_attr_foreground_new(r: u16, g: u16, b: u16) -> *mut RawAttribute;
    fn pango_attr_foreground_alpha_new(alpha: u16) -> *mut RawAttribute;
    fn pango_attr_background_new(r: u16, g: u16, b: u16) -> *mut RawAttribute;
    fn pango_attr_background_alpha_new(alpha: u16) -> *mut RawAttribute;
    fn pango_attr_underline_new(underline: c_int) -> *mut RawAttribute;
    fn pango_attr_underline_color_new(r: u16, g: u16, b: u16) -> *mut RawAttribute;
    fn pango_attr_overline_new(overline: c_int) -> *mut RawAttribute;
    fn pango_attr_overline_color_new(r: u16, g: u16, b: u16) -> *mut RawAttribute;
    fn pango_attr_strikethrough_new(strike: GBoolean) -> *mut RawAttribute;
    fn pango_attr_strikethrough_color_new(r: u16, g: u16, b: u16) -> *mut RawAttribute;
    fn pango_attr_allow_breaks_new(allow: GBoolean) -> *mut RawAttribute;
    fn pango_attr_shape_new(ink: *const Rectangle, logical: *const Rectangle) -> *mut RawAttribute;
    fn pango_cairo_show_layout(cr: *mut c_void, layout: *mut c_void);
    fn pango_context_get_serial(context: *mut c_void) -> c_uint;
    fn pango_font_description_copy(desc: *const c_void) -> *mut c_void;
    fn pango_font_description_equal(a: *const c_void, b: *const c_void) -> GBoolean;
    fn pango_font_description_hash(desc: *const c_void) -> c_uint;
    fn pango_layout_get_attributes(layout: *mut c_void) -> *mut c_void;
    fn pango_layout_get_tabs(layout: *mut c_void) -> *mut c_void;
    fn pango_layout_get_height(layout: *mut c_void) -> c_int;
    fn pango_layout_get_indent(layout: *mut c_void) -> c_int;
    fn pango_layout_get_spacing(layout: *mut c_void) -> c_int;
    fn pango_layout_get_line_spacing(layout: *mut c_void) -> f32;
    fn pango_layout_get_justify(layout: *mut c_void) -> GBoolean;
    fn pango_layout_get_single_paragraph_mode(layout: *mut c_void) -> GBoolean;
    fn pango_layout_get_auto_dir(layout: *mut c_void) -> GBoolean;
    fn pango_layout_get_wrap(layout: *mut c_void) -> c_int;
    fn pango_layout_get_ellipsize(layout: *mut c_void) -> c_int;
    fn pango_layout_get_extents(layout: *mut c_void, ink: *mut Rectangle, logical: *mut Rectangle);
    fn pango_layout_get_baseline(layout: *mut c_void) -> c_int;
    fn pango_layout_index_to_line_x(layout: *mut c_void, index: c_int, trailing: GBoolean, line: *mut c_int, x_pos: *mut c_int);
    fn pango_layout_context_changed(layout: *mut c_void);
    fn pango_extents_to_pixels(inclusive: *mut Rectangle, nearest: *mut Rectangle);
    fn pango_attr_list_copy(list: *mut c_void) -> *mut c_void;
    fn pango_attr_list_to_string(list: *mut c_void) -> *mut c_char;
    fn pango_attr_line_height_new_absolute(height: c_int) -> *mut RawAttribute;
    fn pango_cairo_show_layout_line(cr: *mut c_void, line: *mut RawLine);
}

unsafe extern "C" {
    fn g_object_ref(object: *mut c_void) -> *mut c_void;
    fn g_object_unref(object: *mut c_void);
    fn g_object_set_data_full(
        object: *mut c_void,
        key: *const c_char,
        data: *mut c_void,
        destroy: southstar_glib::GDestroyNotify,
    );
    fn g_object_get_data(object: *mut c_void, key: *const c_char) -> *mut c_void;
    fn g_free(mem: *mut c_void);
    fn cairo_font_options_create() -> *mut c_void;
    fn cairo_font_options_destroy(options: *mut c_void);
    fn cairo_font_options_merge(options: *mut c_void, other: *const c_void);
    fn cairo_font_options_set_antialias(options: *mut c_void, antialias: c_int);
    fn cairo_font_options_set_subpixel_order(options: *mut c_void, order: c_int);
    fn cairo_font_options_set_hint_metrics(options: *mut c_void, metrics: c_int);
    #[cfg(target_os = "macos")]
    fn cairo_font_options_set_hint_style(options: *mut c_void, style: c_int);
}

#[derive(Clone, Copy)]
pub struct FontMap(*mut c_void);

impl FontMap {
    pub fn default_map() -> Option<FontMap> {
        let map = unsafe { pango_cairo_font_map_get_default() };
        (!map.is_null()).then_some(FontMap(map))
    }

    pub fn serial(self) -> u32 {
        unsafe { pango_font_map_get_serial(self.0) }
    }

    pub fn family_names(self) -> Vec<Vec<u8>> {
        let mut families: *mut *mut c_void = ptr::null_mut();
        let mut n: c_int = 0;
        unsafe { pango_font_map_list_families(self.0, &mut families, &mut n) };
        let mut out = Vec::new();
        for i in 0..usize::try_from(n).unwrap_or(0) {
            let name = unsafe { pango_font_family_get_name(*families.add(i)) };
            if !name.is_null() {
                out.push(unsafe { CStr::from_ptr(name) }.to_bytes().to_vec());
            }
        }
        unsafe { g_free(families.cast()) };
        out
    }
}

pub fn font_map_serial() -> u32 {
    FontMap::default_map().map_or(0, FontMap::serial)
}

#[derive(Clone, Copy)]
pub struct FontDescRef(*const c_void);

#[derive(Clone, Copy)]
pub struct Context(*mut c_void);

impl Context {
    pub fn new_text_context() -> Context {
        let map = unsafe { pango_cairo_font_map_get_default() };
        let ctx = unsafe { pango_font_map_create_context(map) };
        unsafe {
            let fo = cairo_font_options_create();
            let base = pango_cairo_context_get_font_options(ctx);
            if !base.is_null() {
                cairo_font_options_merge(fo, base);
            }
            cairo_font_options_set_antialias(fo, CAIRO_ANTIALIAS_GRAY);
            cairo_font_options_set_subpixel_order(fo, CAIRO_SUBPIXEL_ORDER_DEFAULT);
            cairo_font_options_set_hint_metrics(fo, CAIRO_HINT_METRICS_OFF);
            #[cfg(target_os = "macos")]
            cairo_font_options_set_hint_style(fo, CAIRO_HINT_STYLE_NONE);
            pango_cairo_context_set_font_options(ctx, fo);
            cairo_font_options_destroy(fo);
            pango_context_set_round_glyph_positions(ctx, 0);
            pango_cairo_context_set_resolution(ctx, 72.0);
        }
        Context(ctx)
    }

    pub unsafe fn from_raw(ctx: *mut c_void) -> Context {
        Context(ctx)
    }

    pub fn raw(self) -> *mut c_void {
        self.0
    }

    pub fn serial(self) -> u32 {
        unsafe { pango_context_get_serial(self.0) }
    }

    pub fn base_dir(self) -> c_int {
        unsafe { pango_context_get_base_dir(self.0) }
    }

    pub fn set_base_dir(self, direction: c_int) {
        unsafe { pango_context_set_base_dir(self.0, direction) };
    }

    pub fn metrics(self, desc: FontDescRef) -> Option<FontMetrics> {
        let m = unsafe { pango_context_get_metrics(self.0, desc.0, ptr::null_mut()) };
        (!m.is_null()).then_some(FontMetrics(m))
    }

    pub fn font_description(self) -> FontDescRef {
        FontDescRef(unsafe { pango_context_get_font_description(self.0) })
    }
}

pub struct FontMetrics(*mut c_void);

impl FontMetrics {
    pub fn ascent(&self) -> c_int {
        unsafe { pango_font_metrics_get_ascent(self.0) }
    }

    pub fn descent(&self) -> c_int {
        unsafe { pango_font_metrics_get_descent(self.0) }
    }

    pub fn height(&self) -> c_int {
        unsafe { pango_font_metrics_get_height(self.0) }
    }

    pub fn approximate_char_width(&self) -> c_int {
        unsafe { pango_font_metrics_get_approximate_char_width(self.0) }
    }
}

impl Drop for FontMetrics {
    fn drop(&mut self) {
        unsafe { pango_font_metrics_unref(self.0) };
    }
}

pub struct FontDescription(*mut c_void);

impl Default for FontDescription {
    fn default() -> FontDescription {
        FontDescription::new()
    }
}

impl FontDescription {
    pub fn new() -> FontDescription {
        FontDescription(unsafe { pango_font_description_new() })
    }

    pub fn from_string(text: &CStr) -> FontDescription {
        FontDescription(unsafe { pango_font_description_from_string(text.as_ptr()) })
    }

    pub fn as_ref(&self) -> FontDescRef {
        FontDescRef(self.0)
    }

    pub fn set_family(&self, family: &CStr) {
        unsafe { pango_font_description_set_family(self.0, family.as_ptr()) };
    }

    pub fn set_weight(&self, weight: c_int) {
        unsafe { pango_font_description_set_weight(self.0, weight) };
    }

    pub fn set_style(&self, style: c_int) {
        unsafe { pango_font_description_set_style(self.0, style) };
    }

    pub fn set_stretch(&self, stretch: c_int) {
        unsafe { pango_font_description_set_stretch(self.0, stretch) };
    }

    pub fn set_absolute_size(&self, size: f64) {
        unsafe { pango_font_description_set_absolute_size(self.0, size) };
    }

    pub fn set_variations(&self, variations: &CStr) {
        unsafe { pango_font_description_set_variations(self.0, variations.as_ptr()) };
    }
}

impl FontDescRef {
    pub fn raw(self) -> *const c_void {
        self.0
    }

    pub fn hash_value(self) -> u32 {
        unsafe { pango_font_description_hash(self.0) }
    }

    pub fn equal(self, other: FontDescRef) -> bool {
        unsafe { pango_font_description_equal(self.0, other.0) != 0 }
    }

    pub fn copy_owned(self) -> FontDescription {
        FontDescription(unsafe { pango_font_description_copy(self.0) })
    }
}

impl Drop for FontDescription {
    fn drop(&mut self) {
        unsafe { pango_font_description_free(self.0) };
    }
}

pub struct Layout(*mut c_void);

impl Layout {
    pub fn new(ctx: Context) -> Layout {
        Layout(unsafe { pango_layout_new(ctx.0) })
    }

    pub unsafe fn from_borrowed(layout: *mut c_void) -> Option<Layout> {
        (!layout.is_null()).then(|| Layout(unsafe { g_object_ref(layout) }))
    }

    pub unsafe fn from_owned(layout: *mut c_void) -> Layout {
        Layout(layout)
    }

    pub unsafe fn borrowed(layout: *mut c_void) -> Option<core::mem::ManuallyDrop<Layout>> {
        (!layout.is_null()).then(|| core::mem::ManuallyDrop::new(Layout(layout)))
    }

    pub fn raw(&self) -> *mut c_void {
        self.0
    }

    pub fn into_raw(self) -> *mut c_void {
        let raw = self.0;
        core::mem::forget(self);
        raw
    }

    pub fn new_ref(&self) -> *mut c_void {
        unsafe { g_object_ref(self.0) }
    }

    pub fn context(&self) -> Context {
        Context(unsafe { pango_layout_get_context(self.0) })
    }

    pub fn set_font_description(&self, desc: FontDescRef) {
        unsafe { pango_layout_set_font_description(self.0, desc.0) };
    }

    pub fn font_description(&self) -> Option<FontDescRef> {
        let desc = unsafe { pango_layout_get_font_description(self.0) };
        (!desc.is_null()).then_some(FontDescRef(desc))
    }

    pub fn set_text(&self, text: &CStr) {
        unsafe { pango_layout_set_text(self.0, text.as_ptr(), -1) };
    }

    pub fn set_text_bytes(&self, text: &[u8]) {
        unsafe { pango_layout_set_text(self.0, text.as_ptr().cast(), text.len() as c_int) };
    }

    pub fn text(&self) -> Option<&CStr> {
        let t = unsafe { pango_layout_get_text(self.0) };
        (!t.is_null()).then(|| unsafe { CStr::from_ptr(t) })
    }

    pub fn clear_attributes(&self) {
        unsafe { pango_layout_set_attributes(self.0, ptr::null_mut()) };
    }

    pub fn set_width(&self, width: c_int) {
        unsafe { pango_layout_set_width(self.0, width) };
    }

    pub fn width(&self) -> c_int {
        unsafe { pango_layout_get_width(self.0) }
    }

    pub fn set_height(&self, height: c_int) {
        unsafe { pango_layout_set_height(self.0, height) };
    }

    pub fn set_wrap(&self, wrap: c_int) {
        unsafe { pango_layout_set_wrap(self.0, wrap) };
    }

    pub fn set_indent(&self, indent: c_int) {
        unsafe { pango_layout_set_indent(self.0, indent) };
    }

    pub fn set_ellipsize(&self, ellipsize: c_int) {
        unsafe { pango_layout_set_ellipsize(self.0, ellipsize) };
    }

    pub fn set_alignment(&self, alignment: c_int) {
        unsafe { pango_layout_set_alignment(self.0, alignment) };
    }

    pub fn alignment(&self) -> c_int {
        unsafe { pango_layout_get_alignment(self.0) }
    }

    pub fn set_justify(&self, justify: bool) {
        unsafe { pango_layout_set_justify(self.0, GBoolean::from(justify)) };
    }

    pub fn set_auto_dir(&self, auto_dir: bool) {
        unsafe { pango_layout_set_auto_dir(self.0, GBoolean::from(auto_dir)) };
    }

    pub fn set_tabs(&self, tabs: &TabArray) {
        unsafe { pango_layout_set_tabs(self.0, tabs.0) };
    }

    pub fn size(&self) -> (c_int, c_int) {
        let (mut w, mut h) = (0, 0);
        unsafe { pango_layout_get_size(self.0, &mut w, &mut h) };
        (w, h)
    }

    pub fn pixel_size(&self) -> (c_int, c_int) {
        let (mut w, mut h) = (0, 0);
        unsafe { pango_layout_get_pixel_size(self.0, &mut w, &mut h) };
        (w, h)
    }

    pub fn pixel_ink_extents(&self) -> Rectangle {
        let mut ink = Rectangle::default();
        unsafe { pango_layout_get_pixel_extents(self.0, &mut ink, ptr::null_mut()) };
        ink
    }

    pub fn line_count(&self) -> c_int {
        unsafe { pango_layout_get_line_count(self.0) }
    }

    pub fn line(&self, index: c_int) -> Option<Line<'_>> {
        let line = unsafe { pango_layout_get_line_readonly(self.0, index) };
        unsafe { line.as_ref() }.map(Line)
    }

    pub fn index_to_pos(&self, index: c_int) -> Rectangle {
        let mut pos = Rectangle::default();
        unsafe { pango_layout_index_to_pos(self.0, index, &mut pos) };
        pos
    }

    pub fn xy_to_index(&self, x: c_int, y: c_int) -> (c_int, c_int) {
        let (mut index, mut trailing) = (0, 0);
        unsafe { pango_layout_xy_to_index(self.0, x, y, &mut index, &mut trailing) };
        (index, trailing)
    }

    pub fn log_attrs(&self) -> &[LogAttr] {
        let mut n: c_int = 0;
        let attrs = unsafe { pango_layout_get_log_attrs_readonly(self.0, &mut n) };
        if attrs.is_null() || n <= 0 {
            return &[];
        }
        unsafe { core::slice::from_raw_parts(attrs, n as usize) }
    }

    pub fn iter(&self) -> LayoutIter<'_> {
        LayoutIter(
            unsafe { pango_layout_get_iter(self.0) },
            core::marker::PhantomData,
        )
    }

    pub fn set_attributes(&self, attrs: &AttrList) {
        unsafe { pango_layout_set_attributes(self.0, attrs.0) };
    }

    pub fn attributes_string(&self) -> Option<southstar_glib::GStr> {
        let attrs = unsafe { pango_layout_get_attributes(self.0) };
        if attrs.is_null() {
            return None;
        }
        unsafe { southstar_glib::GStr::take(pango_attr_list_to_string(attrs)) }
    }

    pub fn has_tabs(&self) -> bool {
        let tabs = unsafe { pango_layout_get_tabs(self.0) };
        if tabs.is_null() {
            return false;
        }
        unsafe { pango_tab_array_free(tabs) };
        true
    }

    pub fn height(&self) -> c_int {
        unsafe { pango_layout_get_height(self.0) }
    }

    pub fn indent(&self) -> c_int {
        unsafe { pango_layout_get_indent(self.0) }
    }

    pub fn spacing(&self) -> c_int {
        unsafe { pango_layout_get_spacing(self.0) }
    }

    pub fn line_spacing(&self) -> f32 {
        unsafe { pango_layout_get_line_spacing(self.0) }
    }

    pub fn justify(&self) -> bool {
        unsafe { pango_layout_get_justify(self.0) != 0 }
    }

    pub fn single_paragraph_mode(&self) -> bool {
        unsafe { pango_layout_get_single_paragraph_mode(self.0) != 0 }
    }

    pub fn auto_dir(&self) -> bool {
        unsafe { pango_layout_get_auto_dir(self.0) != 0 }
    }

    pub fn wrap(&self) -> c_int {
        unsafe { pango_layout_get_wrap(self.0) }
    }

    pub fn ellipsize(&self) -> c_int {
        unsafe { pango_layout_get_ellipsize(self.0) }
    }

    pub fn logical_extents(&self) -> Rectangle {
        let mut logical = Rectangle::default();
        unsafe { pango_layout_get_extents(self.0, ptr::null_mut(), &mut logical) };
        logical
    }

    pub fn baseline(&self) -> c_int {
        unsafe { pango_layout_get_baseline(self.0) }
    }

    pub fn index_to_line(&self, index: c_int) -> c_int {
        let mut line = 0;
        unsafe { pango_layout_index_to_line_x(self.0, index, 0, &mut line, ptr::null_mut()) };
        line
    }

    pub fn context_changed(&self) {
        unsafe { pango_layout_context_changed(self.0) };
    }

    pub fn css_line_height(&self, key: &CStr) -> Option<f64> {
        let data = unsafe { g_object_get_data(self.0, key.as_ptr()) }.cast::<f64>();
        (!data.is_null()).then(|| unsafe { *data })
    }

    pub fn set_css_line_height(&self, key: &CStr, line_height: f64) {
        let data = unsafe { southstar_glib::g_malloc(core::mem::size_of::<f64>()) }.cast::<f64>();
        unsafe {
            data.write(line_height);
            g_object_set_data_full(self.0, key.as_ptr(), data.cast(), Some(g_free));
        }
    }

    pub fn show(&self, cr: Cr) {
        unsafe { pango_cairo_show_layout(cr.raw(), self.0) };
    }
}

pub fn extents_to_pixels(rect: &mut Rectangle) {
    unsafe { pango_extents_to_pixels(rect, ptr::null_mut()) };
}

impl Clone for Layout {
    fn clone(&self) -> Layout {
        Layout(unsafe { g_object_ref(self.0) })
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        unsafe { g_object_unref(self.0) };
    }
}

#[derive(Clone, Copy)]
pub struct Line<'a>(&'a RawLine);

pub struct Run {
    pub offset: c_int,
    pub length: c_int,
    pub y_offset: c_int,
    pub is_spacer: bool,
}

impl Line<'_> {
    pub fn start_index(self) -> c_int {
        self.0.start_index
    }

    pub fn length(self) -> c_int {
        self.0.length
    }

    fn raw(self) -> *mut RawLine {
        ptr::from_ref(self.0).cast_mut()
    }

    pub fn index_to_x(self, index: c_int, trailing: bool) -> c_int {
        let mut x = 0;
        unsafe {
            pango_layout_line_index_to_x(self.raw(), index, GBoolean::from(trailing), &mut x)
        };
        x
    }

    pub fn x_ranges(self, start: c_int, end: c_int) -> Vec<(c_int, c_int)> {
        let mut ranges: *mut c_int = ptr::null_mut();
        let mut n: c_int = 0;
        unsafe { pango_layout_line_get_x_ranges(self.raw(), start, end, &mut ranges, &mut n) };
        let out = (0..usize::try_from(n).unwrap_or(0))
            .map(|i| unsafe { (*ranges.add(2 * i), *ranges.add(2 * i + 1)) })
            .collect();
        unsafe { g_free(ranges.cast()) };
        out
    }

    pub fn runs(self) -> Vec<Run> {
        let mut out = Vec::new();
        let mut node = self.0.runs;
        while let Some(link) = unsafe { node.as_ref() } {
            let run = unsafe { &*link.data.cast::<GlyphItem>() };
            let item = unsafe { &*run.item };
            let mut is_spacer = false;
            let mut attr = item.analysis.extra_attrs;
            while let Some(a) = unsafe { attr.as_ref() } {
                let raw = unsafe { &*a.data.cast::<RawAttribute>() };
                if unsafe { (*raw.klass).kind } == ATTR_SHAPE {
                    is_spacer = true;
                    break;
                }
                attr = a.next;
            }
            out.push(Run {
                offset: item.offset,
                length: item.length,
                y_offset: run.y_offset,
                is_spacer,
            });
            node = link.next;
        }
        out
    }

    pub fn show(self, cr: Cr) {
        unsafe { pango_cairo_show_layout_line(cr.raw(), self.raw()) };
    }
}

pub struct LayoutIter<'a>(*mut c_void, core::marker::PhantomData<&'a Layout>);

impl<'a> LayoutIter<'a> {
    pub fn next_line(&mut self) -> bool {
        unsafe { pango_layout_iter_next_line(self.0) != 0 }
    }

    pub fn line(&self) -> Option<Line<'a>> {
        let line = unsafe { pango_layout_iter_get_line_readonly(self.0) };
        unsafe { line.as_ref() }.map(Line)
    }

    pub fn line_logical_extents(&self) -> Rectangle {
        let mut logical = Rectangle::default();
        unsafe { pango_layout_iter_get_line_extents(self.0, ptr::null_mut(), &mut logical) };
        logical
    }

    pub fn baseline(&self) -> c_int {
        unsafe { pango_layout_iter_get_baseline(self.0) }
    }
}

impl Drop for LayoutIter<'_> {
    fn drop(&mut self) {
        unsafe { pango_layout_iter_free(self.0) };
    }
}

pub struct TabArray(*mut c_void);

impl TabArray {
    pub fn new(size: c_int, in_pixels: bool) -> TabArray {
        TabArray(unsafe { pango_tab_array_new(size, GBoolean::from(in_pixels)) })
    }

    pub fn set_tab(&self, index: c_int, alignment: c_int, location: c_int) {
        unsafe { pango_tab_array_set_tab(self.0, index, alignment, location) };
    }
}

impl Drop for TabArray {
    fn drop(&mut self) {
        unsafe { pango_tab_array_free(self.0) };
    }
}

pub struct AttrList(*mut c_void);

impl Default for AttrList {
    fn default() -> AttrList {
        AttrList::new()
    }
}

impl AttrList {
    pub fn new() -> AttrList {
        AttrList(unsafe { pango_attr_list_new() })
    }

    pub unsafe fn borrowed(list: *mut c_void) -> core::mem::ManuallyDrop<AttrList> {
        core::mem::ManuallyDrop::new(AttrList(list))
    }

    pub fn raw(&self) -> *mut c_void {
        self.0
    }

    pub fn insert(&self, attr: Attribute, start: u32, end: u32) {
        let raw = attr.into_raw();
        unsafe {
            (*raw).start_index = start;
            (*raw).end_index = end;
            pango_attr_list_insert(self.0, raw);
        }
    }

    pub fn copy(&self) -> AttrList {
        AttrList(unsafe { pango_attr_list_copy(self.0) })
    }

    pub fn insert_range(&self, attr: Option<Attribute>, start: usize, len: usize) {
        if let Some(attr) = attr {
            self.insert(attr, start as u32, (start + len) as u32);
        }
    }
}

impl Drop for AttrList {
    fn drop(&mut self) {
        unsafe { pango_attr_list_unref(self.0) };
    }
}

pub struct Attribute(*mut RawAttribute);

impl Attribute {
    fn wrap(raw: *mut RawAttribute) -> Attribute {
        Attribute(raw)
    }

    pub unsafe fn from_raw(raw: *mut RawAttribute) -> Option<Attribute> {
        (!raw.is_null()).then_some(Attribute(raw))
    }

    pub fn into_raw(self) -> *mut RawAttribute {
        let raw = self.0;
        core::mem::forget(self);
        raw
    }

    pub fn copy(&self) -> Attribute {
        Attribute(unsafe { pango_attribute_copy(self.0) })
    }

    pub fn insert_hyphens(insert: bool) -> Attribute {
        Self::wrap(unsafe { pango_attr_insert_hyphens_new(GBoolean::from(insert)) })
    }

    pub fn language(lang: &CStr) -> Attribute {
        Self::wrap(unsafe { pango_attr_language_new(pango_language_from_string(lang.as_ptr())) })
    }

    pub fn font_features(features: &CStr) -> Attribute {
        Self::wrap(unsafe { pango_attr_font_features_new(features.as_ptr()) })
    }

    pub fn font_desc(desc: &FontDescription) -> Attribute {
        Self::wrap(unsafe { pango_attr_font_desc_new(desc.0) })
    }

    pub fn family(family: &CStr) -> Attribute {
        Self::wrap(unsafe { pango_attr_family_new(family.as_ptr()) })
    }

    pub fn weight(weight: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_weight_new(weight) })
    }

    pub fn stretch(stretch: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_stretch_new(stretch) })
    }

    pub fn style(style: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_style_new(style) })
    }

    pub fn variant(variant: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_variant_new(variant) })
    }

    pub fn size_absolute(size: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_size_new_absolute(size) })
    }

    pub fn scale(scale: f64) -> Attribute {
        Self::wrap(unsafe { pango_attr_scale_new(scale) })
    }

    pub fn rise(rise: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_rise_new(rise) })
    }

    pub fn letter_spacing(spacing: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_letter_spacing_new(spacing) })
    }

    pub fn foreground(r: u16, g: u16, b: u16) -> Attribute {
        Self::wrap(unsafe { pango_attr_foreground_new(r, g, b) })
    }

    pub fn foreground_alpha(alpha: u16) -> Attribute {
        Self::wrap(unsafe { pango_attr_foreground_alpha_new(alpha) })
    }

    pub fn background(r: u16, g: u16, b: u16) -> Attribute {
        Self::wrap(unsafe { pango_attr_background_new(r, g, b) })
    }

    pub fn background_alpha(alpha: u16) -> Attribute {
        Self::wrap(unsafe { pango_attr_background_alpha_new(alpha) })
    }

    pub fn underline(underline: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_underline_new(underline) })
    }

    pub fn underline_color(r: u16, g: u16, b: u16) -> Attribute {
        Self::wrap(unsafe { pango_attr_underline_color_new(r, g, b) })
    }

    pub fn overline(overline: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_overline_new(overline) })
    }

    pub fn overline_color(r: u16, g: u16, b: u16) -> Attribute {
        Self::wrap(unsafe { pango_attr_overline_color_new(r, g, b) })
    }

    pub fn strikethrough(strike: bool) -> Attribute {
        Self::wrap(unsafe { pango_attr_strikethrough_new(GBoolean::from(strike)) })
    }

    pub fn strikethrough_color(r: u16, g: u16, b: u16) -> Attribute {
        Self::wrap(unsafe { pango_attr_strikethrough_color_new(r, g, b) })
    }

    pub fn allow_breaks(allow: bool) -> Attribute {
        Self::wrap(unsafe { pango_attr_allow_breaks_new(GBoolean::from(allow)) })
    }

    pub fn line_height_absolute(height: c_int) -> Attribute {
        Self::wrap(unsafe { pango_attr_line_height_new_absolute(height) })
    }

    pub fn shape_rect(rect: Rectangle) -> Attribute {
        Self::wrap(unsafe { pango_attr_shape_new(&rect, &rect) })
    }

    pub fn shape(width: c_int) -> Attribute {
        let rect = Rectangle {
            x: 0,
            y: 0,
            width,
            height: 0,
        };
        Self::wrap(unsafe { pango_attr_shape_new(&rect, &rect) })
    }
}

impl Drop for Attribute {
    fn drop(&mut self) {
        unsafe { pango_attribute_destroy(self.0) };
    }
}
