//! Southstar text — fonts: one fontconfig pattern rendered through cairo and shaped through HarfBuzz, with its metrics, glyph extents and hex boxes.
//! Copyright 1999 Red Hat Software, 2000 Tor Lillqvist, 2005 Keith Packard; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::cell::Cell;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};
use std::thread::ThreadId;

use crate::context::layout_hooks;
use crate::coverage::Coverage;
use crate::ffi::fontconfig::{
    FC_DPI, FC_EMBOLDEN, FC_FONT_FEATURES, FC_FONT_VARIATIONS, FC_INDEX, FC_MATRIX, FC_PIXEL_SIZE,
    FC_SIZE, FcMatrix, PANGO_FC_GRAVITY, Pattern,
};
use crate::ffi::harfbuzz::{
    self, HbFace, HbFontBuilder, METRICS_STRIKEOUT_OFFSET, METRICS_STRIKEOUT_SIZE,
    METRICS_UNDERLINE_OFFSET, METRICS_UNDERLINE_SIZE,
};
use crate::font_description::{FontDescription, c_double_to_int};
use crate::fontmap::{
    FaceData, FontKey, FontMap, FontMapShared, font_description_from_pattern, gravity_from_nick,
};
use crate::gravity::gravity_to_rotation;
use crate::language::{Language, language_get_sample_string, script_get_sample_language};
use crate::matrix::{Matrix, matrix_get_font_scale_factors};
use crate::metrics::FontMetrics;
use crate::script::Script;
use crate::types::{
    CoverageLevel, GLYPH_EMPTY, GLYPH_INVALID_INPUT, GLYPH_UNKNOWN_FLAG, Glyph, Gravity, Rectangle,
    SCALE, Variant, font_mask, get_unknown_glyph, pixels_ceil, units_ceil, units_floor,
    units_from_double, units_round,
};
use crate::unicode::{UnicodeType, is_wide, is_zero_width, unichar_type};

pub use crate::ffi::cairo::{
    Antialias, CairoContext, CairoMatrix, FontExtents, FontOptions, HintMetrics, HintStyle,
    ScaledFont, SubpixelOrder, TextExtents,
};
pub use crate::ffi::harfbuzz::{Feature, HbFont};

#[cfg(test)]
#[path = "tests/fonts.rs"]
mod tests;

pub const UNKNOWN_GLYPH_WIDTH: i32 = 10;
pub const UNKNOWN_GLYPH_HEIGHT: i32 = 14;

const GLYPH_CACHE_NUM_ENTRIES: usize = 256;
const GLYPH_CACHE_MASK: u32 = (GLYPH_CACHE_NUM_ENTRIES - 1) as u32;

#[derive(Clone)]
pub struct FontCoverage(Arc<Mutex<Coverage>>);

impl FontCoverage {
    pub fn new(coverage: Coverage) -> FontCoverage {
        FontCoverage(Arc::new(Mutex::new(coverage)))
    }

    fn lock(&self) -> MutexGuard<'_, Coverage> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn get(&self, ch: u32) -> CoverageLevel {
        self.lock().get(ch as i32)
    }

    pub fn set(&self, ch: u32, level: CoverageLevel) {
        self.lock().set(ch as i32, level);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct HbFontSetup {
    pub x_scale: i32,
    pub y_scale: i32,
    pub point_size: f32,
    pub slant: f32,
    pub embolden: f32,
}

#[derive(Clone, Debug)]
pub struct HexBoxInfo {
    pub font: Font,
    pub rows: i32,
    pub digit_width: f64,
    pub digit_height: f64,
    pub pad_x: f64,
    pub pad_y: f64,
    pub line_width: f64,
    pub box_descent: f64,
    pub box_height: f64,
}

#[derive(Clone, Copy, Default)]
struct GlyphExtentsEntry {
    glyph: Glyph,
    width: i32,
    ink_rect: Rectangle,
}

struct GlyphExtentsCache {
    font_extents: Rectangle,
    entries: Option<Box<[GlyphExtentsEntry; GLYPH_CACHE_NUM_ENTRIES]>>,
}

#[derive(Default)]
struct HexBoxGlyphs {
    base: u32,
    map: HashMap<Glyph, u32>,
    glyphs: Vec<Glyph>,
}

struct MetricsInfo {
    sample_str: &'static str,
    metrics: FontMetrics,
    computing: Option<ThreadId>,
}

pub(crate) struct FontInner {
    fontmap: Weak<FontMapShared>,
    key: FontKey,
    description: FontDescription,
    is_hinted: bool,
    is_transformed: bool,
    gravity: Gravity,
    font_matrix: CairoMatrix,
    ctm: CairoMatrix,
    options: FontOptions,
    face_data: OnceLock<Option<Arc<FaceData>>>,
    scaled_font: OnceLock<Option<ScaledFont>>,
    hb_font: OnceLock<HbFont>,
    hex_box_info: OnceLock<Option<HexBoxInfo>>,
    hex_box_scaled_font: Mutex<Option<ScaledFont>>,
    hex_box_glyphs: Mutex<HexBoxGlyphs>,
    glyph_cache: Mutex<GlyphExtentsCache>,
    metrics_by_lang: Mutex<Vec<MetricsInfo>>,
}

#[derive(Clone)]
pub struct Font(Arc<FontInner>);

impl PartialEq for Font {
    fn eq(&self, other: &Font) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Font {}

impl core::hash::Hash for Font {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl core::fmt::Debug for Font {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Font({})", self.0.description)
    }
}

thread_local! {
    static IN_GET_METRICS: Cell<bool> = const { Cell::new(false) };
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn pattern_size(pattern: &Pattern) -> f64 {
    if let Some(size) = pattern.get_double(FC_PIXEL_SIZE, 0) {
        return size;
    }
    let dpi = pattern.get_double(FC_DPI, 0).unwrap_or(72.0);
    if let Some(size) = pattern.get_double(FC_SIZE, 0) {
        return size * dpi / 72.;
    }
    18.
}

fn pattern_gravity(pattern: &Pattern) -> Gravity {
    pattern
        .get_string(PANGO_FC_GRAVITY, 0)
        .map_or(Gravity::South, |nick| gravity_from_nick(&nick))
}

const IGNORABLES: [(u32, &str); 24] = [
    (0x00ad, "SHY"),
    (0x034f, "CGJ"),
    (0x061c, "ALM"),
    (0x200b, "ZWS"),
    (0x200c, "ZWNJ"),
    (0x200d, "ZWJ"),
    (0x200e, "LRM"),
    (0x200f, "RLM"),
    (0x2028, "LS"),
    (0x2029, "PS"),
    (0x202a, "LRE"),
    (0x202b, "RLE"),
    (0x202c, "PDF"),
    (0x202d, "LRO"),
    (0x202e, "RLO"),
    (0x2060, "WJ"),
    (0x2061, "FA"),
    (0x2062, "IT"),
    (0x2063, "IS"),
    (0x2066, "LRI"),
    (0x2067, "RLI"),
    (0x2068, "FSI"),
    (0x2069, "PDI"),
    (0xfeff, "ZWNBS"),
];

pub fn get_ignorable(ch: u32) -> Option<&'static str> {
    for &(ignorable, nick) in &IGNORABLES {
        if ch < ignorable {
            return None;
        }
        if ch == ignorable {
            return Some(nick);
        }
    }
    None
}

pub fn get_ignorable_size(ch: u32) -> Option<(&'static str, i32, i32)> {
    let nick = get_ignorable(ch)?;
    let len = nick.len() as i32;
    let (rows, cols) = match len {
        0..=3 => (1, len),
        4 => (2, 2),
        _ => (2, 3),
    };
    Some((nick, rows, cols))
}

fn unichar_width(ch: u32) -> i64 {
    if is_zero_width(ch) {
        0
    } else if is_wide(ch) {
        2
    } else {
        1
    }
}

pub fn utf8_strwidth(text: &str) -> i64 {
    text.chars().map(|ch| unichar_width(u32::from(ch))).sum()
}

pub fn parse_variations(variations: &str, axes: &[(u32, u32)], coords: &mut [f32]) {
    let mut rest = Some(variations);
    while let Some(p) = rest.filter(|p| !p.is_empty()) {
        let (item, next) = match p.find(',') {
            Some(comma) => (&p[..comma], Some(&p[comma + 1..])),
            None => (p, None),
        };
        if let Some((tag, value)) = harfbuzz::variation_from_string(item.as_bytes()) {
            for &(axis_tag, axis_index) in axes {
                if axis_tag == tag {
                    if let Some(coord) = coords.get_mut(axis_index as usize) {
                        *coord = value;
                    }
                    break;
                }
            }
        }
        rest = next;
    }
}

pub fn null_glyph_extents() -> (Rectangle, Rectangle) {
    let ink = Rectangle {
        x: SCALE,
        y: -(UNKNOWN_GLYPH_HEIGHT - 1) * SCALE,
        width: (UNKNOWN_GLYPH_WIDTH - 2) * SCALE,
        height: (UNKNOWN_GLYPH_HEIGHT - 2) * SCALE,
    };
    let logical = Rectangle {
        x: 0,
        y: -UNKNOWN_GLYPH_HEIGHT * SCALE,
        width: UNKNOWN_GLYPH_WIDTH * SCALE,
        height: UNKNOWN_GLYPH_HEIGHT * SCALE,
    };
    (ink, logical)
}

pub fn null_font_metrics() -> FontMetrics {
    FontMetrics {
        ascent: SCALE * UNKNOWN_GLYPH_HEIGHT,
        descent: 0,
        height: 0,
        approximate_char_width: SCALE * UNKNOWN_GLYPH_WIDTH,
        approximate_digit_width: SCALE * UNKNOWN_GLYPH_WIDTH,
        underline_position: -SCALE,
        underline_thickness: SCALE,
        strikethrough_position: SCALE * UNKNOWN_GLYPH_HEIGHT / 2,
        strikethrough_thickness: SCALE,
    }
}

pub fn glyph_extents_of(font: Option<&Font>, glyph: Glyph) -> (Rectangle, Rectangle) {
    match font {
        Some(font) => font.glyph_extents(glyph),
        None => null_glyph_extents(),
    }
}

pub fn metrics_of(font: Option<&Font>, language: Option<Language>) -> FontMetrics {
    match font {
        Some(font) => font.metrics(language),
        None => null_font_metrics(),
    }
}

pub fn scale_factors_of(font: Option<&Font>) -> (f64, f64) {
    match font {
        Some(font) => font.scale_factors(),
        None => (0.1, 0.1),
    }
}

fn hint(value: f64, scale: f64, scale_inv: f64) -> f64 {
    ((value - 1e-5) * scale).ceil() * scale_inv
}

fn cairo_to_matrix(m: &CairoMatrix) -> Matrix {
    Matrix {
        xx: m.xx,
        xy: m.xy,
        yx: m.yx,
        yy: m.yy,
        x0: m.x0,
        y0: m.y0,
    }
}

impl Font {
    pub(crate) fn new(fontmap: Weak<FontMapShared>, key: FontKey) -> Font {
        let pattern = &key.pattern;
        let description = font_description_from_pattern(pattern, true);
        let is_transformed = pattern
            .get_matrix(FC_MATRIX, 0)
            .is_some_and(|m| m.xx != 1.0 || m.xy != 0.0 || m.yx != 0.0 || m.yy != 1.0);
        let size = pattern_size(pattern) / key.matrix.font_scale_factor();
        let fc_matrix = pattern.matrix_product();
        let mut font_matrix = CairoMatrix::new(
            fc_matrix.xx,
            -fc_matrix.yx,
            -fc_matrix.xy,
            fc_matrix.yy,
            0.,
            0.,
        );
        font_matrix.scale(size, size);
        let options = key.context_key.clone().unwrap_or_default();
        let gravity = pattern_gravity(pattern);
        let gravity_matrix = CairoMatrix::rotation(gravity_to_rotation(gravity));
        let font_matrix = CairoMatrix::multiply(&font_matrix, &gravity_matrix);
        let ctm = CairoMatrix::new(
            key.matrix.xx,
            key.matrix.yx,
            key.matrix.xy,
            key.matrix.yy,
            0.,
            0.,
        );
        let is_hinted = !options.hint_metrics_off();
        Font(Arc::new(FontInner {
            fontmap,
            description,
            is_hinted,
            is_transformed,
            gravity,
            font_matrix,
            ctm,
            options,
            key,
            face_data: OnceLock::new(),
            scaled_font: OnceLock::new(),
            hb_font: OnceLock::new(),
            hex_box_info: OnceLock::new(),
            hex_box_scaled_font: Mutex::new(None),
            hex_box_glyphs: Mutex::new(HexBoxGlyphs::default()),
            glyph_cache: Mutex::new(GlyphExtentsCache {
                font_extents: Rectangle::default(),
                entries: None,
            }),
            metrics_by_lang: Mutex::new(Vec::new()),
        }))
    }

    pub(crate) fn from_inner(inner: Arc<FontInner>) -> Font {
        Font(inner)
    }

    pub(crate) fn downgrade(&self) -> Weak<FontInner> {
        Arc::downgrade(&self.0)
    }

    pub fn id(&self) -> usize {
        Arc::as_ptr(&self.0) as usize
    }

    pub fn ptr_eq(&self, other: &Font) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    pub fn font_map(&self) -> Option<FontMap> {
        self.0.fontmap.upgrade().map(FontMap::from_shared)
    }

    pub(crate) fn pattern(&self) -> &Pattern {
        &self.0.key.pattern
    }

    pub fn describe(&self) -> FontDescription {
        self.0.description.clone()
    }

    pub fn describe_with_absolute_size(&self) -> FontDescription {
        let mut desc = self.0.description.clone();
        if let Some(size) = self.pattern().get_double(FC_PIXEL_SIZE, 0) {
            desc.set_absolute_size(size * f64::from(SCALE));
        }
        desc
    }

    pub fn absolute_size(&self) -> i32 {
        self.pattern()
            .get_double(FC_PIXEL_SIZE, 0)
            .map_or(0, |size| c_double_to_int(size * f64::from(SCALE)))
    }

    pub fn variant(&self) -> Variant {
        self.0.description.variant()
    }

    pub fn gravity(&self) -> Gravity {
        self.0.gravity
    }

    pub fn is_hinted(&self) -> bool {
        self.0.is_hinted
    }

    pub fn is_transformed(&self) -> bool {
        self.0.is_transformed
    }

    pub fn scale_factors(&self) -> (f64, f64) {
        matrix_get_font_scale_factors(Some(&self.0.key.matrix))
    }

    pub fn matrix(&self) -> Matrix {
        let fc_matrix = self.pattern().matrix_product();
        Matrix {
            xx: fc_matrix.xx,
            xy: -fc_matrix.xy,
            yx: -fc_matrix.yx,
            yy: fc_matrix.yy,
            x0: 0.,
            y0: 0.,
        }
    }

    pub fn ctm(&self) -> Matrix {
        self.0.key.matrix
    }

    pub fn font_options(&self) -> Option<&FontOptions> {
        self.0.key.context_key.as_ref()
    }

    pub fn variations(&self) -> Option<&str> {
        self.0.key.variations.as_deref()
    }

    pub fn file(&self) -> Option<String> {
        self.pattern()
            .get_string(crate::ffi::fontconfig::FC_FILE, 0)
    }

    fn face_data(&self) -> Option<&Arc<FaceData>> {
        self.0
            .face_data
            .get_or_init(|| self.font_map()?.face_data(self.pattern()))
            .as_ref()
    }

    pub fn coverage(&self, _language: Option<Language>) -> FontCoverage {
        match self.face_data() {
            Some(data) if self.0.fontmap.strong_count() > 0 => data.coverage(self.pattern()),
            _ => FontCoverage::new(Coverage::new()),
        }
    }

    pub fn has_char(&self, wc: u32) -> bool {
        self.coverage(Some(Language::default_language())).get(wc) != CoverageLevel::None
    }

    pub fn languages(&self) -> Option<Vec<Language>> {
        self.font_map()?;
        self.face_data()?.languages(self.pattern())
    }

    pub fn features(&self, features: &mut Vec<Feature>, len: usize) {
        while features.len() < len {
            let Some(text) = self
                .pattern()
                .get_cstr(FC_FONT_FEATURES, features.len() as i32)
            else {
                break;
            };
            let Some(mut feature) = harfbuzz::feature_from_string(text.to_bytes()) else {
                break;
            };
            feature.start = 0;
            feature.end = u32::MAX;
            features.push(feature);
        }
    }

    fn hb_face(&self) -> HbFace {
        if let Some(data) = self.face_data() {
            return data.hb_face();
        }
        let file = self
            .pattern()
            .get_cstr(crate::ffi::fontconfig::FC_FILE, 0)
            .map(ToOwned::to_owned)
            .unwrap_or_default();
        let index = self.pattern().get_integer(FC_INDEX, 0).unwrap_or(0);
        HbFace::from_file(&file, index)
    }

    pub fn hb_font(&self) -> &HbFont {
        self.0.hb_font.get_or_init(|| self.create_hb_font())
    }

    #[cfg(feature = "rust-shaper")]
    pub(crate) fn face_index(&self) -> i32 {
        self.pattern().get_integer(FC_INDEX, 0).unwrap_or(0)
    }

    #[cfg(feature = "rust-shaper")]
    pub(crate) fn pattern_variations(&self) -> Option<String> {
        self.pattern().get_string(FC_FONT_VARIATIONS, 0)
    }

    pub(crate) fn hb_font_setup(&self) -> HbFontSetup {
        let key = &self.0.key;
        let pattern = &key.pattern;
        let embolden = if pattern.get_bool(FC_EMBOLDEN, 0) == Some(true) {
            1.0 / 24.0
        } else {
            0.0
        };
        let (mut x_scale_inv, mut y_scale_inv) = matrix_get_font_scale_factors(Some(&key.matrix));
        let fc_matrix = pattern.matrix_product();
        let font_matrix = Matrix {
            xx: fc_matrix.xx,
            yx: -fc_matrix.yx,
            xy: fc_matrix.xy,
            yy: -fc_matrix.yy,
            x0: 0.,
            y0: 0.,
        };
        let (x, y) = font_matrix.font_scale_factors();
        let slant = font_matrix.slant_ratio();
        x_scale_inv /= x;
        y_scale_inv /= y;
        if pattern_gravity(pattern).is_improper() {
            x_scale_inv = -x_scale_inv;
            y_scale_inv = -y_scale_inv;
        }
        let point_size = pattern.get_double(FC_SIZE, 0).unwrap_or(13.);
        let pixel_size = pattern.get_double(FC_PIXEL_SIZE, 0).unwrap_or_else(|| {
            let dpi = pattern.get_double(FC_DPI, 0).unwrap_or(72.);
            point_size * dpi / 72.
        });
        let x_scale = 1. / x_scale_inv;
        let y_scale = 1. / y_scale_inv;
        HbFontSetup {
            x_scale: c_double_to_int(pixel_size * f64::from(SCALE) * x_scale),
            y_scale: c_double_to_int(pixel_size * f64::from(SCALE) * y_scale),
            point_size: point_size as f32,
            slant: slant as f32,
            embolden: embolden as f32,
        }
    }

    fn create_hb_font(&self) -> HbFont {
        let key = &self.0.key;
        let pattern = &key.pattern;
        let setup = self.hb_font_setup();
        let point_size = setup.point_size;

        let hb_face = self.hb_face();
        let mut hb_font = HbFontBuilder::new(&hb_face);
        hb_font.set_scale(setup.x_scale, setup.y_scale);
        hb_font.set_ptem(setup.point_size);
        hb_font.set_synthetic_slant(setup.slant);
        hb_font.set_synthetic_bold(setup.embolden, setup.embolden, false);

        let n_axes = hb_face.axis_count();
        if n_axes != 0 {
            let (axes, mut n_axes) = hb_face.axis_infos(n_axes);
            let mut coords = vec![0f32; axes.len()];
            for axis in axes.iter().take(n_axes as usize) {
                let slot = axis.axis_index as usize;
                if let Some(coord) = coords.get_mut(slot) {
                    *coord = axis.default_value;
                    if axis.tag == harfbuzz::tag(b"opsz") {
                        *coord = point_size;
                    }
                }
            }
            if let Some(index) = pattern.get_integer(FC_INDEX, 0)
                && index != 0
            {
                let instance = ((index >> 16) as u32).wrapping_sub(1);
                let len = (n_axes as usize).min(coords.len());
                n_axes = hb_face.named_instance_design_coords(instance, &mut coords[..len]);
            }
            let axis_tags: Vec<(u32, u32)> = axes
                .iter()
                .take(n_axes as usize)
                .map(|axis| (axis.tag, axis.axis_index))
                .collect();
            if let Some(variations) = pattern.get_string(FC_FONT_VARIATIONS, 0) {
                parse_variations(&variations, &axis_tags, &mut coords);
            }
            if let Some(variations) = key.variations.as_deref() {
                parse_variations(variations, &axis_tags, &mut coords);
            }
            hb_font.set_var_coords_design(&coords, n_axes);
        }
        hb_font.finish()
    }

    pub fn get_glyph(&self, wc: u32) -> Glyph {
        let mut glyph = get_unknown_glyph(wc);
        self.hb_font().nominal_glyph_into(wc, &mut glyph);
        glyph
    }

    pub fn scaled_font(&self) -> Option<&ScaledFont> {
        self.0
            .scaled_font
            .get_or_init(|| {
                ScaledFont::for_pattern(
                    self.pattern(),
                    &self.0.font_matrix,
                    &self.0.ctm,
                    &self.0.options,
                )
            })
            .as_ref()
    }

    fn ok_scaled_font(&self) -> Option<&ScaledFont> {
        self.scaled_font().filter(|font| font.is_ok())
    }

    pub fn install(&self, cr: &CairoContext) -> bool {
        match self.ok_scaled_font() {
            Some(font) => {
                cr.set_scaled_font(font);
                true
            }
            None => false,
        }
    }

    fn face_metrics(&self) -> FontMetrics {
        let hb_font = self.hb_font();
        let extents = hb_font.extents_ltr();
        let mut metrics = FontMetrics::new();
        let transform = self
            .pattern()
            .get_matrix(FC_MATRIX, 0)
            .filter(|m| m.xx != 1.0 || m.xy != 0.0 || m.yx != 0.0 || m.yy != 1.0);
        match transform {
            Some(FcMatrix { yy, .. }) => {
                metrics.descent = c_double_to_int(f64::from(-extents.descender) * yy);
                metrics.ascent = c_double_to_int(f64::from(extents.ascender) * yy);
                metrics.height = c_double_to_int(
                    f64::from(extents.ascender - extents.descender + extents.line_gap) * yy,
                );
            }
            None => {
                metrics.descent = -extents.descender;
                metrics.ascent = extents.ascender;
                metrics.height = extents.ascender - extents.descender + extents.line_gap;
            }
        }
        let position = |tag| hb_font.ot_metrics_position(tag).filter(|&p| p != 0);
        metrics.underline_thickness = position(METRICS_UNDERLINE_SIZE).unwrap_or(SCALE);
        metrics.underline_position = position(METRICS_UNDERLINE_OFFSET).unwrap_or(-SCALE);
        metrics.strikethrough_thickness = position(METRICS_STRIKEOUT_SIZE).unwrap_or(SCALE);
        metrics.strikethrough_position =
            position(METRICS_STRIKEOUT_OFFSET).unwrap_or(metrics.ascent / 2);
        metrics
    }

    pub fn base_metrics(&self) -> FontMetrics {
        let mut metrics = self.face_metrics();
        if self.0.is_hinted {
            let ceil = |value: i32| pixels_ceil(value) * SCALE;
            metrics.ascent = ceil(metrics.ascent);
            metrics.descent = ceil(metrics.descent);
            metrics.height = ceil(metrics.height);
            metrics.underline_position = ceil(metrics.underline_position);
            metrics.underline_thickness = ceil(metrics.underline_thickness);
            metrics.strikethrough_position = ceil(metrics.strikethrough_position);
            metrics.strikethrough_thickness = ceil(metrics.strikethrough_thickness);
        }
        metrics
    }

    pub fn metrics(&self, language: Option<Language>) -> FontMetrics {
        let sample_str = language_get_sample_string(language);
        let this_thread = std::thread::current().id();
        let mut cache = true;
        if let Some(info) = lock(&self.0.metrics_by_lang)
            .iter()
            .find(|info| core::ptr::eq(info.sample_str, sample_str))
        {
            match info.computing {
                Some(thread) if thread != this_thread => cache = false,
                _ => return info.metrics,
            }
        }
        let Some(fontmap) = self.font_map() else {
            return FontMetrics::new();
        };
        let scaled_options = self.scaled_font().map(ScaledFont::font_options);
        let mut context = fontmap.create_context();
        context.set_language(language);
        context.set_font_options(Some(&scaled_options.unwrap_or_default()));
        let mut metrics = self.base_metrics();
        if cache {
            let mut list = lock(&self.0.metrics_by_lang);
            match list
                .iter()
                .find(|info| core::ptr::eq(info.sample_str, sample_str))
            {
                Some(info) if info.computing.is_none() => return info.metrics,
                Some(_) => cache = false,
                None => list.insert(
                    0,
                    MetricsInfo {
                        sample_str,
                        metrics,
                        computing: Some(this_thread),
                    },
                ),
            }
        }

        if !IN_GET_METRICS.with(Cell::get)
            && let Some(hooks) = layout_hooks()
        {
            IN_GET_METRICS.with(|flag| flag.set(true));
            let desc = self.describe_with_absolute_size();
            let widths = (hooks.measure_font_sample)(&mut context, &desc, sample_str);
            let sample_str_width = utf8_strwidth(sample_str);
            if sample_str_width > 0 {
                metrics.approximate_char_width =
                    (i64::from(widths.sample_width) / sample_str_width) as i32;
            }
            metrics.approximate_digit_width = widths.digits_max_glyph_width;
            IN_GET_METRICS.with(|flag| flag.set(false));
        }

        let height = metrics.ascent + metrics.descent;
        match self.0.gravity {
            Gravity::North => metrics.ascent = metrics.descent,
            Gravity::East | Gravity::West => {
                let mut ascent = height / 2;
                if self.0.is_hinted {
                    ascent = units_round(ascent);
                }
                metrics.ascent = ascent;
            }
            Gravity::South | Gravity::Auto => {}
        }
        let shift = (height - metrics.ascent) - metrics.descent;
        metrics.descent += shift;
        metrics.underline_position -= shift;
        metrics.strikethrough_position -= shift;
        metrics.ascent = height - metrics.descent;

        if cache
            && let Some(info) = lock(&self.0.metrics_by_lang)
                .iter_mut()
                .find(|info| core::ptr::eq(info.sample_str, sample_str))
        {
            info.metrics = metrics;
            info.computing = None;
        }
        metrics
    }

    fn with_glyph_cache<R>(&self, f: impl FnOnce(&mut GlyphExtentsCache) -> R) -> R {
        {
            let mut cache = lock(&self.0.glyph_cache);
            if cache.entries.is_some() {
                return f(&mut cache);
            }
        }
        let font_extents = self.compute_font_extents();
        let mut cache = lock(&self.0.glyph_cache);
        if cache.entries.is_none() {
            cache.font_extents = font_extents;
            let mut entries = Box::new([GlyphExtentsEntry::default(); GLYPH_CACHE_NUM_ENTRIES]);
            entries[0].glyph = 1;
            cache.entries = Some(entries);
        }
        f(&mut cache)
    }

    fn compute_font_extents(&self) -> Rectangle {
        let extents = self.hb_font().h_extents();
        let is_hinted = self.0.is_hinted;
        let mut font_extents = Rectangle {
            x: 0,
            width: 0,
            height: extents.ascender - extents.descender,
            y: 0,
        };
        font_extents.y = match self.0.gravity {
            Gravity::North => extents.descender,
            Gravity::East | Gravity::West => {
                let mut ascent = font_extents.height / 2;
                if is_hinted {
                    ascent = units_round(ascent);
                }
                -ascent
            }
            Gravity::South | Gravity::Auto => -extents.ascender,
        };
        if is_hinted {
            font_extents.y = if font_extents.y < 0 {
                units_floor(font_extents.y)
            } else {
                units_ceil(font_extents.y)
            };
            font_extents.height = if font_extents.height < 0 {
                units_floor(extents.ascender) - units_ceil(extents.descender)
            } else {
                units_ceil(extents.ascender) - units_floor(extents.descender)
            };
        }
        if self.0.gravity.is_improper() {
            font_extents.y = -font_extents.y;
            font_extents.height = -font_extents.height;
        }
        font_extents
    }

    fn compute_glyph_extents(&self, glyph: Glyph) -> GlyphExtentsEntry {
        let extents = self
            .scaled_font()
            .map(|font| font.glyph_extents(glyph))
            .unwrap_or_default();
        let width = if self.0.gravity.is_vertical() {
            units_from_double(extents.y_advance)
        } else {
            units_from_double(extents.x_advance)
        };
        GlyphExtentsEntry {
            glyph,
            width,
            ink_rect: Rectangle {
                x: units_from_double(extents.x_bearing),
                y: units_from_double(extents.y_bearing),
                width: units_from_double(extents.width),
                height: units_from_double(extents.height),
            },
        }
    }

    pub fn glyph_extents(&self, glyph: Glyph) -> (Rectangle, Rectangle) {
        enum Lookup {
            Done(Rectangle, Rectangle),
            Missing,
        }
        let lookup = self.with_glyph_cache(|cache| {
            let font_extents = cache.font_extents;
            if glyph == GLYPH_EMPTY {
                return Lookup::Done(Rectangle::default(), font_extents);
            }
            if glyph & GLYPH_UNKNOWN_FLAG != 0 {
                return Lookup::Missing;
            }
            let index = (glyph & GLYPH_CACHE_MASK) as usize;
            let entries = cache.entries.as_mut().expect("glyph cache initialized");
            if entries[index].glyph != glyph {
                entries[index] = self.compute_glyph_extents(glyph);
            }
            let entry = entries[index];
            let mut logical = font_extents;
            match self.0.gravity {
                Gravity::East => {
                    logical.width = font_extents.height;
                    logical.x = -logical.width;
                }
                Gravity::West => {
                    logical.width = -font_extents.height;
                    logical.x = -logical.width;
                }
                Gravity::South | Gravity::North | Gravity::Auto => logical.width = entry.width,
            }
            Lookup::Done(entry.ink_rect, logical)
        });
        match lookup {
            Lookup::Done(ink, logical) => (ink, logical),
            Lookup::Missing => self.glyph_extents_missing(glyph),
        }
    }

    fn space_extents(&self) -> (Rectangle, Rectangle) {
        let width = self.absolute_size() / 4;
        let ink = Rectangle {
            x: 0,
            y: 0,
            width,
            height: 0,
        };
        let mut logical = lock(&self.0.glyph_cache).font_extents;
        logical.width = width;
        (ink, logical)
    }

    fn glyph_extents_missing(&self, glyph: Glyph) -> (Rectangle, Rectangle) {
        let ch = glyph & !GLYPH_UNKNOWN_FLAG;
        if ch == 0x20 || ch == 0x2423 {
            return self.space_extents();
        }
        let Some(hbi) = self.hex_box_info() else {
            return null_glyph_extents();
        };
        let cols = if glyph == GLYPH_INVALID_INPUT || ch > 0x10FFFF {
            1
        } else if let Some((_, _, cols)) = get_ignorable_size(ch) {
            cols
        } else {
            (if ch > 0xffff { 6 } else { 4 }) / hbi.rows
        };
        let scale = f64::from(SCALE);
        let cols = f64::from(cols);
        let ink = Rectangle {
            x: c_double_to_int(scale * hbi.pad_x),
            y: c_double_to_int(scale * (hbi.box_descent - hbi.box_height)),
            width: c_double_to_int(scale * (4. * hbi.pad_x + cols * (hbi.digit_width + hbi.pad_x))),
            height: c_double_to_int(scale * hbi.box_height),
        };
        let logical = Rectangle {
            x: 0,
            y: c_double_to_int(scale * (hbi.box_descent - (hbi.box_height + hbi.pad_y))),
            width: c_double_to_int(scale * (5. * hbi.pad_x + cols * (hbi.digit_width + hbi.pad_x))),
            height: c_double_to_int(scale * (hbi.box_height + 2. * hbi.pad_y)),
        };
        (ink, logical)
    }

    pub fn hex_box_info(&self) -> Option<&HexBoxInfo> {
        if let Some(info) = self.0.hex_box_info.get() {
            return info.as_ref();
        }
        let info = self.compute_hex_box_info()?;
        let _ = self.0.hex_box_info.set(Some(info));
        self.0.hex_box_info.get().and_then(Option::as_ref)
    }

    fn compute_hex_box_info(&self) -> Option<HexBoxInfo> {
        let scaled_font = self.ok_scaled_font()?;
        let is_hinted = self.0.is_hinted;
        let mut desc = self.describe_with_absolute_size();
        let cairo_ctm = scaled_font.ctm();
        let cairo_font_matrix = scaled_font.font_matrix();
        let font_options = scaled_font.font_options();
        let pango_ctm = cairo_to_matrix(&cairo_ctm);
        let pango_font_matrix = cairo_to_matrix(&cairo_font_matrix);
        let size = pango_font_matrix.font_scale_factor() / pango_ctm.font_scale_factor();
        let (mut scale_x, mut scale_x_inv, mut scale_y, mut scale_y_inv) = (1., 1., 1., 1.);
        if is_hinted {
            let (mut x, mut y) = (1., 0.);
            cairo_ctm.transform_distance(&mut x, &mut y);
            scale_x = (x * x + y * y).sqrt();
            scale_x_inv = 1. / scale_x;
            let (mut x, mut y) = (0., 1.);
            cairo_ctm.transform_distance(&mut x, &mut y);
            scale_y = (x * x + y * y).sqrt();
            scale_y_inv = 1. / scale_y;
        }
        let hint_x = |value: f64| hint(value, scale_x, scale_x_inv);
        let hint_y = |value: f64| hint(value, scale_y, scale_y_inv);

        let fontmap = self.font_map()?;
        desc.unset_fields(font_mask::GRAVITY);
        desc.set_family(Some("monospace"));
        let rows = 2;
        let mut mini_size = size / 2.2;
        if is_hinted {
            mini_size = hint_y(mini_size);
        }
        desc.set_absolute_size(f64::from(units_from_double(mini_size)));
        let mut context = fontmap.create_context();
        context.set_matrix(Some(&pango_ctm));
        context.set_language(script_get_sample_language(Script::LATIN));
        context.set_font_options(Some(&font_options));
        let mini_font = fontmap.load_font(Some(&context), &desc)?;
        drop(context);
        let scaled_mini_font = mini_font.ok_scaled_font()?.clone();

        let mut width: f64 = 0.;
        let mut height: f64 = 0.;
        for digit in b"0123456789ABCDEF" {
            let text = CString::new(vec![*digit]).expect("hex digit has no NUL");
            let extents = scaled_mini_font.text_extents(&text);
            width = if width > extents.width {
                width
            } else {
                extents.width
            };
            height = if height > extents.height {
                height
            } else {
                extents.height
            };
        }
        let mut font_extents = scaled_font.extents();
        if font_extents.ascent + font_extents.descent <= 0. {
            font_extents.ascent = f64::from(UNKNOWN_GLYPH_HEIGHT);
            font_extents.descent = 0.;
        }
        let mut pad = (font_extents.ascent + font_extents.descent) / 43.;
        pad = if pad < mini_size { pad } else { mini_size };
        let mut hbi = HexBoxInfo {
            font: mini_font,
            rows,
            digit_width: width,
            digit_height: height,
            pad_x: pad,
            pad_y: pad,
            line_width: 0.,
            box_descent: 0.,
            box_height: 0.,
        };
        if is_hinted {
            hbi.digit_width = hint_x(hbi.digit_width);
            hbi.digit_height = hint_y(hbi.digit_height);
            hbi.pad_x = hint_x(hbi.pad_x);
            hbi.pad_y = hint_y(hbi.pad_y);
        }
        hbi.line_width = if hbi.pad_x < hbi.pad_y {
            hbi.pad_x
        } else {
            hbi.pad_y
        };
        hbi.box_height = 3. * hbi.pad_y + f64::from(rows) * (hbi.pad_y + hbi.digit_height);
        if rows == 1 || hbi.box_height <= font_extents.ascent {
            hbi.box_descent = 2. * hbi.pad_y;
        } else if hbi.box_height <= font_extents.ascent + font_extents.descent - 2. * hbi.pad_y {
            hbi.box_descent = 2. * hbi.pad_y + hbi.box_height - font_extents.ascent;
        } else {
            hbi.box_descent = font_extents.descent * hbi.box_height
                / (font_extents.ascent + font_extents.descent);
        }
        if is_hinted {
            hbi.box_descent = hint_y(hbi.box_descent);
        }
        Some(hbi)
    }

    pub fn encode_hex_box_glyph(&self, glyph: Glyph) -> u64 {
        if glyph & GLYPH_UNKNOWN_FLAG == 0 {
            return u64::from(glyph);
        }
        let mut glyphs = lock(&self.0.hex_box_glyphs);
        if glyphs.base == 0 {
            glyphs.base = 1;
        }
        if let Some(&value) = glyphs.map.get(&glyph) {
            return u64::from(value);
        }
        let next = glyphs.base + glyphs.glyphs.len() as u32;
        if next > 0xFFFF {
            return u64::from(glyph);
        }
        glyphs.map.insert(glyph, next);
        glyphs.glyphs.push(glyph);
        u64::from(next)
    }

    fn decode_hex_box_glyph(&self, glyph: u64) -> Glyph {
        let glyphs = lock(&self.0.hex_box_glyphs);
        let base = u64::from(glyphs.base);
        if !glyphs.glyphs.is_empty() && glyph >= base && glyph < base + glyphs.glyphs.len() as u64 {
            return glyphs.glyphs[(glyph - base) as usize];
        }
        glyph as Glyph
    }

    pub fn hex_box_scaled_font(&self) -> Option<ScaledFont> {
        let mut slot = lock(&self.0.hex_box_scaled_font);
        if let Some(font) = slot.as_ref() {
            return Some(font.clone());
        }
        let native = self.ok_scaled_font()?;
        let ctm = native.ctm();
        let font_matrix = native.font_matrix();
        let options = native.font_options();
        let handler = Box::new(HexBoxRenderer {
            font: self.downgrade(),
        });
        let font = ScaledFont::user_font(handler, &font_matrix, &ctm, &options)?;
        *slot = Some(font.clone());
        Some(font)
    }

    fn render_missing_glyph(&self, glyph: Glyph, cr: &CairoContext) {
        let ch = glyph & !GLYPH_UNKNOWN_FLAG;
        let invalid_input = glyph == GLYPH_INVALID_INPUT || ch > 0x10FFFF;
        let (_, logical_rect) = self.glyph_extents_missing(glyph);
        let width = f64::from(logical_rect.width) / f64::from(SCALE);
        let hbi = match self.hex_box_info() {
            Some(hbi) if hbi.font.install(cr) => hbi,
            _ => {
                draw_box_glyph(cr, width, invalid_input);
                return;
            }
        };
        let rows;
        let cols;
        let name: String;
        if invalid_input {
            rows = hbi.rows;
            cols = 1;
            name = String::new();
        } else if ch == 0x2423 || unichar_type(ch) == UnicodeType::SpaceSeparator {
            let x = 0.5 * width;
            let y = hbi.box_descent - 0.5 * hbi.box_height;
            cr.new_sub_path();
            cr.arc(x, y, 1.5 * hbi.line_width, 0., 2. * core::f64::consts::PI);
            cr.close_path();
            cr.fill();
            return;
        } else if ch == u32::from('\t') {
            let y = hbi.box_descent - 0.5 * hbi.box_height;
            let offset = 0.2 * width;
            let x = offset;
            let al = width - 2. * offset;
            let tl = if hbi.digit_width < 0.75 * al {
                hbi.digit_width
            } else {
                0.75 * al
            };
            let tw2 = 2.5 * hbi.line_width;
            let lw2 = 0.5 * hbi.line_width;
            cr.move_to(x - lw2, y - tw2);
            cr.line_to(x + lw2, y - tw2);
            cr.line_to(x + lw2, y - lw2);
            cr.line_to(x + al - tl, y - lw2);
            cr.line_to(x + al - tl, y - tw2);
            cr.line_to(x + al, y);
            cr.line_to(x + al - tl, y + tw2);
            cr.line_to(x + al - tl, y + lw2);
            cr.line_to(x + lw2, y + lw2);
            cr.line_to(x + lw2, y + tw2);
            cr.line_to(x - lw2, y + tw2);
            cr.close_path();
            cr.fill();
            return;
        } else if ch == u32::from('\n') || ch == 0x2028 || ch == 0x2029 {
            let offset = 0.2 * width;
            let al = width - 2. * offset;
            let tl = if hbi.digit_width < 0.75 * al {
                hbi.digit_width
            } else {
                0.75 * al
            };
            let ah = al - 0.5 * tl;
            let tw2 = 2.5 * hbi.line_width;
            let x = offset;
            let y = -(hbi.box_height - al) / 2.;
            let lw2 = 0.5 * hbi.line_width;
            cr.move_to(x, y);
            cr.line_to(x + tl, y - tw2);
            cr.line_to(x + tl, y - lw2);
            cr.line_to(x + al - lw2, y - lw2);
            cr.line_to(x + al - lw2, y - ah);
            cr.line_to(x + al + lw2, y - ah);
            cr.line_to(x + al + lw2, y + lw2);
            cr.line_to(x + tl, y + lw2);
            cr.line_to(x + tl, y + tw2);
            cr.close_path();
            cr.fill();
            return;
        } else if let Some((nick, nick_rows, nick_cols)) = get_ignorable_size(ch) {
            rows = nick_rows;
            cols = nick_cols;
            name = nick.to_owned();
        } else {
            rows = hbi.rows;
            cols = (if ch > 0xffff { 6 } else { 4 }) / rows;
            name = if ch > 0xffff {
                format!("{ch:06X}")
            } else {
                format!("{ch:04X}")
            };
        }
        let box_width = 3. * hbi.pad_x + f64::from(cols) * (hbi.digit_width + hbi.pad_x);
        let mut lsb = (f64::from(logical_rect.width) / f64::from(SCALE) - box_width) * 0.5;
        lsb = (lsb / hbi.pad_x).floor() * hbi.pad_x;
        draw_frame(
            cr,
            lsb + 1.5 * hbi.pad_x,
            hbi.box_descent - hbi.box_height + hbi.pad_y * 0.5,
            box_width - hbi.pad_x,
            hbi.box_height - hbi.pad_y,
            hbi.line_width,
            invalid_input,
        );
        if invalid_input {
            return;
        }
        let x0 = lsb + hbi.pad_x * 3.;
        let y0 =
            hbi.box_descent - hbi.pad_y * 2. - (f64::from(hbi.rows - rows) * hbi.digit_height / 2.);
        let mut chars = name.bytes();
        for row in 0..rows {
            let y = y0 - f64::from(rows - 1 - row) * (hbi.digit_height + hbi.pad_y);
            for col in 0..cols {
                let x = x0 + f64::from(col) * (hbi.digit_width + hbi.pad_x);
                cr.move_to(x, y);
                let byte = chars.next().unwrap_or(0);
                let text = CString::new(if byte == 0 { Vec::new() } else { vec![byte] })
                    .expect("hex box text has no NUL");
                cr.text_path(&text);
                cr.fill();
            }
        }
    }
}

fn draw_frame(
    cr: &CairoContext,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    line_width: f64,
    invalid: bool,
) {
    let half_line_width = 0.5 * line_width;
    cr.set_fill_rule_even_odd();
    cr.rectangle(
        x - half_line_width,
        y - half_line_width,
        width + line_width,
        height + line_width,
    );
    let inner_width = if width - line_width > 0. {
        width - line_width
    } else {
        0.
    };
    let inner_height = if height - line_width > 0. {
        height - line_width
    } else {
        0.
    };
    cr.rectangle(
        x + half_line_width,
        y + half_line_width,
        inner_width,
        inner_height,
    );
    if invalid {
        let diagonal_length = width.hypot(height);
        if diagonal_length > 0. {
            let dx = half_line_width * height / diagonal_length;
            let dy = half_line_width * width / diagonal_length;
            cr.new_sub_path();
            cr.move_to(x - dx, y + dy);
            cr.line_to(x + dx, y - dy);
            cr.line_to(x + width + dx, y + height - dy);
            cr.line_to(x + width - dx, y + height + dy);
            cr.close_path();
            cr.new_sub_path();
            cr.move_to(x + width + dx, y + dy);
            cr.line_to(x + width - dx, y - dy);
            cr.line_to(x - dx, y + height - dy);
            cr.line_to(x + dx, y + height + dy);
            cr.close_path();
        }
    }
    cr.fill();
}

fn draw_box_glyph(cr: &CairoContext, width: f64, invalid: bool) {
    draw_frame(
        cr,
        1.5,
        1.5 - f64::from(UNKNOWN_GLYPH_HEIGHT),
        width - 3.0,
        f64::from(UNKNOWN_GLYPH_HEIGHT) - 3.0,
        1.0,
        invalid,
    );
}

struct HexBoxRenderer {
    font: Weak<FontInner>,
}

impl crate::ffi::cairo::UserFontHandler for HexBoxRenderer {
    fn render_glyph(&self, glyph: u64, cr: &CairoContext, extents: &mut TextExtents) -> bool {
        let Some(font) = self.font.upgrade().map(Font) else {
            return false;
        };
        let pango_glyph = font.decode_hex_box_glyph(glyph);
        let mut x_advance;
        let mut y_advance = 0.;
        let native = if pango_glyph & GLYPH_UNKNOWN_FLAG != 0 {
            let (_, logical_rect) = font.glyph_extents_missing(pango_glyph);
            x_advance = f64::from(logical_rect.width) / f64::from(SCALE);
            None
        } else {
            let Some(native) = font.ok_scaled_font() else {
                return false;
            };
            *extents = native.glyph_extents(pango_glyph);
            x_advance = extents.x_advance;
            y_advance = extents.y_advance;
            Some(native.clone())
        };
        let mut matrix = cr.matrix();
        if matrix.invert() {
            matrix.transform_distance(&mut x_advance, &mut y_advance);
        }
        *extents = TextExtents {
            x_bearing: 0.,
            y_bearing: 0.,
            width: 0.,
            height: 0.,
            x_advance,
            y_advance,
        };
        cr.save();
        cr.identity_matrix();
        match native {
            None => font.render_missing_glyph(pango_glyph, cr),
            Some(native) => {
                cr.set_scaled_font(&native);
                cr.glyph_path(pango_glyph);
                cr.fill();
            }
        }
        cr.restore();
        true
    }

    fn unicode_to_glyph(&self, unicode: u64) -> Option<u64> {
        let font = self.font.upgrade().map(Font)?;
        let mut glyph: u32 = 0;
        let codepoint = unicode as u32;
        if font.hb_font().nominal_glyph_into(codepoint, &mut glyph) {
            return Some(u64::from(glyph));
        }
        if unicode == 0x20 {
            return Some(font.encode_hex_box_glyph(get_unknown_glyph(0x20)));
        }
        let pango_glyph = if unicode > 0x10FFFF {
            GLYPH_INVALID_INPUT
        } else {
            get_unknown_glyph(codepoint)
        };
        Some(font.encode_hex_box_glyph(pango_glyph))
    }
}
