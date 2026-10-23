//! Southstar text — the context text is laid out in: direction, gravity, language, base font, matrix, font map, cairo font options and cached metrics.
//! Copyright 2000 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::font::{CairoContext, Font, FontOptions, null_font_metrics, utf8_strwidth};
use crate::font_description::FontDescription;
use crate::fontmap::FontMap;
use crate::fontset::Fontset;
use crate::gravity::gravity_get_for_matrix;
use crate::language::{Language, language_get_sample_string};
use crate::matrix::Matrix;
use crate::metrics::FontMetrics;
use crate::types::{Direction, Gravity, GravityHint, SCALE, Stretch, Style, Variant, Weight};

const METRICS_CACHE_MAX_ENTRIES: usize = 512;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SampleWidths {
    pub sample_width: i32,
    pub digits_max_glyph_width: i32,
}

#[derive(Clone, Debug)]
pub struct SampleItem {
    pub font: Option<Font>,
    pub width: i32,
}

#[derive(Clone, Copy)]
pub struct LayoutHooks {
    pub measure_font_sample: fn(&mut Context, &FontDescription, &str) -> SampleWidths,
    pub shape_sample: fn(&mut Context, &FontDescription, &str) -> Vec<SampleItem>,
    pub font_map_changed: fn(),
}

static LAYOUT_HOOKS: OnceLock<LayoutHooks> = OnceLock::new();

pub fn set_layout_hooks(hooks: LayoutHooks) -> bool {
    LAYOUT_HOOKS.set(hooks).is_ok()
}

pub(crate) fn layout_hooks() -> Option<LayoutHooks> {
    LAYOUT_HOOKS.get().copied()
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct MetricsKey {
    desc: FontDescription,
    language: Language,
}

#[derive(Clone)]
struct MetricsEntry {
    metrics: FontMetrics,
    read: bool,
}

#[derive(Clone, Debug, Default)]
struct CairoInfo {
    dpi: f64,
    set_options_explicit: bool,
    set_options: Option<FontOptions>,
    surface_options: Option<FontOptions>,
}

#[derive(Clone)]
pub struct Context {
    serial: u32,
    fontmap_serial: u32,
    set_language: Option<Language>,
    language: Language,
    base_dir: Direction,
    base_gravity: Gravity,
    resolved_gravity: Gravity,
    gravity_hint: GravityHint,
    font_desc: FontDescription,
    matrix: Option<Matrix>,
    font_map: Option<FontMap>,
    metrics: Option<FontMetrics>,
    metrics_cache: HashMap<MetricsKey, MetricsEntry>,
    round_glyph_positions: bool,
    cairo: CairoInfo,
    merged_options: OnceCell<FontOptions>,
}

impl Default for Context {
    fn default() -> Context {
        Context::new()
    }
}

impl core::fmt::Debug for Context {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Context")
            .field("serial", &self.serial)
            .field("language", &self.language)
            .field("base_dir", &self.base_dir)
            .field("font_desc", &self.font_desc)
            .field("matrix", &self.matrix)
            .finish_non_exhaustive()
    }
}

impl Context {
    pub fn new() -> Context {
        let mut font_desc = FontDescription::new();
        font_desc.set_family(Some("serif"));
        font_desc.set_style(Style::Normal);
        font_desc.set_variant(Variant::Normal);
        font_desc.set_weight(Weight::NORMAL);
        font_desc.set_stretch(Stretch::Normal);
        font_desc.set_size(12 * SCALE);
        Context {
            serial: 1,
            fontmap_serial: 0,
            set_language: None,
            language: Language::default_language(),
            base_dir: Direction::WeakLtr,
            base_gravity: Gravity::South,
            resolved_gravity: Gravity::South,
            gravity_hint: GravityHint::Natural,
            font_desc,
            matrix: None,
            font_map: None,
            metrics: None,
            metrics_cache: HashMap::new(),
            round_glyph_positions: true,
            cairo: CairoInfo {
                dpi: -1.0,
                ..CairoInfo::default()
            },
            merged_options: OnceCell::new(),
        }
    }

    pub fn create_for_cairo(cr: &CairoContext) -> Context {
        let mut context = FontMap::default_for_thread().create_context();
        context.update_from_cairo(cr);
        context
    }

    fn context_changed(&mut self) {
        self.serial = self.serial.wrapping_add(1);
        if self.serial == 0 {
            self.serial = 1;
        }
        self.metrics = None;
        self.metrics_cache.clear();
    }

    pub fn changed(&mut self) {
        self.context_changed();
    }

    fn check_fontmap_changed(&mut self) {
        let Some(font_map) = self.font_map.as_ref() else {
            return;
        };
        let old_serial = self.fontmap_serial;
        self.fontmap_serial = font_map.serial();
        if old_serial != self.fontmap_serial {
            self.context_changed();
            if let Some(hooks) = layout_hooks() {
                (hooks.font_map_changed)();
            }
        }
    }

    pub fn serial(&mut self) -> u32 {
        self.check_fontmap_changed();
        self.serial
    }

    fn update_resolved_gravity(&mut self) {
        self.resolved_gravity = if self.base_gravity == Gravity::Auto {
            gravity_get_for_matrix(self.matrix.as_ref())
        } else {
            self.base_gravity
        };
    }

    pub fn set_matrix(&mut self, matrix: Option<&Matrix>) {
        if self.matrix.is_some() || matrix.is_some() {
            self.context_changed();
        }
        self.matrix = matrix.copied();
        self.update_resolved_gravity();
    }

    pub fn matrix(&self) -> Option<Matrix> {
        self.matrix
    }

    pub fn set_font_map(&mut self, font_map: Option<FontMap>) {
        let same = match (&font_map, &self.font_map) {
            (Some(a), Some(b)) => a.ptr_eq(b),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        self.context_changed();
        self.fontmap_serial = font_map.as_ref().map_or(0, FontMap::serial);
        self.font_map = font_map;
    }

    pub fn font_map(&self) -> Option<FontMap> {
        self.font_map.clone()
    }

    pub fn font_description(&self) -> &FontDescription {
        &self.font_desc
    }

    pub fn set_font_description(&mut self, desc: &FontDescription) {
        if !desc.equal(&self.font_desc) {
            self.context_changed();
        }
        self.font_desc = desc.clone();
    }

    pub fn set_language(&mut self, language: Option<Language>) {
        let effective = language.unwrap_or_else(Language::default_language);
        if language != Some(self.language) {
            self.context_changed();
        }
        self.set_language = language;
        self.language = effective;
    }

    pub fn language(&self) -> Option<Language> {
        self.set_language
    }

    pub fn effective_language(&self) -> Language {
        self.language
    }

    pub fn set_base_dir(&mut self, direction: Direction) {
        if direction != self.base_dir {
            self.context_changed();
        }
        self.base_dir = direction;
    }

    pub fn base_dir(&self) -> Direction {
        self.base_dir
    }

    pub fn set_base_gravity(&mut self, gravity: Gravity) {
        if gravity != self.base_gravity {
            self.context_changed();
        }
        self.base_gravity = gravity;
        self.update_resolved_gravity();
    }

    pub fn base_gravity(&self) -> Gravity {
        self.base_gravity
    }

    pub fn gravity(&self) -> Gravity {
        self.resolved_gravity
    }

    pub fn set_gravity_hint(&mut self, hint: GravityHint) {
        if hint != self.gravity_hint {
            self.context_changed();
        }
        self.gravity_hint = hint;
    }

    pub fn gravity_hint(&self) -> GravityHint {
        self.gravity_hint
    }

    pub fn set_round_glyph_positions(&mut self, round_positions: bool) {
        if self.round_glyph_positions != round_positions {
            self.round_glyph_positions = round_positions;
            self.context_changed();
        }
    }

    pub fn round_glyph_positions(&self) -> bool {
        self.round_glyph_positions
    }

    pub fn set_resolution(&mut self, dpi: f64) {
        self.cairo.dpi = dpi;
    }

    pub fn resolution(&self) -> f64 {
        self.cairo.dpi
    }

    pub fn set_font_options(&mut self, options: Option<&FontOptions>) {
        if self.cairo.set_options.is_none() && options.is_none() {
            return;
        }
        if let (Some(current), Some(options)) = (self.cairo.set_options.as_ref(), options)
            && current == options
        {
            return;
        }
        self.context_changed();
        match options {
            Some(options) => {
                self.cairo.set_options = Some(options.clone());
                self.cairo.set_options_explicit = true;
            }
            None => {
                self.cairo.set_options = None;
                self.cairo.set_options_explicit = false;
            }
        }
        self.merged_options = OnceCell::new();
    }

    pub fn font_options(&self) -> Option<&FontOptions> {
        self.cairo.set_options.as_ref()
    }

    pub fn merged_font_options(&self) -> FontOptions {
        self.merged_options
            .get_or_init(|| {
                let mut merged = FontOptions::new();
                if let Some(surface) = self.cairo.surface_options.as_ref() {
                    merged.merge(surface);
                }
                if let Some(set) = self.cairo.set_options.as_ref() {
                    merged.merge(set);
                }
                merged
            })
            .clone()
    }

    pub fn update_from_cairo(&mut self, cr: &CairoContext) {
        self.cairo.surface_options = Some(cr.target_font_options());
        if !self.cairo.set_options_explicit {
            self.cairo.set_options = Some(cr.font_options());
        }
        let old_merged = self.merged_options.take();
        let merged = self.merged_font_options();
        let mut changed = match old_merged {
            Some(old) => old != merged,
            None => true,
        };
        let cairo_matrix = cr.matrix();
        let pango_matrix = Matrix {
            xx: cairo_matrix.xx,
            yx: cairo_matrix.yx,
            xy: cairo_matrix.xy,
            yy: cairo_matrix.yy,
            x0: 0.,
            y0: 0.,
        };
        let current = self.matrix.unwrap_or(Matrix::IDENTITY);
        let bits = |m: &Matrix| {
            [
                m.xx.to_bits(),
                m.xy.to_bits(),
                m.yx.to_bits(),
                m.yy.to_bits(),
                m.x0.to_bits(),
                m.y0.to_bits(),
            ]
        };
        if !merged.hint_metrics_off() && bits(&pango_matrix) != bits(&current) {
            changed = true;
        }
        self.set_matrix(Some(&pango_matrix));
        if changed {
            self.context_changed();
        }
    }

    pub fn load_font(&self, desc: &FontDescription) -> Option<Font> {
        self.font_map.as_ref()?.load_font(Some(self), desc)
    }

    pub fn load_fontset(
        &self,
        desc: &FontDescription,
        language: Option<Language>,
    ) -> Option<Fontset> {
        self.font_map
            .as_ref()?
            .load_fontset(Some(self), desc, language)
    }

    fn metrics_cache_lookup(&mut self, key: &MetricsKey) -> Option<FontMetrics> {
        let entry = self.metrics_cache.get_mut(key)?;
        entry.read = true;
        Some(entry.metrics)
    }

    fn metrics_cache_insert(&mut self, key: MetricsKey, metrics: FontMetrics) {
        if self.metrics_cache.len() >= METRICS_CACHE_MAX_ENTRIES {
            self.metrics_cache.retain(|_, entry| {
                let keep = entry.read;
                entry.read = false;
                keep
            });
            if self.metrics_cache.len() >= METRICS_CACHE_MAX_ENTRIES / 2 {
                self.metrics_cache.clear();
            }
        }
        self.metrics_cache.insert(
            key,
            MetricsEntry {
                metrics,
                read: false,
            },
        );
    }

    pub fn metrics(
        &mut self,
        desc: Option<&FontDescription>,
        language: Option<Language>,
    ) -> FontMetrics {
        self.check_fontmap_changed();
        let is_own_desc = desc.is_none();
        let desc = desc.cloned().unwrap_or_else(|| self.font_desc.clone());
        let language = language.unwrap_or(self.language);
        let is_own = is_own_desc && language == self.language;
        if is_own && let Some(metrics) = self.metrics {
            return metrics;
        }
        let key = MetricsKey {
            desc: desc.clone(),
            language,
        };
        if let Some(metrics) = self.metrics_cache_lookup(&key) {
            return metrics;
        }
        let Some(font_map) = self.font_map.clone() else {
            return null_font_metrics();
        };
        let current_fonts = font_map.load_fontset(Some(self), &desc, Some(language));
        let mut metrics = FontMetrics::new();
        if let Some(fonts) = current_fonts.as_ref() {
            let fontset_language = fonts.language();
            fonts.foreach(|font| {
                metrics = font.metrics(fontset_language);
                true
            });
        }
        let sample_str = language_get_sample_string(Some(language));
        if let Some(hooks) = layout_hooks() {
            let items = (hooks.shape_sample)(self, &desc, sample_str);
            let mut fonts_seen: HashSet<usize> = HashSet::new();
            let mut approximate_char_width: i64 = 0;
            for item in &items {
                if let Some(font) = item.font.as_ref()
                    && fonts_seen.insert(font.id())
                {
                    let raw_metrics = font.metrics(Some(language));
                    metrics.ascent = metrics.ascent.max(raw_metrics.ascent);
                    metrics.descent = metrics.descent.max(raw_metrics.descent);
                    metrics.height = metrics.height.max(raw_metrics.height);
                }
                approximate_char_width += i64::from(item.width);
            }
            let text_width = utf8_strwidth(sample_str);
            if text_width > 0 {
                metrics.approximate_char_width =
                    (i64::from(approximate_char_width as i32) / text_width) as i32;
            }
        }
        drop(current_fonts);
        if is_own {
            self.metrics = Some(metrics);
        }
        self.metrics_cache_insert(key, metrics);
        metrics
    }
}
