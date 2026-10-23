//! Southstar text — itemization: cutting a paragraph into items of one bidi level, script, language, font, width class and emoji presentation.
//! Copyright 2000, 2006 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::attrs::{
    AttrIterator, AttrList, AttrType, AttrValue, Attribute, attr_font_scale_new,
    attr_text_transform_new,
};
use crate::bidi::log2vis_fill_embedding_levels;
use crate::context::Context;
use crate::emoji::EmojiIter;
use crate::ffi::glib_case::{unichar_islower, unicode_script_to_iso15924};
use crate::ffi::harfbuzz::{
    METRICS_CAP_HEIGHT, METRICS_SUBSCRIPT_EM_Y_SIZE, METRICS_SUPERSCRIPT_EM_Y_SIZE,
    METRICS_X_HEIGHT, tag,
};
use crate::font::Font;
use crate::font_description::{FontDescription, c_double_to_int};
use crate::fontset::Fontset;
use crate::gravity::gravity_get_for_script_and_width;
use crate::item::{Item, analysis_flags};
use crate::item_cache;
use crate::language::{Language, language_includes_script, script_get_sample_language};
use crate::script::{Script, ScriptIter};
use crate::types::{
    Direction, FontColor, FontScale, Gravity, GravityHint, LogAttr, TextTransform, Variant,
    font_mask,
};
use crate::unicode::{UnicodeType, unichar_type};
use crate::utf8::{char_at, g_utf8_strlen, next_char, pointer_to_offset};

const EMBEDDING_CHANGED: u32 = 1 << 0;
const SCRIPT_CHANGED: u32 = 1 << 1;
const LANG_CHANGED: u32 = 1 << 2;
const FONT_CHANGED: u32 = 1 << 3;
const DERIVED_LANG_CHANGED: u32 = 1 << 4;
const WIDTH_CHANGED: u32 = 1 << 5;
const EMOJI_CHANGED: u32 = 1 << 6;

const NO_FONT_POSITION: i32 = 0xffff;
const LINE_SEPARATOR: u32 = 0x2028;

const UPRIGHT: [(u32, u32); 98] = [
    (0x00A7, 0x00A7),
    (0x00A9, 0x00A9),
    (0x00AE, 0x00AE),
    (0x00B1, 0x00B1),
    (0x00BC, 0x00BE),
    (0x00D7, 0x00D7),
    (0x00F7, 0x00F7),
    (0x02EA, 0x02EB),
    (0x1100, 0x11FF),
    (0x1401, 0x167F),
    (0x18B0, 0x18FF),
    (0x2016, 0x2016),
    (0x2020, 0x2021),
    (0x2030, 0x2031),
    (0x203B, 0x203C),
    (0x2042, 0x2042),
    (0x2047, 0x2049),
    (0x2051, 0x2051),
    (0x2065, 0x2065),
    (0x20DD, 0x20E0),
    (0x20E2, 0x20E4),
    (0x2100, 0x2101),
    (0x2103, 0x2109),
    (0x210F, 0x210F),
    (0x2113, 0x2114),
    (0x2116, 0x2117),
    (0x211E, 0x2123),
    (0x2125, 0x2125),
    (0x2127, 0x2127),
    (0x2129, 0x2129),
    (0x212E, 0x212E),
    (0x2135, 0x213F),
    (0x2145, 0x214A),
    (0x214C, 0x214D),
    (0x214F, 0x2189),
    (0x218C, 0x218F),
    (0x221E, 0x221E),
    (0x2234, 0x2235),
    (0x2300, 0x2307),
    (0x230C, 0x231F),
    (0x2324, 0x2328),
    (0x232B, 0x232B),
    (0x237D, 0x239A),
    (0x23BE, 0x23CD),
    (0x23CF, 0x23CF),
    (0x23D1, 0x23DB),
    (0x23E2, 0x2422),
    (0x2424, 0x24FF),
    (0x25A0, 0x2619),
    (0x2620, 0x2767),
    (0x2776, 0x2793),
    (0x2B12, 0x2B2F),
    (0x2B50, 0x2B59),
    (0x2BB8, 0x2BD1),
    (0x2BD3, 0x2BEB),
    (0x2BF0, 0x2BFF),
    (0x2E80, 0x3007),
    (0x3012, 0x3013),
    (0x3020, 0x302F),
    (0x3031, 0x309F),
    (0x30A1, 0x30FB),
    (0x30FD, 0xA4CF),
    (0xA960, 0xA97F),
    (0xAC00, 0xD7FF),
    (0xE000, 0xFAFF),
    (0xFE10, 0xFE1F),
    (0xFE30, 0xFE48),
    (0xFE50, 0xFE57),
    (0xFE5F, 0xFE62),
    (0xFE67, 0xFE6F),
    (0xFF01, 0xFF07),
    (0xFF0A, 0xFF0C),
    (0xFF0E, 0xFF19),
    (0xFF1F, 0xFF3A),
    (0xFF3C, 0xFF3C),
    (0xFF3E, 0xFF3E),
    (0xFF40, 0xFF5A),
    (0xFFE0, 0xFFE2),
    (0xFFE4, 0xFFE7),
    (0xFFF0, 0xFFF8),
    (0xFFFC, 0xFFFD),
    (0x10980, 0x1099F),
    (0x11580, 0x115FF),
    (0x11A00, 0x11AAF),
    (0x13000, 0x1342F),
    (0x14400, 0x1467F),
    (0x16FE0, 0x18AFF),
    (0x1B000, 0x1B12F),
    (0x1B170, 0x1B2FF),
    (0x1D000, 0x1D1FF),
    (0x1D2E0, 0x1D37F),
    (0x1D800, 0x1DAAF),
    (0x1F000, 0x1F7FF),
    (0x1F900, 0x1FA6F),
    (0x20000, 0x2FFFD),
    (0x30000, 0x3FFFD),
    (0xF0000, 0xFFFFD),
    (0x100000, 0x10FFFD),
];

fn width_iter_is_upright(ch: u32) -> bool {
    let table = &UPRIGHT;
    if ch < table[0].0 {
        return false;
    }
    let mut st: i32 = 0;
    let mut ed: i32 = table.len() as i32 - 1;
    while st <= ed {
        let mid = (st + ed) / 2;
        let (lo, hi) = table[mid as usize];
        if lo <= ch && ch <= hi {
            return true;
        } else if lo <= ch {
            st = mid + 1;
        } else {
            ed = mid - 1;
        }
    }
    false
}

struct WidthIter {
    text_end: usize,
    start: usize,
    end: usize,
    upright: bool,
}

impl WidthIter {
    fn new(text: &[u8], start: usize, end: usize) -> WidthIter {
        let mut iter = WidthIter {
            text_end: end,
            start,
            end: start,
            upright: false,
        };
        iter.next(text);
        iter
    }

    fn next(&mut self, text: &[u8]) {
        let mut met_joiner = false;
        self.start = self.end;

        if self.end < self.text_end {
            self.upright = width_iter_is_upright(char_at(text, self.end));
        }

        while self.end < self.text_end {
            let ch = char_at(text, self.end);

            if ch == 0x200D {
                self.end = next_char(text, self.end);
                met_joiner = true;
                continue;
            }

            if met_joiner {
                self.end = next_char(text, self.end);
                met_joiner = false;
                continue;
            }

            if ch == 0xFE0E
                || ch == 0xFE0F
                || (0xE0020..=0xE007F).contains(&ch)
                || (0x1F3FB..=0x1F3FF).contains(&ch)
            {
                self.end = next_char(text, self.end);
                continue;
            }

            if width_iter_is_upright(ch) != self.upright {
                break;
            }

            self.end = next_char(text, self.end);
        }
    }
}

struct ItemizeState<'a, 'i, 'l> {
    context: &'a Context,
    bytes: &'a [u8],
    start_index: usize,
    end: usize,
    run_start: usize,
    run_end: usize,
    result: Vec<Item>,
    has_item: bool,
    embedding_levels: Vec<u8>,
    embedding_end_offset: usize,
    embedding_end: usize,
    embedding: u8,
    gravity: Gravity,
    gravity_hint: GravityHint,
    resolved_gravity: Gravity,
    font_desc_gravity: Gravity,
    centered_baseline: bool,
    attr_iter: Option<&'i mut AttrIterator<'l>>,
    attr_end: usize,
    font_desc: FontDescription,
    emoji_font_desc: Option<FontDescription>,
    text_emoji_font_desc: Option<FontDescription>,
    lang: Option<Language>,
    lang_is_explicit: bool,
    extra_attrs: Vec<Attribute>,
    changed: u32,
    script_iter: ScriptIter<'a>,
    script_end: usize,
    script: Script,
    width_iter: WidthIter,
    emoji_iter: EmojiIter<'a>,
    derived_lang: Option<Language>,
    current_fonts: Option<Fontset>,
    base_font: Option<Font>,
    enable_fallback: bool,
    first_space: Option<usize>,
    font_position: i32,
}

fn find_attribute(attrs: &[Attribute], kind: AttrType) -> Option<&Attribute> {
    attrs.iter().find(|attr| attr.kind() == kind)
}

fn attr_int(attr: &Attribute) -> i32 {
    attr.value.as_int().unwrap_or(0)
}

fn compute_derived_language(lang: Option<Language>, script: Script) -> Option<Language> {
    if lang.is_some() && language_includes_script(lang, script) {
        return lang;
    }
    Some(script_get_sample_language(script).unwrap_or_else(|| Language::from_string("xx")))
}

pub(crate) fn consider_as_space(wc: u32) -> bool {
    let kind = unichar_type(wc);
    kind == UnicodeType::Control
        || (kind == UnicodeType::Format
            && !((0x600..=0x06ff).contains(&wc) || wc == 0x70f || wc == 0x8e2))
        || kind == UnicodeType::Surrogate
        || kind == UnicodeType::LineSeparator
        || kind == UnicodeType::ParagraphSeparator
        || (kind == UnicodeType::SpaceSeparator && wc != 0x1680)
        || (0xfe00..=0xfe0f).contains(&wc)
        || (0xe0100..=0xe01ef).contains(&wc)
}

fn same_font(a: Option<&Font>, b: Option<&Font>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

impl<'a, 'i, 'l> ItemizeState<'a, 'i, 'l> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        context: &'a Context,
        text: &'a str,
        base_dir: Direction,
        start_index: usize,
        length: usize,
        attr_iter: Option<&'i mut AttrIterator<'l>>,
        desc: Option<&FontDescription>,
    ) -> ItemizeState<'a, 'i, 'l> {
        let bytes = text.as_bytes();
        let end = start_index + length;
        let slice = &text[start_index..end];
        let n_chars = g_utf8_strlen(slice.as_bytes(), length) as usize;

        let mut embedding_levels =
            vec![0u8; n_chars.max(pointer_to_offset(bytes, start_index, end) as usize)];
        let mut base_dir = base_dir;
        log2vis_fill_embedding_levels(slice, &mut embedding_levels[..n_chars], &mut base_dir);

        let script_iter = ScriptIter::new(slice);
        let (_, script_rel_end, script) = script_iter.range();
        let emoji_iter = EmojiIter::new(slice);
        let width_iter = WidthIter::new(bytes, start_index, end);

        let mut state = ItemizeState {
            context,
            bytes,
            start_index,
            end,
            run_start: start_index,
            run_end: start_index,
            result: Vec::new(),
            has_item: false,
            embedding_levels,
            embedding_end_offset: 0,
            embedding_end: start_index,
            embedding: 0,
            gravity: Gravity::Auto,
            gravity_hint: context.gravity_hint(),
            resolved_gravity: Gravity::Auto,
            font_desc_gravity: Gravity::Auto,
            centered_baseline: context.gravity().is_vertical(),
            attr_iter,
            attr_end: end,
            font_desc: desc.unwrap_or(context.font_description()).clone(),
            emoji_font_desc: None,
            text_emoji_font_desc: None,
            lang: Some(context.effective_language()),
            lang_is_explicit: false,
            extra_attrs: Vec::new(),
            changed: EMBEDDING_CHANGED
                | SCRIPT_CHANGED
                | LANG_CHANGED
                | FONT_CHANGED
                | WIDTH_CHANGED
                | EMOJI_CHANGED,
            script_iter,
            script_end: start_index + script_rel_end,
            script,
            width_iter,
            emoji_iter,
            derived_lang: None,
            current_fonts: None,
            base_font: None,
            enable_fallback: true,
            first_space: None,
            font_position: NO_FONT_POSITION,
        };

        state.update_embedding_end();

        if let Some(iter) = state.attr_iter.as_deref_mut() {
            iter.advance(start_index as i32);
        }
        if state.attr_iter.is_some() {
            state.lang = None;
            state.lang_is_explicit = false;
            state.update_attr_iterator();
        }

        if !context.gravity().is_vertical() {
            state.width_iter.end = state.end;
        } else if state.emoji_iter.is_emoji() {
            state.width_iter.end = state
                .width_iter
                .end
                .max(start_index + state.emoji_iter.end());
        }

        state.update_end();

        state.font_desc_gravity = if state.font_desc.set_fields() & font_mask::GRAVITY != 0 {
            state.font_desc.gravity()
        } else {
            Gravity::Auto
        };

        state
    }

    fn emoji_end(&self) -> usize {
        self.start_index + self.emoji_iter.end()
    }

    fn update_embedding_end(&mut self) {
        self.embedding = self.embedding_levels[self.embedding_end_offset];
        while self.embedding_end < self.end
            && self.embedding_levels[self.embedding_end_offset] == self.embedding
        {
            self.embedding_end_offset += 1;
            self.embedding_end = next_char(self.bytes, self.embedding_end);
        }
        self.changed |= EMBEDDING_CHANGED;
    }

    fn update_attr_iterator(&mut self) {
        let Some(iter) = self.attr_iter.as_deref_mut() else {
            return;
        };
        let (_, end_index) = iter.range();
        self.attr_end = if (end_index as i64) < self.end as i64 {
            end_index as usize
        } else {
            self.end
        };

        self.emoji_font_desc = None;
        self.text_emoji_font_desc = None;

        let old_lang = self.lang;
        let old_lang_was_explicit = self.lang_is_explicit;
        self.font_desc = self.context.font_description().clone();
        iter.get_font(
            &mut self.font_desc,
            Some(&mut self.lang),
            Some(&mut self.extra_attrs),
        );
        self.font_desc_gravity = if self.font_desc.set_fields() & font_mask::GRAVITY != 0 {
            self.font_desc.gravity()
        } else {
            Gravity::Auto
        };

        if self.lang.is_none() {
            self.lang = Some(self.context.effective_language());
            self.lang_is_explicit = false;
        } else {
            self.lang_is_explicit = true;
        }

        self.enable_fallback =
            find_attribute(&self.extra_attrs, AttrType::Fallback).is_none_or(|a| attr_int(a) != 0);

        self.gravity = find_attribute(&self.extra_attrs, AttrType::Gravity)
            .map_or(Gravity::Auto, |a| {
                Gravity::from_i32(attr_int(a)).unwrap_or(Gravity::Auto)
            });

        self.gravity_hint = find_attribute(&self.extra_attrs, AttrType::GravityHint)
            .map_or(self.context.gravity_hint(), |a| {
                GravityHint::from_i32(attr_int(a)).unwrap_or(GravityHint::Natural)
            });

        self.changed |= FONT_CHANGED;
        if self.lang != old_lang {
            self.changed |= LANG_CHANGED;
        }
        if self.lang_is_explicit != old_lang_was_explicit {
            self.changed |= LANG_CHANGED;
        }
    }

    fn update_end(&mut self) {
        self.run_end = self.embedding_end;
        if self.attr_end < self.run_end {
            self.run_end = self.attr_end;
        }
        if self.script_end < self.run_end {
            self.run_end = self.script_end;
        }
        if self.width_iter.end < self.run_end {
            self.run_end = self.width_iter.end;
        }
        let emoji_end = self.emoji_end();
        if emoji_end < self.run_end {
            self.run_end = emoji_end;
        }
    }

    fn next(&mut self) -> bool {
        if self.run_end == self.end {
            return false;
        }

        self.changed = 0;
        self.run_start = self.run_end;

        if self.run_end == self.embedding_end {
            self.update_embedding_end();
        }

        if self.run_end == self.attr_end {
            if let Some(iter) = self.attr_iter.as_deref_mut() {
                iter.next();
            }
            self.update_attr_iterator();
        }

        if self.run_end == self.script_end {
            self.script_iter.next_range();
            let (_, rel_end, script) = self.script_iter.range();
            self.script_end = self.start_index + rel_end;
            self.script = script;
            self.changed |= SCRIPT_CHANGED;
        }

        if self.run_end == self.emoji_end() {
            self.emoji_iter.next_run();
            self.changed |= EMOJI_CHANGED;
            if self.emoji_iter.is_emoji() {
                self.width_iter.end = self.width_iter.end.max(self.emoji_end());
            }
        }

        if self.run_end == self.width_iter.end {
            self.width_iter.next(self.bytes);
            self.changed |= WIDTH_CHANGED;
        }

        self.update_end();
        true
    }

    fn fill_font(&mut self, font: Option<&Font>) {
        for item in self.result.iter_mut().rev() {
            if item.analysis.font.is_some() {
                break;
            }
            if let Some(font) = font {
                item.analysis.font = Some(font.clone());
            }
        }
    }

    fn get_base_font(&mut self) -> Option<Font> {
        if self.base_font.is_none() {
            self.base_font = self.context.load_font(&self.font_desc);
        }
        self.base_font.clone()
    }

    fn add_character(
        &mut self,
        mut font: Option<Font>,
        mut font_position: i32,
        force_break: bool,
        mut pos: usize,
        is_space: bool,
    ) {
        let first_space = self.first_space;
        let mut n_spaces = 0;

        if is_space {
            if self.first_space.is_none() {
                self.first_space = Some(pos);
            }
        } else {
            self.first_space = None;
        }

        if self.has_item {
            let item_font = self
                .result
                .last()
                .and_then(|item| item.analysis.font.clone());
            if item_font.is_none() && font.is_some() {
                self.fill_font(font.as_ref());
                self.font_position = font_position;
            } else if item_font.is_some() && font.is_none() {
                font = item_font;
                font_position = self.font_position;
            }

            let current_position = self.font_position;
            let bytes = self.bytes;
            let Some(item) = self.result.last_mut() else {
                return;
            };

            if !force_break && same_font(item.analysis.font.as_ref(), font.as_ref()) {
                item.num_chars += 1;
                return;
            }

            if let Some(first_space) = first_space
                && (item.offset as usize) < first_space
                && font_position < current_position
            {
                n_spaces = g_utf8_strlen(&bytes[first_space..], pos - first_space);
                item.num_chars -= n_spaces;
                pos = first_space;
            }

            item.length = pos as i32 - item.offset;
        }

        let mut item = Item::new();
        item.offset = pos as i32;
        item.length = 0;
        item.num_chars = n_spaces + 1;
        item.analysis.font = font;
        self.font_position = font_position;
        item.analysis.strut_font = self.get_base_font();

        item.analysis.level = self.embedding;
        item.analysis.gravity = self.resolved_gravity;

        match item.analysis.gravity {
            Gravity::North => item.analysis.level = item.analysis.level.wrapping_add(1),
            Gravity::East => {
                item.analysis.level = item.analysis.level.wrapping_add(1);
                item.analysis.level &= !1;
            }
            Gravity::West => item.analysis.level |= 1,
            Gravity::South | Gravity::Auto => {}
        }

        if self.centered_baseline {
            item.analysis.flags |= analysis_flags::CENTERED_BASELINE;
        }

        item.analysis.script = self.script;
        item.analysis.language = self.derived_lang;
        item.analysis.extra_attrs = self.extra_attrs.clone();

        self.result.push(item);
        self.has_item = true;
    }

    fn get_font(&mut self, wc: u32) -> (Option<Font>, i32) {
        if self.enable_fallback
            && let Some(cached) = self
                .current_fonts
                .as_ref()
                .and_then(|fonts| fonts.cached_char_font(wc))
        {
            return cached;
        }

        let mut found: Option<Font> = None;
        let mut position = 0;

        if self.enable_fallback
            && let Some(fonts) = self.current_fonts.as_ref()
        {
            fonts.foreach(|font| {
                if font.has_char(wc) {
                    found = Some(font.clone());
                    return true;
                }
                position += 1;
                false
            });
        }

        if found.is_none() {
            found = self.get_base_font();
        }

        if self.enable_fallback
            && let Some(fonts) = self.current_fonts.as_ref()
        {
            fonts.cache_char_font(wc, found.clone(), position);
        }

        (found, position)
    }

    fn update_for_new_run(&mut self) {
        if self.changed & (FONT_CHANGED | SCRIPT_CHANGED | WIDTH_CHANGED) != 0 {
            if self.font_desc_gravity != Gravity::Auto {
                self.resolved_gravity = self.font_desc_gravity;
            } else {
                let mut gravity = self.gravity;
                if gravity == Gravity::Auto {
                    gravity = self.context.gravity();
                }
                self.resolved_gravity = gravity_get_for_script_and_width(
                    self.script,
                    self.width_iter.upright,
                    gravity,
                    self.gravity_hint,
                );
            }

            if self.font_desc_gravity != self.resolved_gravity {
                self.font_desc.set_gravity(self.resolved_gravity);
                self.changed |= FONT_CHANGED;
            }
        }

        if self.changed & (SCRIPT_CHANGED | LANG_CHANGED) != 0 {
            let old_derived_lang = self.derived_lang;
            self.derived_lang = if self.lang_is_explicit {
                self.lang
            } else {
                compute_derived_language(self.lang, self.script)
            };
            if old_derived_lang != self.derived_lang {
                self.changed |= DERIVED_LANG_CHANGED;
            }
        }

        if self.changed & EMOJI_CHANGED != 0 {
            self.changed |= FONT_CHANGED;
        }

        if self.changed & (FONT_CHANGED | DERIVED_LANG_CHANGED) != 0 && self.current_fonts.is_some()
        {
            self.current_fonts = None;
        }

        if self.current_fonts.is_none() {
            let is_emoji = self.emoji_iter.is_emoji();
            let has_vs = self.emoji_iter.has_vs();
            if is_emoji && self.emoji_font_desc.is_none() {
                let mut desc = self.font_desc.clone();
                desc.set_family(Some("emoji"));
                desc.set_color(if has_vs {
                    FontColor::Required
                } else {
                    FontColor::DontCare
                });
                self.emoji_font_desc = Some(desc);
            }
            if !is_emoji && has_vs && self.text_emoji_font_desc.is_none() {
                let mut desc = self.font_desc.clone();
                desc.set_family(Some("emoji"));
                desc.set_color(FontColor::Forbidden);
                self.text_emoji_font_desc = Some(desc);
            }
            let desc = if is_emoji {
                self.emoji_font_desc.as_ref()
            } else if has_vs {
                self.text_emoji_font_desc.as_ref()
            } else {
                Some(&self.font_desc)
            };
            let desc = desc.unwrap_or(&self.font_desc);
            self.current_fonts = self.context.load_fontset(desc, self.derived_lang);
        }

        if self.changed & FONT_CHANGED != 0 && self.base_font.is_some() {
            self.base_font = None;
        }
    }

    fn process_run(&mut self) {
        self.update_for_new_run();

        let mut last_was_forced_break = false;
        let mut p = self.run_start;
        while p < self.run_end {
            let wc = char_at(self.bytes, p);
            let is_forced_break = wc == u32::from('\t') || wc == LINE_SEPARATOR;
            let (font, font_position, is_space) = if consider_as_space(wc) {
                (None, NO_FONT_POSITION, true)
            } else {
                let (font, position) = self.get_font(wc);
                (font, position, false)
            };

            self.add_character(
                font,
                font_position,
                is_forced_break || last_was_forced_break,
                p,
                is_space,
            );

            last_was_forced_break = is_forced_break;
            p = next_char(self.bytes, p);
        }

        if let Some(item) = self.result.last_mut() {
            item.length = p as i32 - item.offset;
        }
        if self
            .result
            .last()
            .is_some_and(|item| item.analysis.font.is_none())
        {
            let (font, _) = self.get_font(u32::from(' '));
            self.fill_font(font.as_ref());
        }
        self.has_item = false;
    }

    fn into_items(self) -> Vec<Item> {
        self.result
    }
}

fn hb_scale_ratio(font: &Font, metrics_tag: u32) -> Option<f64> {
    let hb_font = font.hb_font();
    let size = hb_font.ot_metrics_position(metrics_tag)?;
    Some(f64::from(size) / f64::from(hb_font.y_scale()))
}

fn small_caps_ratio(font: &Font) -> Option<f64> {
    let hb_font = font.hb_font();
    let cap_height = hb_font.ot_metrics_position(METRICS_CAP_HEIGHT)?;
    let x_height = hb_font.ot_metrics_position(METRICS_X_HEIGHT)?;
    Some(f64::from(x_height) / f64::from(cap_height))
}

struct ScaleEntry {
    value: i32,
    end_index: u32,
    scale: f64,
}

fn collect_font_scale(
    stack: &mut Vec<ScaleEntry>,
    item: &Item,
    prev: Option<&Item>,
) -> Option<(f64, bool)> {
    for attr in &item.analysis.extra_attrs {
        let AttrValue::FontScale(value) = attr.value else {
            continue;
        };
        if attr.start_index != item.offset as u32 {
            continue;
        }
        let prev_font = prev.and_then(|prev| prev.analysis.font.as_ref());
        let scale = match FontScale::from_i32(value) {
            Some(FontScale::Superscript) => prev_font
                .and_then(|font| hb_scale_ratio(font, METRICS_SUPERSCRIPT_EM_Y_SIZE))
                .unwrap_or(1.0 / 1.2),
            Some(FontScale::Subscript) => prev_font
                .and_then(|font| hb_scale_ratio(font, METRICS_SUBSCRIPT_EM_Y_SIZE))
                .unwrap_or(1.0 / 1.2),
            Some(FontScale::SmallCaps) => item
                .analysis
                .font
                .as_ref()
                .and_then(small_caps_ratio)
                .unwrap_or(0.8),
            Some(FontScale::None) | None => 1.0,
        };
        stack.insert(
            0,
            ScaleEntry {
                value,
                end_index: attr.end_index,
                scale,
            },
        );
    }

    let mut scale = 1.0;
    let mut is_small_caps = true;
    let mut retval = false;
    for entry in stack.iter() {
        scale *= entry.scale;
        if entry.value != FontScale::SmallCaps as i32 {
            is_small_caps = false;
        }
        retval = true;
    }

    let item_end = (item.offset + item.length) as u32;
    stack.retain(|entry| entry.end_index != item_end);

    retval.then_some((scale, is_small_caps))
}

fn apply_scale_to_item(context: &Context, item: &mut Item, scale: f64, is_small_caps: bool) {
    let Some(font) = item.analysis.font.clone() else {
        return;
    };

    if is_small_caps {
        item.analysis.size_font = Some(font.clone());
    }

    let mut desc = font.describe();
    let size = scale * f64::from(desc.size());
    if desc.size_is_absolute() {
        desc.set_absolute_size(size);
    } else {
        desc.set_size(c_double_to_int(size));
    }

    item.analysis.font = context.load_font(&desc);
}

fn apply_font_scale(context: &Context, items: &mut [Item]) {
    let mut stack: Vec<ScaleEntry> = Vec::new();
    for i in 0..items.len() {
        let (before, rest) = items.split_at_mut(i);
        let item = &mut rest[0];
        if let Some((scale, is_small_caps)) = collect_font_scale(&mut stack, item, before.last()) {
            apply_scale_to_item(context, item, scale, is_small_caps);
        }
    }
}

fn variant_supported(item: &Item, variant: Variant) -> bool {
    let features: &[u32] = match variant {
        Variant::Normal | Variant::TitleCaps => return true,
        Variant::SmallCaps => &[tag(b"smcp")],
        Variant::AllSmallCaps => &[tag(b"smcp"), tag(b"c2sc")],
        Variant::PetiteCaps => &[tag(b"pcap")],
        Variant::AllPetiteCaps => &[tag(b"pcap"), tag(b"c2pc")],
        Variant::Unicase => &[tag(b"unic")],
    };
    let Some(font) = item.analysis.font.as_ref() else {
        return true;
    };
    let script_tag = unicode_script_to_iso15924(i32::from(item.analysis.script.0 as u8));
    font.hb_font().gsub_features_supported(
        script_tag,
        item.analysis.language.map(Language::as_str),
        features,
    )
}

fn find_text_transform(item: &Item) -> TextTransform {
    let mut transform = TextTransform::None;
    for attr in &item.analysis.extra_attrs {
        if let AttrValue::TextTransform(value) = attr.value {
            transform = TextTransform::from_i32(value).unwrap_or(TextTransform::None);
        }
    }
    transform
}

fn is_word_start_at(log_attrs: Option<&[LogAttr]>, offset: i32) -> bool {
    log_attrs.is_some_and(|attrs| {
        usize::try_from(offset)
            .ok()
            .and_then(|offset| attrs.get(offset))
            .is_some_and(|attr| attr.is_word_start())
    })
}

fn push_range_attr(item: &mut Item, mut attr: Attribute) {
    attr.start_index = item.offset as u32;
    attr.end_index = (item.offset + item.length) as u32;
    item.analysis.extra_attrs.push(attr);
}

fn segment_target<'b>(
    item: &'b mut Item,
    pieces: &'b mut Vec<Item>,
    bytes: &[u8],
    p0: usize,
    p: usize,
    end: usize,
) -> &'b mut Item {
    if p >= end {
        return item;
    }
    match item.split((p - p0) as i32, g_utf8_strlen(&bytes[p0..], p - p0)) {
        Some(new_item) => {
            let index = pieces.len();
            pieces.push(new_item);
            &mut pieces[index]
        }
        None => item,
    }
}

fn split_item_for_variant(
    text: &str,
    log_attrs: Option<&[LogAttr]>,
    variant: Variant,
    mut item: Item,
) -> Vec<Item> {
    let (transform, lowercase_scale, uppercase_scale) = match variant {
        Variant::AllSmallCaps | Variant::AllPetiteCaps => (
            TextTransform::Uppercase,
            FontScale::SmallCaps,
            FontScale::SmallCaps,
        ),
        Variant::SmallCaps | Variant::PetiteCaps => (
            TextTransform::Uppercase,
            FontScale::SmallCaps,
            FontScale::None,
        ),
        Variant::Unicase => (TextTransform::None, FontScale::None, FontScale::SmallCaps),
        Variant::Normal | Variant::TitleCaps => return vec![item],
    };

    let item_transform = find_text_transform(&item);
    let bytes = text.as_bytes();
    let start = item.offset as usize;
    let end = start + item.length as usize;
    let mut offset = item.char_offset;
    let mut pieces: Vec<Item> = Vec::new();

    let mut p = start;
    while p < end {
        let p0 = p;
        let mut wc = char_at(bytes, p);
        let mut is_word_start = is_word_start_at(log_attrs, offset);
        while p < end
            && (item_transform == TextTransform::Lowercase
                || consider_as_space(wc)
                || (unichar_islower(wc)
                    && !(item_transform == TextTransform::Uppercase
                        || (item_transform == TextTransform::Capitalize && is_word_start))))
        {
            p = next_char(bytes, p);
            wc = char_at(bytes, p);
            offset += 1;
            is_word_start = is_word_start_at(log_attrs, offset);
        }

        if p0 < p {
            let target = segment_target(&mut item, &mut pieces, bytes, p0, p, end);
            if transform != TextTransform::None {
                push_range_attr(target, attr_text_transform_new(transform));
            }
            if lowercase_scale != FontScale::None {
                push_range_attr(target, attr_font_scale_new(lowercase_scale));
            }
        }

        let p0 = p;
        let mut wc = char_at(bytes, p);
        let mut is_word_start = is_word_start_at(log_attrs, offset);
        while p < end
            && (item_transform == TextTransform::Uppercase
                || consider_as_space(wc)
                || !(item_transform == TextTransform::Lowercase || unichar_islower(wc))
                || (item_transform == TextTransform::Capitalize && is_word_start))
        {
            p = next_char(bytes, p);
            wc = char_at(bytes, p);
            offset += 1;
            is_word_start = is_word_start_at(log_attrs, offset);
        }

        if p0 < p {
            let target = segment_target(&mut item, &mut pieces, bytes, p0, p, end);
            if uppercase_scale != FontScale::None {
                push_range_attr(target, attr_font_scale_new(uppercase_scale));
            }
        }
    }

    pieces.push(item);
    pieces
}

fn handle_variants(text: &str, log_attrs: Option<&[LogAttr]>, items: &mut Vec<Item>) {
    let mut i = 0;
    while i < items.len() {
        let variant = items[i]
            .analysis
            .font
            .as_ref()
            .map_or(Variant::Normal, Font::variant);
        if variant_supported(&items[i], variant) {
            i += 1;
            continue;
        }
        let item = items.remove(i);
        let pieces = split_item_for_variant(text, log_attrs, variant, item);
        let count = pieces.len();
        items.splice(i..i, pieces);
        i += count;
    }
}

pub fn itemize_post_process_items(
    context: &Context,
    text: &str,
    log_attrs: Option<&[LogAttr]>,
    items: &mut Vec<Item>,
) {
    handle_variants(text, log_attrs, items);
    apply_font_scale(context, items);
}

#[allow(clippy::too_many_arguments)]
pub fn itemize_with_font<'l>(
    context: &mut Context,
    base_dir: Direction,
    text: &str,
    start_index: usize,
    length: usize,
    attrs: Option<&'l AttrList>,
    cached_iter: Option<&mut AttrIterator<'l>>,
    desc: Option<&FontDescription>,
) -> Vec<Item> {
    if context.font_map().is_none() {
        return Vec::new();
    }

    let Some(slice) = text.get(start_index..start_index + length) else {
        return Vec::new();
    };
    if length == 0 || slice.starts_with('\0') {
        return Vec::new();
    }

    let state_id = context.state_id();

    if desc.is_none()
        && let Some(items) =
            item_cache::lookup(state_id, base_dir, text, start_index, length, attrs)
    {
        return items;
    }

    let context: &Context = context;
    let mut owned_iter = match (cached_iter.is_some(), attrs) {
        (false, Some(attrs)) => Some(attrs.iterator()),
        _ => None,
    };
    let attr_iter = match cached_iter {
        Some(iter) => Some(iter),
        None => owned_iter.as_mut(),
    };

    let mut state = ItemizeState::new(
        context,
        text,
        base_dir,
        start_index,
        length,
        attr_iter,
        desc,
    );

    loop {
        state.process_run();
        if !state.next() {
            break;
        }
    }

    let mut items = state.into_items();

    let mut char_offset = g_utf8_strlen(text.as_bytes(), start_index);
    for item in &mut items {
        item.char_offset = char_offset;
        char_offset += item.num_chars;
    }

    if desc.is_none() {
        item_cache::insert(state_id, base_dir, text, start_index, length, attrs, &items);
    }

    items
}

pub fn itemize_with_base_dir<'l>(
    context: &mut Context,
    base_dir: Direction,
    text: &str,
    start_index: usize,
    length: usize,
    attrs: Option<&'l AttrList>,
    cached_iter: Option<&mut AttrIterator<'l>>,
) -> Vec<Item> {
    let mut items = itemize_with_font(
        context,
        base_dir,
        text,
        start_index,
        length,
        attrs,
        cached_iter,
        None,
    );
    itemize_post_process_items(context, text, None, &mut items);
    items
}

pub fn itemize<'l>(
    context: &mut Context,
    text: &str,
    start_index: usize,
    length: usize,
    attrs: Option<&'l AttrList>,
    cached_iter: Option<&mut AttrIterator<'l>>,
) -> Vec<Item> {
    let base_dir = context.base_dir();
    itemize_with_base_dir(
        context,
        base_dir,
        text,
        start_index,
        length,
        attrs,
        cached_iter,
    )
}
