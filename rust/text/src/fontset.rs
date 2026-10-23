//! Southstar text — fontsets: the fonts fontconfig sorted for one description, loaded lazily and searched for the font covering a character.
//! Copyright 2001 Red Hat Software, 2003 Keith Packard; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::collections::HashSet;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use crate::ffi::fontconfig::{FontSet, Pattern};
use crate::font::{Font, FontCoverage};
use crate::fontmap::{FontMap, FontMapShared, FontsetKey};
use crate::language::{Language, language_get_sample_string};
use crate::metrics::FontMetrics;
use crate::types::CoverageLevel;

pub(crate) struct Patterns {
    fontmap: Weak<FontMapShared>,
    pattern: Pattern,
    fonts: Arc<FontSet>,
    state: Mutex<PatternsState>,
}

#[derive(Default)]
struct PatternsState {
    matched: bool,
    matched_font: Option<Pattern>,
    sorted: Option<Option<FontSet>>,
}

impl Patterns {
    pub(crate) fn new(
        fontmap: Weak<FontMapShared>,
        pattern: Pattern,
        fonts: Arc<FontSet>,
    ) -> Patterns {
        Patterns {
            fontmap,
            pattern,
            fonts,
            state: Mutex::new(PatternsState::default()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, PatternsState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn sort_into(&self, state: &mut PatternsState) {
        if state.sorted.is_some() {
            return;
        }
        let fonts = self
            .fontmap
            .upgrade()
            .map(|shared| FontMap::from_shared(shared).config_fonts())
            .unwrap_or_else(|| self.fonts.clone());
        state.sorted = Some(fonts.sort(&self.pattern));
    }

    fn font_pattern(&self, i: usize) -> Option<(Pattern, bool)> {
        let mut state = self.lock();
        if i == 0 {
            if !state.matched {
                let (matched, no_match) = self.fonts.font_match(&self.pattern);
                state.matched = true;
                state.matched_font = matched;
                if no_match {
                    self.sort_into(&mut state);
                }
            }
            if let Some(matched) = state.matched_font.as_ref() {
                return Some((matched.clone(), false));
            }
        } else {
            self.sort_into(&mut state);
        }
        let sorted = state.sorted.as_ref()?.as_ref()?;
        sorted.get(i).map(|pattern| (pattern, true))
    }
}

#[derive(Clone)]
pub struct Fontset(Arc<FontsetInner>);

struct FontsetInner {
    fontmap: Weak<FontMapShared>,
    key: FontsetKey,
    patterns: Arc<Patterns>,
    state: Mutex<FontsetState>,
}

#[derive(Default)]
struct FontsetState {
    patterns_i: usize,
    fonts: Vec<Option<Font>>,
    coverages: Vec<Option<FontCoverage>>,
}

impl core::fmt::Debug for Fontset {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Fontset({:p})", Arc::as_ptr(&self.0))
    }
}

impl Fontset {
    pub(crate) fn new(
        fontmap: Weak<FontMapShared>,
        key: FontsetKey,
        patterns: Arc<Patterns>,
    ) -> Fontset {
        Fontset(Arc::new(FontsetInner {
            fontmap,
            key,
            patterns,
            state: Mutex::new(FontsetState::default()),
        }))
    }

    pub(crate) fn key(&self) -> &FontsetKey {
        &self.0.key
    }

    pub fn ptr_eq(&self, other: &Fontset) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }

    fn lock(&self) -> MutexGuard<'_, FontsetState> {
        self.0
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn language(&self) -> Option<Language> {
        self.0.key.language
    }

    fn load_next_font(&self, state: &mut FontsetState) -> Option<Font> {
        let index = state.patterns_i;
        state.patterns_i += 1;
        let (font_pattern, prepare) = self.0.patterns.font_pattern(index)?;
        let font_pattern = if prepare {
            self.0.patterns.pattern.render_prepare(&font_pattern)?
        } else {
            font_pattern
        };
        let fontmap = FontMap::from_shared(self.0.fontmap.upgrade()?);
        fontmap.new_font(&self.0.key, &font_pattern)
    }

    pub fn font_at(&self, i: usize) -> Option<Font> {
        let mut state = self.lock();
        while i >= state.fonts.len() {
            let font = self.load_next_font(&mut state);
            let loaded = font.is_some();
            state.fonts.push(font);
            state.coverages.push(None);
            if !loaded {
                return None;
            }
        }
        state.fonts[i].clone()
    }

    fn coverage_at(&self, i: usize, font: &Font) -> FontCoverage {
        if let Some(coverage) = self.lock().coverages.get(i).and_then(Clone::clone) {
            return coverage;
        }
        let coverage = font.coverage(self.0.key.language);
        if let Some(slot) = self.lock().coverages.get_mut(i) {
            *slot = Some(coverage.clone());
        }
        coverage
    }

    pub fn get_font(&self, wc: u32) -> Option<Font> {
        let mut best_level = CoverageLevel::None;
        let mut result: Option<Font> = None;
        let mut i = 0;
        while let Some(font) = self.font_at(i) {
            let coverage = self.coverage_at(i, &font);
            let level = coverage.get(wc);
            if result.is_none() || level > best_level {
                best_level = level;
                result = Some(font);
                if level == CoverageLevel::Exact {
                    break;
                }
            }
            i += 1;
        }
        result
    }

    pub fn foreach(&self, mut func: impl FnMut(&Font) -> bool) {
        let mut i = 0;
        while let Some(font) = self.font_at(i) {
            if func(&font) {
                return;
            }
            i += 1;
        }
    }

    pub fn metrics(&self) -> FontMetrics {
        let language = self.language();
        let sample_str = language_get_sample_string(language);
        let mut metrics = FontMetrics::new();
        self.foreach(|font| {
            metrics = font.metrics(language);
            true
        });
        let mut count = 0;
        let mut fonts_seen: HashSet<usize> = HashSet::new();
        for wc in sample_str.chars() {
            let Some(font) = self.get_font(u32::from(wc)) else {
                continue;
            };
            if !fonts_seen.insert(font.id()) {
                continue;
            }
            let raw_metrics = font.metrics(language);
            if count == 0 {
                metrics.ascent = raw_metrics.ascent;
                metrics.descent = raw_metrics.descent;
                metrics.approximate_char_width = raw_metrics.approximate_char_width;
                metrics.approximate_digit_width = raw_metrics.approximate_digit_width;
            } else {
                metrics.ascent = metrics.ascent.max(raw_metrics.ascent);
                metrics.descent = metrics.descent.max(raw_metrics.descent);
                metrics.approximate_char_width += raw_metrics.approximate_char_width;
                metrics.approximate_digit_width += raw_metrics.approximate_digit_width;
            }
            count += 1;
        }
        if count != 0 {
            metrics.approximate_char_width /= count;
            metrics.approximate_digit_width /= count;
        }
        metrics
    }
}
