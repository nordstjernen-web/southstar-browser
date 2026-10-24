//! Southstar text — line breaking: itemizing paragraphs, fitting runs into lines with tabs, hyphens and wrapping, then justifying and reordering them.
//! Copyright 2000, 2001, 2006 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::collections::VecDeque;

use crate::attrs::{
    AttrIterator, AttrList, AttrType, AttrValue, attr_font_desc_new, attr_language_new,
};
use crate::bidi::find_base_dir;
use crate::breaks::{AttrSpan, BreakAttrSpans, attr_break, default_break, tailor_break};
use crate::context::Context;
use crate::ellipsize::ellipsize_line;
use crate::ffi::harfbuzz::tag;
use crate::font::metrics_of;
use crate::glyph::GlyphString;
use crate::glyph_item::{GlyphItem, GlyphItemIter};
use crate::item::{Item, analysis_flags};
use crate::itemize::{itemize, itemize_post_process_items, itemize_with_font};
use crate::shape::{shape_item, shape_with_flags};
use crate::types::{
    Alignment, BaselineShift, Direction, GLYPH_EMPTY, Gravity, LogAttr, SCALE, TabAlign, WrapMode,
    get_unknown_glyph, shape_flags, show_flags, units_round,
};
use crate::utils::find_paragraph_boundary;

use super::extents::{ItemProperties, empty_extents_and_height_at_index, line_extents_and_height};
use super::{
    Layout, LayoutLine, affects_break_or_shape, affects_itemization, byte_at, byte_length_of_chars,
    char_at, utf8_strlen_at,
};

const LINE_SEPARATOR: u32 = 0x2028;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BreakResult {
    NoneFit,
    SomeFit,
    AllFit,
    EmptyFit,
    LineSeparator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TabGlyphs {
    Absent,
    Pending,
    Run(usize),
}

#[derive(Clone, Copy, Debug)]
struct LastTabState {
    glyphs: TabGlyphs,
    index: i32,
    width: i32,
    pos: i32,
    align: TabAlign,
    decimal: u32,
}

impl LastTabState {
    fn reset() -> LastTabState {
        LastTabState {
            glyphs: TabGlyphs::Absent,
            index: 0,
            width: 0,
            pos: 0,
            align: TabAlign::Left,
            decimal: 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct BaselineItem {
    start_index: u32,
    end_index: u32,
    value: i32,
    x_offset: i32,
    y_offset: i32,
}

struct ParaBreakState {
    line_height: i32,
    remaining_height: i32,
    items: VecDeque<Item>,
    base_dir: Direction,
    line_of_par: i32,
    glyphs: Option<GlyphString>,
    start_offset: i32,
    properties: ItemProperties,
    log_widths: Vec<i32>,
    log_widths_offset: i32,
    line_start_index: i32,
    line_start_offset: i32,
    line_width: i32,
    remaining_width: i32,
    hyphen_width: i32,
    baseline_shifts: Vec<BaselineItem>,
    last_tab: LastTabState,
}

struct Breaker<'a> {
    layout: &'a Layout,
    ctx: &'a mut Context,
    log_attrs: Vec<LogAttr>,
    lines: Vec<LayoutLine>,
    tab_width: i32,
    decimal: u32,
    is_wrapped: bool,
    is_ellipsized: bool,
}

fn break_spans(list: &AttrList, kind: AttrType) -> Vec<AttrSpan> {
    let mut sub = AttrList::new();
    for attr in list.as_slice() {
        if attr.kind() == kind {
            sub.insert(attr.clone());
        }
    }
    let mut spans = Vec::new();
    let mut iter = sub.iterator();
    loop {
        if let Some(attr) = iter.get(kind) {
            let value = attr.value.as_int().unwrap_or(0);
            let span = if kind == AttrType::InsertHyphens {
                let (start, end) = iter.range();
                AttrSpan {
                    start_index: start as u32,
                    end_index: end as u32,
                    value,
                }
            } else {
                AttrSpan {
                    start_index: attr.start_index,
                    end_index: attr.end_index,
                    value,
                }
            };
            spans.push(span);
        }
        if !iter.next() {
            break;
        }
    }
    spans
}

fn get_items_log_attrs(
    text: &str,
    start: usize,
    length: usize,
    items: &[Item],
    attrs: Option<&AttrList>,
    log_attrs: &mut [LogAttr],
) {
    let paragraph = &text[start..start + length];
    default_break(paragraph, log_attrs);

    let mut offset = 0usize;
    for item in items {
        let item_start = item.offset as usize;
        let item_end = item_start + item.length as usize;
        let num_chars = item.num_chars as usize;
        let language = item.analysis.language.map(|language| language.as_str());
        let end = (offset + num_chars + 1).min(log_attrs.len());
        tailor_break(
            text,
            item_start,
            item_end,
            item.analysis.script,
            language,
            &mut log_attrs[offset..end],
        );
        offset += num_chars;
    }

    if let (Some(attrs), Some(first)) = (attrs, items.first()) {
        let words = break_spans(attrs, AttrType::Word);
        let sentences = break_spans(attrs, AttrType::Sentence);
        let insert_hyphens = break_spans(attrs, AttrType::InsertHyphens);
        let allow_breaks = break_spans(attrs, AttrType::AllowBreaks);
        let spans = BreakAttrSpans {
            words: &words,
            sentences: &sentences,
            insert_hyphens: &insert_hyphens,
            allow_breaks: &allow_breaks,
        };
        attr_break(paragraph, &spans, first.offset, log_attrs);
    }
}

fn apply_attributes_to_items(items: &mut [Item], attrs: Option<&AttrList>) {
    let Some(attrs) = attrs else {
        return;
    };
    let mut iter = attrs.iterator();
    for item in items {
        item.apply_attrs(&mut iter);
    }
}

fn resolved_direction(direction: Direction, gravity: Gravity) -> Direction {
    let resolved = match direction {
        Direction::Rtl | Direction::WeakRtl | Direction::TtbLtr => Direction::Rtl,
        _ => Direction::Ltr,
    };
    match gravity {
        Gravity::North => {
            if resolved == Direction::Ltr {
                Direction::Rtl
            } else {
                Direction::Ltr
            }
        }
        Gravity::East => Direction::Ltr,
        Gravity::West => Direction::Rtl,
        Gravity::South | Gravity::Auto => resolved,
    }
}

fn distribute_letter_spacing(letter_spacing: i32) -> (i32, i32) {
    let mut space_left = letter_spacing / 2;
    if (letter_spacing & (SCALE - 1)) == 0 {
        space_left = units_round(space_left);
    }
    (space_left, letter_spacing - space_left)
}

fn find_show_flags(item: &Item) -> u32 {
    item.analysis
        .extra_attrs
        .iter()
        .filter_map(|attr| match attr.value {
            AttrValue::Show(value) => Some(value as u32),
            _ => None,
        })
        .fold(0, |flags, value| flags | value)
}

fn find_hyphen_width(item: &Item) -> i32 {
    let Some(font) = item.analysis.font.as_ref() else {
        return 0;
    };
    font.nominal_glyph(0x2010)
        .or_else(|| font.nominal_glyph(u32::from('-')))
        .map_or(0, |glyph| font.glyph_h_advance(glyph))
}

fn shape_shape(text: &str, offset: i32, n_chars: i32, width: i32, glyphs: &mut GlyphString) {
    let n = n_chars.max(0) as usize;
    glyphs.set_size(n);
    let bytes = text.as_bytes();
    let mut p = offset.max(0) as usize;
    for i in 0..n {
        let info = &mut glyphs.glyphs[i];
        info.glyph = GLYPH_EMPTY;
        info.geometry.x_offset = 0;
        info.geometry.y_offset = 0;
        info.geometry.width = width;
        info.attr.is_cluster_start = true;
        glyphs.log_clusters[i] = (p - offset.max(0) as usize) as i32;
        p = crate::utf8::next_char(bytes, p);
    }
}

fn with_glyph_item<R>(
    item: &mut Item,
    glyphs: &mut GlyphString,
    f: impl FnOnce(&mut GlyphItem) -> R,
) -> R {
    let mut glyph_item = GlyphItem::new(core::mem::take(item), core::mem::take(glyphs));
    let result = f(&mut glyph_item);
    *item = glyph_item.item;
    *glyphs = glyph_item.glyphs;
    result
}

fn decimal_prefix_width(glyph_item: &GlyphItem, text: &str, decimal: u32) -> (i32, bool) {
    let item = &glyph_item.item;
    let mut log_widths = vec![0; item.num_chars.max(0) as usize];
    glyph_item.get_logical_widths(text, &mut log_widths);
    let mut width = 0;
    let bytes = text.as_bytes();
    let mut p = item.offset as usize;
    for &char_width in &log_widths {
        if crate::utf8::char_at(bytes, p) == decimal {
            width += char_width / 2;
            return (width, true);
        }
        width += char_width;
        p = crate::utf8::next_char(bytes, p);
    }
    (width, false)
}

fn reorder_runs_recurse(levels: &[u8], start: usize, end: usize) -> VecDeque<usize> {
    let mut result = VecDeque::new();
    if start == end {
        return result;
    }
    let min_level = levels[start..end].iter().copied().min().unwrap_or(0);
    let mut level_start = start;
    for i in start..end {
        if levels[i] == min_level {
            if min_level % 2 == 1 {
                if i > level_start {
                    let mut inner = reorder_runs_recurse(levels, level_start, i);
                    inner.extend(result);
                    result = inner;
                }
                result.push_front(i);
            } else {
                if i > level_start {
                    result.extend(reorder_runs_recurse(levels, level_start, i));
                }
                result.push_back(i);
            }
            level_start = i + 1;
        }
    }
    if end > level_start {
        let inner = reorder_runs_recurse(levels, level_start, end);
        if min_level % 2 == 1 {
            let mut inner = inner;
            inner.extend(result);
            result = inner;
        } else {
            result.extend(inner);
        }
    }
    result
}

fn reorder_line(line: &mut LayoutLine) {
    let mut level_or = 0u8;
    let mut level_and = 1u8;
    for run in &line.runs {
        level_or |= run.item.analysis.level;
        level_and &= run.item.analysis.level;
    }
    let all_even = (level_or & 1) == 0;
    let all_odd = (level_and & 1) == 1;

    if !all_even && !all_odd {
        let levels: Vec<u8> = line
            .runs
            .iter()
            .map(|run| run.item.analysis.level)
            .collect();
        let order = reorder_runs_recurse(&levels, 0, levels.len());
        let mut slots: Vec<Option<GlyphItem>> = core::mem::take(&mut line.runs)
            .into_iter()
            .map(Some)
            .collect();
        line.runs = order
            .into_iter()
            .filter_map(|index| slots[index].take())
            .collect();
    } else if all_odd {
        line.runs.reverse();
    }
}

pub(crate) fn check_lines(layout: &mut Layout, ctx: &mut Context) {
    let mut attrs = layout.effective_attributes();
    let (shape_attrs, itemize_attrs) = match attrs.as_mut() {
        Some(attrs) => {
            let shape_attrs = attrs.filter(affects_break_or_shape);
            let itemize_attrs = attrs.filter(affects_itemization);
            (shape_attrs, itemize_attrs)
        }
        None => (None, None),
    };

    let need_log_attrs = layout.log_attrs.is_none();
    let log_attrs = layout
        .log_attrs
        .take()
        .unwrap_or_else(|| vec![LogAttr::default(); layout.n_chars as usize + 1]);

    let mut breaker = Breaker {
        tab_width: layout.tab_width,
        decimal: layout.decimal,
        layout,
        ctx,
        log_attrs,
        lines: Vec::new(),
        is_wrapped: false,
        is_ellipsized: false,
    };
    breaker.run(need_log_attrs, shape_attrs.as_ref(), itemize_attrs.as_ref());

    let Breaker {
        log_attrs,
        mut lines,
        tab_width,
        decimal,
        is_wrapped,
        is_ellipsized,
        ..
    } = breaker;

    if let Some(attrs) = attrs.as_ref() {
        let text = layout.text.as_str();
        for line in &mut lines {
            let runs = core::mem::take(&mut line.runs);
            line.runs = runs
                .into_iter()
                .flat_map(|run| run.apply_attrs(text, attrs))
                .collect();
        }
    }

    layout.log_attrs = Some(log_attrs);
    layout.lines = Some(lines);
    layout.tab_width = tab_width;
    layout.decimal = decimal;
    layout.is_wrapped = is_wrapped;
    layout.is_ellipsized = is_ellipsized;
}

impl Breaker<'_> {
    fn run(
        &mut self,
        need_log_attrs: bool,
        shape_attrs: Option<&AttrList>,
        itemize_attrs: Option<&AttrList>,
    ) {
        let layout = self.layout;
        let text = layout.text.as_str();
        let length = text.len();
        let mut iter: Option<AttrIterator<'_>> = itemize_attrs.map(AttrList::iterator);

        let mut start_offset = 0i32;
        let mut start = 0usize;
        let mut prev_base_dir = Direction::Neutral;
        let mut base_dir = Direction::Neutral;

        if layout.auto_dir {
            prev_base_dir = find_base_dir(text);
            if prev_base_dir == Direction::Neutral {
                prev_base_dir = self.ctx.base_dir();
            }
        } else {
            base_dir = self.ctx.base_dir();
        }

        let mut state = ParaBreakState {
            line_height: -1,
            remaining_height: layout.height,
            items: VecDeque::new(),
            base_dir,
            line_of_par: 1,
            glyphs: None,
            start_offset: 0,
            properties: ItemProperties::default(),
            log_widths: Vec::new(),
            log_widths_offset: 0,
            line_start_index: 0,
            line_start_offset: 0,
            line_width: -1,
            remaining_width: -1,
            hyphen_width: -1,
            baseline_shifts: Vec::new(),
            last_tab: LastTabState::reset(),
        };

        if layout.height >= 0 {
            let (logical, height) = empty_extents_and_height_at_index(layout, self.ctx, 0, true);
            state.line_height = if layout.line_spacing == 0.0 {
                logical.height as f32 as i32
            } else {
                (layout.line_spacing * height as f32) as i32
            };
        }

        loop {
            let (delimiter_index, next_para_index) = if layout.single_paragraph {
                (length, length)
            } else {
                find_paragraph_boundary(&text[start..])
            };

            if layout.auto_dir {
                base_dir = find_base_dir(&text[start..start + delimiter_index]);
                if base_dir == Direction::Neutral {
                    base_dir = prev_base_dir;
                } else {
                    prev_base_dir = base_dir;
                }
            }

            let end = start + delimiter_index;
            let delim_len = next_para_index - delimiter_index;
            let mut done = end == length;

            let mut items = itemize_with_font(
                self.ctx,
                base_dir,
                text,
                start,
                end - start,
                itemize_attrs,
                iter.as_mut(),
                None,
            );

            apply_attributes_to_items(&mut items, shape_attrs);

            if need_log_attrs {
                let offset = start_offset as usize;
                get_items_log_attrs(
                    text,
                    start,
                    delimiter_index + delim_len,
                    &items,
                    shape_attrs,
                    &mut self.log_attrs[offset..],
                );
            }

            itemize_post_process_items(self.ctx, text, Some(&self.log_attrs), &mut items);

            state.items = items.into();
            state.base_dir = base_dir;
            state.line_of_par = 1;
            state.start_offset = start_offset;
            state.line_start_offset = start_offset;
            state.line_start_index = start as i32;
            state.glyphs = None;
            state.line_width = -1;
            state.remaining_width = -1;
            state.log_widths_offset = 0;
            state.hyphen_width = -1;

            if state.items.is_empty() {
                let mut empty_line = LayoutLine::new(state.line_start_index, true);
                empty_line.resolved_dir = resolved_direction(base_dir, self.ctx.gravity());
                self.add_line(empty_line, &mut state);
            } else {
                while !state.items.is_empty() {
                    self.process_line(&mut state, itemize_attrs);
                }
            }

            if layout.height >= 0 && state.remaining_height < state.line_height {
                done = true;
            }

            if !done {
                start_offset +=
                    utf8_strlen_at(text, start as i32, ((end - start) + delim_len) as i32);
            }

            start = end + delim_len;
            if done {
                break;
            }
        }
    }

    fn can_break_at(&self, offset: i32, wrap: WrapMode) -> bool {
        if offset == self.layout.n_chars {
            return true;
        }
        let attr = self.log_attr(offset);
        match wrap {
            WrapMode::None => false,
            WrapMode::Char => attr.is_char_break(),
            WrapMode::Word | WrapMode::WordChar => attr.is_line_break(),
        }
    }

    fn can_break_in(&self, start_offset: i32, num_chars: i32, allow_break_at_start: bool) -> bool {
        let first = if allow_break_at_start { 0 } else { 1 };
        (first..num_chars).any(|i| self.can_break_at(start_offset + i, self.layout.wrap))
    }

    fn log_attr(&self, offset: i32) -> LogAttr {
        usize::try_from(offset)
            .ok()
            .and_then(|offset| self.log_attrs.get(offset).copied())
            .unwrap_or_default()
    }

    fn log_attrs_from(&self, offset: i32) -> &[LogAttr] {
        let offset = (offset.max(0) as usize).min(self.log_attrs.len());
        &self.log_attrs[offset..]
    }

    fn should_ellipsize_current_line(&self, state: &ParaBreakState) -> bool {
        let layout = self.layout;
        if layout.ellipsize == crate::types::EllipsizeMode::None || layout.width < 0 {
            return false;
        }
        if layout.wrap == WrapMode::None {
            return true;
        }
        if layout.height >= 0 {
            state.line_height * 2 > state.remaining_height
        } else {
            state.line_of_par == -layout.height
        }
    }

    fn ensure_tab_width(&mut self) {
        if self.tab_width != -1 {
            return;
        }
        let layout = self.layout;
        let mut font_desc = self.ctx.font_description().clone();
        let mut language = None;
        let mut shape = shape_flags::NONE;
        if self.ctx.round_glyph_positions() {
            shape |= shape_flags::ROUND_POSITIONS;
        }

        if let Some(layout_attrs) = layout.effective_attributes() {
            let iter = layout_attrs.iterator();
            iter.get_font(&mut font_desc, Some(&mut language), None);
        }

        let mut tmp_attrs = AttrList::new();
        tmp_attrs.insert_before(attr_font_desc_new(&font_desc));
        if let Some(language) = language {
            tmp_attrs.insert_before(attr_language_new(Some(language)));
        }

        let items = itemize(self.ctx, " ", 0, 1, Some(&tmp_attrs), None);
        let mut glyphs = GlyphString::new();
        if let Some(item) = items.first() {
            let eight_spaces = "        ";
            shape_with_flags(eight_spaces, 0, 8, &item.analysis, &mut glyphs, shape);
        }

        self.tab_width = glyphs.get_width();
        if self.tab_width <= 0 {
            self.tab_width = 50 * SCALE;
        }
    }

    fn ensure_decimal(&mut self) {
        if self.decimal == 0 {
            self.decimal = u32::from('.');
        }
    }

    fn tab_pos(&self, line: &LayoutLine, index: i32) -> (i32, TabAlign, u32, bool) {
        let layout = self.layout;
        let mut offset = 0;
        if layout.alignment != Alignment::Center {
            if line.is_paragraph_start && layout.indent >= 0 {
                offset = layout.indent;
            } else if !line.is_paragraph_start && layout.indent < 0 {
                offset = -layout.indent;
            }
        }

        let (n_tabs, in_pixels, is_default) = match layout.tabs.as_ref() {
            Some(tabs) => (tabs.size(), tabs.positions_in_pixels(), false),
            None => (0, false, true),
        };

        let (mut tab_pos, alignment, decimal) = if index < n_tabs {
            let tabs = layout.tabs.as_ref().expect("tab array with tabs");
            let (alignment, mut location) = tabs.tab(index).unwrap_or((TabAlign::Left, 0));
            if in_pixels {
                location *= SCALE;
            }
            (location, alignment, tabs.decimal_point(index))
        } else if n_tabs > 0 {
            let tabs = layout.tabs.as_ref().expect("tab array with tabs");
            let (alignment, mut last_pos) = tabs.tab(n_tabs - 1).unwrap_or((TabAlign::Left, 0));
            let decimal = tabs.decimal_point(n_tabs - 1);
            let mut next_to_last_pos = if n_tabs > 1 {
                tabs.tab(n_tabs - 2).map_or(0, |(_, location)| location)
            } else {
                0
            };
            if in_pixels {
                next_to_last_pos *= SCALE;
                last_pos *= SCALE;
            }
            let tab_width = if last_pos > next_to_last_pos {
                last_pos - next_to_last_pos
            } else {
                self.tab_width
            };
            (
                last_pos + tab_width * (index - n_tabs + 1),
                alignment,
                decimal,
            )
        } else {
            (self.tab_width * index, TabAlign::Left, 0)
        };

        tab_pos -= offset;
        (tab_pos, alignment, decimal, is_default)
    }

    fn shape_tab(
        &mut self,
        line: &LayoutLine,
        state: &mut ParaBreakState,
        current_width: i32,
    ) -> GlyphString {
        let mut glyphs = GlyphString::new();
        glyphs.set_size(1);
        let info = &mut glyphs.glyphs[0];
        info.glyph = if state.properties.showing_space {
            get_unknown_glyph(u32::from('\t'))
        } else {
            GLYPH_EMPTY
        };
        info.geometry.x_offset = 0;
        info.geometry.y_offset = 0;
        info.attr.is_cluster_start = true;
        info.attr.is_color = false;
        glyphs.log_clusters[0] = 0;

        self.ensure_tab_width();
        let space_width = self.tab_width / 8;

        let mut i = state.last_tab.index;
        let (tab_pos, tab_align, mut tab_decimal) = loop {
            let (tab_pos, tab_align, tab_decimal, is_default) = self.tab_pos(line, i);
            if tab_pos >= current_width + if is_default { space_width } else { 1 } {
                glyphs.glyphs[0].geometry.width = tab_pos - current_width;
                break (tab_pos, tab_align, tab_decimal);
            }
            i += 1;
        };

        if tab_decimal == 0 {
            self.ensure_decimal();
            tab_decimal = self.decimal;
        }

        state.last_tab = LastTabState {
            glyphs: TabGlyphs::Pending,
            index: i,
            width: current_width,
            pos: tab_pos,
            align: tab_align,
            decimal: tab_decimal,
        };
        glyphs
    }

    fn line_width(state: &ParaBreakState, line: &LayoutLine) -> i32 {
        if state.remaining_width > -1 {
            return state.line_width - state.remaining_width;
        }
        line.runs
            .iter()
            .flat_map(|run| run.glyphs.glyphs.iter())
            .fold(0i32, |width, glyph| {
                width.wrapping_add(glyph.geometry.width)
            })
    }

    fn tab_glyph_width(line: &LayoutLine, state: &ParaBreakState) -> Option<i32> {
        match state.last_tab.glyphs {
            TabGlyphs::Run(index) => line
                .runs
                .get(index)
                .and_then(|run| run.glyphs.glyphs.first())
                .map(|glyph| glyph.geometry.width),
            TabGlyphs::Absent | TabGlyphs::Pending => None,
        }
    }

    fn set_tab_glyph_width(line: &mut LayoutLine, state: &ParaBreakState, width: i32) {
        if let TabGlyphs::Run(index) = state.last_tab.glyphs
            && let Some(glyph) = line
                .runs
                .get_mut(index)
                .and_then(|run| run.glyphs.glyphs.first_mut())
        {
            glyph.geometry.width = width;
        }
    }

    fn shape_run(
        &mut self,
        line: &mut LayoutLine,
        state: &mut ParaBreakState,
        item: &Item,
    ) -> GlyphString {
        let layout = self.layout;
        let text = layout.text.as_str();

        if byte_at(text, item.offset) == b'\t' {
            let current_width = Self::line_width(state, line);
            return self.shape_tab(line, state, current_width);
        }

        let mut glyphs = GlyphString::new();
        let mut flags = shape_flags::NONE;
        if self.ctx.round_glyph_positions() {
            flags |= shape_flags::ROUND_POSITIONS;
        }

        if state.properties.shape_set {
            shape_shape(
                text,
                item.offset,
                item.num_chars,
                state.properties.shape_logical_rect.width,
                &mut glyphs,
            );
        } else {
            shape_item(
                item,
                text,
                Some(self.log_attrs_from(state.start_offset)),
                &mut glyphs,
                flags,
            );
        }

        if state.properties.letter_spacing != 0 {
            let mut glyph_item = GlyphItem::new(item.clone(), glyphs);
            glyph_item.letter_space(
                text,
                self.log_attrs_from(state.start_offset),
                state.properties.letter_spacing,
            );
            glyphs = glyph_item.glyphs;
        }

        if state.properties.word_spacing != 0 {
            let mut glyph_item = GlyphItem::new(item.clone(), glyphs);
            glyph_item.word_space(text, state.properties.word_spacing);
            glyphs = glyph_item.glyphs;
        }

        if matches!(state.last_tab.glyphs, TabGlyphs::Run(_)) {
            let mut w = state.last_tab.pos - state.last_tab.width;
            match state.last_tab.align {
                TabAlign::Right => w -= glyphs.get_width(),
                TabAlign::Center => w -= glyphs.get_width() / 2,
                TabAlign::Decimal => {
                    let glyph_item = GlyphItem::new(item.clone(), glyphs.clone());
                    let (width, _) =
                        decimal_prefix_width(&glyph_item, text, state.last_tab.decimal);
                    w -= width;
                }
                TabAlign::Left => {}
            }
            Self::set_tab_glyph_width(line, state, w.max(0));
        }

        glyphs
    }

    fn insert_run(
        &mut self,
        line: &mut LayoutLine,
        state: &mut ParaBreakState,
        run_item: Item,
        glyphs: Option<GlyphString>,
        last_run: bool,
    ) {
        let glyphs = match glyphs {
            Some(glyphs) => glyphs,
            None => {
                if last_run
                    && state.log_widths_offset == 0
                    && state.glyphs.is_some()
                    && run_item.analysis.flags & analysis_flags::NEED_HYPHEN == 0
                {
                    state.glyphs.take().unwrap_or_default()
                } else {
                    self.shape_run(line, state, &run_item)
                }
            }
        };

        if last_run {
            state.glyphs = None;
        }

        line.length += run_item.length;
        line.runs.push(GlyphItem::new(run_item, glyphs));
        let run_index = line.runs.len() - 1;

        match state.last_tab.glyphs {
            TabGlyphs::Pending => {
                state.last_tab.glyphs = TabGlyphs::Run(run_index);
            }
            TabGlyphs::Run(tab_index) if tab_index != run_index => {
                let text = self.layout.text.as_str();
                let run = &line.runs[run_index];
                let mut found_decimal = false;
                match state.last_tab.align {
                    TabAlign::Right => state.last_tab.width += run.glyphs.get_width(),
                    TabAlign::Center => state.last_tab.width += run.glyphs.get_width() / 2,
                    TabAlign::Decimal => {
                        let (width, found) =
                            decimal_prefix_width(run, text, state.last_tab.decimal);
                        found_decimal = found;
                        state.last_tab.width += width;
                    }
                    TabAlign::Left => {}
                }
                let width = (state.last_tab.pos - state.last_tab.width).max(0);
                Self::set_tab_glyph_width(line, state, width);
                if found_decimal || width == 0 {
                    state.last_tab.glyphs = TabGlyphs::Absent;
                }
            }
            TabGlyphs::Run(_) | TabGlyphs::Absent => {}
        }
    }

    fn break_needs_hyphen(&self, state: &ParaBreakState, pos: i32) -> bool {
        let attr = self.log_attr(state.start_offset + pos);
        attr.break_inserts_hyphen() || attr.break_removes_preceding()
    }

    fn log_width(state: &ParaBreakState, index: i32) -> i32 {
        usize::try_from(index)
            .ok()
            .and_then(|index| state.log_widths.get(index).copied())
            .unwrap_or(0)
    }

    fn find_break_extra_width(&self, state: &mut ParaBreakState, item: &Item, pos: i32) -> i32 {
        let attr = self.log_attr(state.start_offset + pos);
        if attr.break_inserts_hyphen() {
            if state.hyphen_width < 0 {
                state.hyphen_width = find_hyphen_width(item);
            }
            if attr.break_removes_preceding() && pos > 0 {
                return state.hyphen_width
                    - Self::log_width(state, state.log_widths_offset + pos - 1);
            }
            return state.hyphen_width;
        }
        if pos > 0 && self.log_attr(state.start_offset + pos - 1).is_white() {
            return -Self::log_width(state, state.log_widths_offset + pos - 1);
        }
        0
    }

    fn compute_log_widths(&self, state: &mut ParaBreakState, item: &mut Item) {
        let num_chars = item.num_chars.max(0) as usize;
        if num_chars > state.log_widths.len() {
            state.log_widths.resize(num_chars, 0);
        }
        let text = self.layout.text.as_str();
        let mut glyphs = state.glyphs.take().unwrap_or_default();
        let log_widths = &mut state.log_widths;
        with_glyph_item(item, &mut glyphs, |glyph_item| {
            glyph_item.get_logical_widths(text, log_widths);
        });
        state.glyphs = Some(glyphs);
    }

    fn tab_width_change(line: &LayoutLine, state: &ParaBreakState) -> i32 {
        match Self::tab_glyph_width(line, state) {
            Some(width) => width - (state.last_tab.pos - state.last_tab.width),
            None => 0,
        }
    }

    #[allow(clippy::too_many_lines)]
    fn process_item(
        &mut self,
        line: &mut LayoutLine,
        state: &mut ParaBreakState,
        mut item: Item,
        force_fit: bool,
        no_break_at_end: bool,
        is_last_item: bool,
    ) -> (BreakResult, Option<Item>) {
        let layout = self.layout;
        let text = layout.text.as_str();

        let mut processing_new_item = false;
        if state.glyphs.is_none() {
            state.properties = ItemProperties::of(&item);
            let glyphs = self.shape_run(line, state, &item);
            state.glyphs = Some(glyphs);
            state.log_widths_offset = 0;
            processing_new_item = true;
        }

        let num_chars = item.num_chars;

        if !layout.single_paragraph
            && char_at(text, item.offset) == LINE_SEPARATOR
            && !self.should_ellipsize_current_line(state)
        {
            self.insert_run(line, state, item, None, true);
            state.log_widths_offset += num_chars;
            return (BreakResult::LineSeparator, None);
        }

        if state.remaining_width < 0 && !no_break_at_end {
            self.insert_run(line, state, item, None, true);
            return (BreakResult::AllFit, None);
        }

        let mut width = if processing_new_item {
            state.glyphs.as_ref().map_or(0, GlyphString::get_width)
        } else {
            (0..num_chars).fold(0, |width, i| {
                width + Self::log_width(state, state.log_widths_offset + i)
            })
        };

        if byte_at(text, item.offset) == b'\t' {
            self.insert_run(line, state, item, None, true);
            state.remaining_width -= width;
            state.remaining_width = state.remaining_width.max(0);
            return (BreakResult::AllFit, None);
        }

        let mut wrap = layout.wrap;
        let mut extra_width;

        if !no_break_at_end && self.can_break_at(state.start_offset + num_chars, wrap) {
            if processing_new_item {
                self.compute_log_widths(state, &mut item);
                processing_new_item = false;
            }
            extra_width = if !is_last_item {
                self.find_break_extra_width(state, &item, num_chars)
            } else {
                0
            };
        } else {
            extra_width = 0;
        }

        let tab_not_left = !matches!(state.last_tab.glyphs, TabGlyphs::Absent)
            && state.last_tab.align != TabAlign::Left;
        if (width + extra_width <= state.remaining_width
            || (num_chars == 1 && line.runs.is_empty())
            || tab_not_left)
            && !no_break_at_end
        {
            let glyphs = self.shape_run(line, state, &item);
            width = glyphs.get_width() + Self::tab_width_change(line, state);

            if width + extra_width <= state.remaining_width
                || (num_chars == 1 && line.runs.is_empty())
            {
                self.insert_run(line, state, item, Some(glyphs), true);
                state.remaining_width -= width;
                state.remaining_width = state.remaining_width.max(0);
                return (BreakResult::AllFit, None);
            }
        }

        let orig_width = width;
        let orig_extra_width = extra_width;
        let mut break_width = width;
        let mut break_extra_width = extra_width;
        let mut break_num_chars = num_chars;
        wrap = layout.wrap;
        let mut break_glyphs: Option<GlyphString> = None;

        let metrics = metrics_of(item.analysis.font.as_ref(), item.analysis.language);
        let safe_distance = metrics.approximate_char_width * 3;

        if processing_new_item {
            self.compute_log_widths(state, &mut item);
        }

        loop {
            width = 0;
            let limit = if no_break_at_end {
                num_chars
            } else {
                num_chars + 1
            };
            for i in 0..limit {
                extra_width = if !is_last_item || i != num_chars {
                    self.find_break_extra_width(state, &item, i)
                } else {
                    0
                };

                if (width + extra_width).min(width) > state.remaining_width + safe_distance
                    && break_num_chars < num_chars
                {
                    break;
                }

                if self.can_break_at(state.start_offset + i, wrap)
                    && (i > 0 || !line.runs.is_empty())
                {
                    if i == 0 || width + extra_width < state.remaining_width - safe_distance {
                        break_num_chars = i;
                        break_width = width;
                        break_extra_width = extra_width;
                    } else {
                        let length = byte_length_of_chars(text, item.offset, i);
                        let split = i < num_chars;
                        let glyphs = if split {
                            let mut new_item = item.split(length, i).expect("split inside item");
                            if self.break_needs_hyphen(state, i) {
                                new_item.analysis.flags |= analysis_flags::NEED_HYPHEN;
                            } else {
                                new_item.analysis.flags &= !analysis_flags::NEED_HYPHEN;
                            }
                            let glyphs = self.shape_run(line, state, &new_item);
                            item.unsplit(length, i);
                            glyphs
                        } else {
                            self.shape_run(line, state, &item)
                        };

                        let new_break_width =
                            glyphs.get_width() + Self::tab_width_change(line, state);

                        extra_width = if i > 0
                            && (split || !is_last_item)
                            && self.log_attr(state.start_offset + i - 1).is_white()
                        {
                            -Self::log_width(state, state.log_widths_offset + i - 1)
                        } else if !split && !is_last_item && self.break_needs_hyphen(state, i) {
                            state.hyphen_width
                        } else {
                            0
                        };

                        if break_num_chars == num_chars
                            || new_break_width + extra_width <= state.remaining_width
                            || new_break_width + extra_width < break_width + break_extra_width
                        {
                            break_num_chars = i;
                            break_width = new_break_width;
                            break_extra_width = extra_width;
                            break_glyphs = Some(glyphs);
                        }
                    }
                }

                if i < num_chars {
                    width += Self::log_width(state, state.log_widths_offset + i);
                }
            }

            if wrap == WrapMode::WordChar
                && force_fit
                && break_width + break_extra_width > state.remaining_width
            {
                wrap = WrapMode::Char;
                break_num_chars = num_chars;
                break_width = orig_width;
                break_extra_width = orig_extra_width;
                break_glyphs = None;
                continue;
            }
            break;
        }

        if force_fit || break_width + break_extra_width <= state.remaining_width {
            if state.remaining_width >= 0 {
                state.remaining_width -= break_width + break_extra_width;
                state.remaining_width = state.remaining_width.max(0);
            }

            if break_num_chars == num_chars {
                if self.can_break_at(state.start_offset + break_num_chars, wrap)
                    && self.break_needs_hyphen(state, break_num_chars)
                {
                    item.analysis.flags |= analysis_flags::NEED_HYPHEN;
                }
                self.insert_run(line, state, item, None, true);
                (BreakResult::AllFit, None)
            } else if break_num_chars == 0 {
                (BreakResult::EmptyFit, Some(item))
            } else {
                let length = byte_length_of_chars(text, item.offset, break_num_chars);
                let new_item = item
                    .split(length, break_num_chars)
                    .expect("split inside item");
                self.insert_run(line, state, new_item, break_glyphs, false);
                state.log_widths_offset += break_num_chars;
                (BreakResult::SomeFit, Some(item))
            }
        } else {
            state.glyphs = None;
            (BreakResult::NoneFit, Some(item))
        }
    }

    fn process_line(&mut self, state: &mut ParaBreakState, itemize_attrs: Option<&AttrList>) {
        let layout = self.layout;
        let text = layout.text.as_str();
        let mut line = LayoutLine::new(state.line_start_index, state.line_of_par == 1);
        line.resolved_dir = resolved_direction(state.base_dir, self.ctx.gravity());

        let mut have_break = false;
        let mut break_remaining_width = 0;
        let mut break_start_offset = 0;
        let mut break_run_count = 0usize;
        let mut wrapped = false;

        state.line_width = layout.width;
        if state.line_width >= 0 && layout.alignment != Alignment::Center {
            if line.is_paragraph_start && layout.indent >= 0 {
                state.line_width -= layout.indent;
            } else if !line.is_paragraph_start && layout.indent < 0 {
                state.line_width += layout.indent;
            }
            if state.line_width < 0 {
                state.line_width = 0;
            }
        }

        state.remaining_width = if self.should_ellipsize_current_line(state) {
            -1
        } else {
            state.line_width
        };

        state.last_tab.glyphs = TabGlyphs::Absent;
        state.last_tab.index = 0;
        state.last_tab.align = TabAlign::Left;

        while let Some(front) = state.items.front() {
            let old_num_chars = front.num_chars;
            let is_tab = byte_at(text, front.offset) == b'\t';
            let old_remaining_width = state.remaining_width;
            let first_item_in_line = line.runs.is_empty();
            let last_item_in_line = state.items.len() == 1;
            let item = state.items.pop_front().expect("front item");

            let (result, rest) = self.process_item(
                &mut line,
                state,
                item,
                !have_break,
                false,
                last_item_in_line,
            );

            match result {
                BreakResult::AllFit => {
                    if !is_tab
                        && self.can_break_in(state.start_offset, old_num_chars, !first_item_in_line)
                    {
                        have_break = true;
                        break_remaining_width = old_remaining_width;
                        break_start_offset = state.start_offset;
                        break_run_count = line.runs.len().saturating_sub(1);
                    }
                    state.start_offset += old_num_chars;
                }
                BreakResult::EmptyFit => {
                    if let Some(rest) = rest {
                        state.items.push_front(rest);
                    }
                    wrapped = true;
                    break;
                }
                BreakResult::SomeFit => {
                    if let Some(rest) = rest {
                        state.start_offset += old_num_chars - rest.num_chars;
                        state.items.push_front(rest);
                    }
                    wrapped = true;
                    break;
                }
                BreakResult::NoneFit => {
                    if let Some(rest) = rest {
                        state.items.push_front(rest);
                    }
                    while line.runs.len() > break_run_count {
                        let run_index = line.runs.len() - 1;
                        if state.last_tab.glyphs == TabGlyphs::Run(run_index) {
                            state.last_tab.glyphs = TabGlyphs::Absent;
                            state.last_tab.index = 0;
                            state.last_tab.align = TabAlign::Left;
                        }
                        let run = line.runs.pop().expect("run to uninsert");
                        line.length -= run.item.length;
                        state.items.push_front(run.item);
                    }

                    state.start_offset = break_start_offset;
                    state.remaining_width = break_remaining_width;
                    let last_item_in_line = state.items.len() == 1;

                    let item = state.items.pop_front().expect("item to rebreak");
                    let old_num_chars = item.num_chars;
                    let (result, rest) =
                        self.process_item(&mut line, state, item, true, true, last_item_in_line);
                    match result {
                        BreakResult::AllFit => {
                            state.start_offset += old_num_chars;
                        }
                        BreakResult::EmptyFit => {
                            wrapped = true;
                        }
                        BreakResult::SomeFit => {
                            if let Some(rest) = rest.as_ref() {
                                state.start_offset += old_num_chars - rest.num_chars;
                            }
                            wrapped = true;
                        }
                        BreakResult::NoneFit | BreakResult::LineSeparator => {}
                    }
                    if let Some(rest) = rest {
                        state.items.push_front(rest);
                    }
                    break;
                }
                BreakResult::LineSeparator => {
                    state.start_offset += old_num_chars;
                    wrapped = true;
                    break;
                }
            }
        }

        self.postprocess(&mut line, state, wrapped, itemize_attrs);
        let length = line.length;
        self.add_line(line, state);
        state.line_of_par += 1;
        state.line_start_index += length;
        state.line_start_offset = state.start_offset;
    }

    fn add_line(&mut self, line: LayoutLine, state: &mut ParaBreakState) {
        if self.layout.height >= 0 {
            let (_, logical, _) = line_extents_and_height(self.layout, self.ctx, &line);
            state.remaining_height -= logical.height;
            state.remaining_height -= self.layout.spacing;
            state.line_height = logical.height;
        }
        self.lines.push(line);
    }

    fn add_missing_hyphen(&mut self, line: &mut LayoutLine, state: &mut ParaBreakState) {
        let Some(run_index) = line.runs.len().checked_sub(1) else {
            return;
        };
        let line_chars: i32 = line.runs.iter().map(|run| run.item.num_chars).sum();
        let item = &line.runs[run_index].item;

        if self
            .log_attr(state.line_start_offset + line_chars)
            .break_inserts_hyphen()
            && item.analysis.flags & analysis_flags::NEED_HYPHEN == 0
        {
            let width = line.runs[run_index].glyphs.get_width();
            let start_offset = state.start_offset;
            state.start_offset = state.line_start_offset + line_chars - item.num_chars;

            line.runs[run_index].item.analysis.flags |= analysis_flags::NEED_HYPHEN;
            let item = line.runs[run_index].item.clone();
            let glyphs = self.shape_run(line, state, &item);
            if state.last_tab.glyphs == TabGlyphs::Pending {
                state.last_tab.glyphs = TabGlyphs::Run(run_index);
            }
            line.runs[run_index].glyphs = glyphs;

            state.start_offset = start_offset;
            state.remaining_width += line.runs[run_index].glyphs.get_width() - width;
        }
    }

    fn zero_line_final_space(&self, line: &mut LayoutLine, state: &ParaBreakState) {
        let Some(run) = line.runs.last_mut() else {
            return;
        };
        let rtl = run.item.analysis.level % 2 == 1;
        let num_glyphs = run.glyphs.glyphs.len();
        if num_glyphs == 0 {
            return;
        }
        let glyph = if rtl { 0 } else { num_glyphs - 1 };

        if run.glyphs.glyphs[glyph].glyph == get_unknown_glyph(LINE_SEPARATOR)
            && find_show_flags(&run.item) & show_flags::LINE_BREAKS != 0
        {
            return;
        }

        if state.start_offset == 0 || !self.log_attr(state.start_offset - 1).is_white() {
            return;
        }

        if num_glyphs >= 2 {
            let neighbour = if rtl { glyph + 1 } else { glyph - 1 };
            if run.glyphs.log_clusters[glyph] == run.glyphs.log_clusters[neighbour] {
                return;
            }
        }

        let info = &mut run.glyphs.glyphs[glyph];
        info.geometry.width = 0;
        info.glyph = GLYPH_EMPTY;
    }

    fn collect_baseline_shift(
        state: &mut ParaBreakState,
        item: &Item,
        prev: Option<&Item>,
    ) -> (i32, i32, i32, i32) {
        let mut start_x_offset = 0;
        let mut start_y_offset = 0;
        let mut end_x_offset = 0;
        let mut end_y_offset = 0;

        for attr in &item.analysis.extra_attrs {
            match attr.value {
                AttrValue::Rise(value) => {
                    start_y_offset += value;
                    end_y_offset -= value;
                }
                AttrValue::BaselineShift(value) => {
                    if attr.start_index as i32 == item.offset {
                        let mut entry = BaselineItem {
                            start_index: attr.start_index,
                            end_index: attr.end_index,
                            value,
                            x_offset: 0,
                            y_offset: 0,
                        };

                        if !(-1024..=1024).contains(&value) {
                            entry.y_offset = value;
                        } else {
                            let mut superscript_x_offset = 0;
                            let mut superscript_y_offset = 0;
                            let mut subscript_x_offset = 0;
                            let mut subscript_y_offset = 0;
                            if let Some(font) = prev.and_then(|prev| prev.analysis.font.as_ref()) {
                                let position = |name: &[u8; 4]| font.ot_metric(tag(name));
                                if let Some(p) = position(b"spyo") {
                                    superscript_y_offset = p;
                                }
                                if let Some(p) = position(b"spxo") {
                                    superscript_x_offset = p;
                                }
                                if let Some(p) = position(b"sbyo") {
                                    subscript_y_offset = p;
                                }
                                if let Some(p) = position(b"sbxo") {
                                    subscript_x_offset = p;
                                }
                            }
                            if superscript_y_offset == 0 {
                                superscript_y_offset = 5000;
                            }
                            if subscript_y_offset == 0 {
                                subscript_y_offset = 5000;
                            }
                            match BaselineShift::from_i32(value) {
                                Some(BaselineShift::Superscript) => {
                                    entry.x_offset = superscript_x_offset;
                                    entry.y_offset = superscript_y_offset;
                                }
                                Some(BaselineShift::Subscript) => {
                                    entry.x_offset = subscript_x_offset;
                                    entry.y_offset = -subscript_y_offset;
                                }
                                Some(BaselineShift::None) | None => {
                                    entry.x_offset = 0;
                                    entry.y_offset = 0;
                                }
                            }
                        }

                        state.baseline_shifts.insert(0, entry);
                        start_x_offset += entry.x_offset;
                        start_y_offset += entry.y_offset;
                    }

                    if attr.end_index as i32 == item.offset + item.length
                        && !state.baseline_shifts.is_empty()
                    {
                        let entry = state.baseline_shifts.remove(0);
                        if attr.start_index == entry.start_index
                            && attr.end_index == entry.end_index
                            && value == entry.value
                        {
                            end_x_offset -= entry.x_offset;
                            end_y_offset -= entry.y_offset;
                        }
                    }
                }
                _ => {}
            }
        }

        (start_x_offset, start_y_offset, end_x_offset, end_y_offset)
    }

    fn apply_baseline_shift(line: &mut LayoutLine, state: &mut ParaBreakState) {
        let mut y_offset = 0;
        let mut prev: Option<usize> = None;
        for index in 0..line.runs.len() {
            if line.runs[index].item.analysis.font.is_none() {
                let run = &mut line.runs[index];
                run.y_offset = 0;
                run.start_x_offset = 0;
                run.end_x_offset = 0;
                continue;
            }
            let (start_x_offset, start_y_offset, end_x_offset, end_y_offset) = {
                let item = &line.runs[index].item;
                let prev_item = prev.map(|prev| &line.runs[prev].item);
                Self::collect_baseline_shift(state, item, prev_item)
            };
            y_offset += start_y_offset;
            let run = &mut line.runs[index];
            run.y_offset = y_offset;
            run.start_x_offset = start_x_offset;
            run.end_x_offset = end_x_offset;
            y_offset += end_y_offset;
            prev = Some(index);
        }
    }

    fn justify_clusters(&self, line: &mut LayoutLine, state: &mut ParaBreakState) {
        let text = self.layout.text.as_str();
        let total_remaining_width = state.remaining_width;
        if total_remaining_width <= 0 {
            return;
        }
        let is_hinted = (total_remaining_width & (SCALE - 1)) == 0;
        let mut total_gaps = 0;
        let mut added_so_far = 0;

        for adjust in [false, true] {
            let mut leftedge = true;
            let mut rightmost: Option<(usize, i32)> = None;
            let mut residual = 0;
            added_so_far = 0;
            let mut gaps_so_far = 0;

            for run_index in 0..line.runs.len() {
                let dir: i32 = if line.runs[run_index].item.analysis.level.is_multiple_of(2) {
                    1
                } else {
                    -1
                };
                let offset = line.runs[run_index].item.char_offset;
                let mut cluster_iter = GlyphItemIter::default();
                let mut have_cluster = if dir > 0 {
                    cluster_iter.init_start(&line.runs[run_index], text)
                } else {
                    cluster_iter.init_end(&line.runs[run_index], text)
                };
                while have_cluster {
                    let cursor = self.log_attr(offset + cluster_iter.start_char);
                    let glyphs = &mut line.runs[run_index].glyphs;
                    if cursor.is_cursor_position() {
                        let mut width = 0;
                        let mut i = cluster_iter.start_glyph;
                        while i != cluster_iter.end_glyph {
                            width += glyphs.glyphs[i as usize].geometry.width;
                            i += dir;
                        }

                        if width != 0 {
                            gaps_so_far += 1;
                            if adjust {
                                let mut adjustment = total_remaining_width / total_gaps + residual;
                                if is_hinted {
                                    let old_adjustment = adjustment;
                                    adjustment = units_round(adjustment);
                                    residual = old_adjustment - adjustment;
                                }
                                let (space_left, space_right) =
                                    distribute_letter_spacing(adjustment);
                                let (leftmost, rightmost_glyph) =
                                    if cluster_iter.start_glyph < cluster_iter.end_glyph {
                                        (cluster_iter.start_glyph, cluster_iter.end_glyph - 1)
                                    } else {
                                        (cluster_iter.end_glyph + 1, cluster_iter.start_glyph)
                                    };
                                if leftedge {
                                    leftedge = false;
                                } else {
                                    let geometry = &mut glyphs.glyphs[leftmost as usize].geometry;
                                    geometry.width += space_left;
                                    geometry.x_offset += space_left;
                                    added_so_far += space_left;
                                }
                                rightmost = Some((run_index, space_right));
                                glyphs.glyphs[rightmost_glyph as usize].geometry.width +=
                                    space_right;
                                added_so_far += space_right;
                            }
                        }
                    }
                    have_cluster = if dir > 0 {
                        cluster_iter.next_cluster(&line.runs[run_index], text)
                    } else {
                        cluster_iter.prev_cluster(&line.runs[run_index], text)
                    };
                }
            }

            if !adjust {
                total_gaps = gaps_so_far - 1;
                if total_gaps == 0 {
                    return;
                }
            } else if let Some((run_index, space)) = rightmost {
                let glyphs = &mut line.runs[run_index].glyphs.glyphs;
                if let Some(last) = glyphs.last_mut() {
                    last.geometry.width -= space;
                }
                added_so_far -= space;
            }
        }

        state.remaining_width -= added_so_far;
    }

    fn justify_words(&self, line: &mut LayoutLine, state: &mut ParaBreakState) {
        let text = self.layout.text.as_str();
        let total_remaining_width = state.remaining_width;
        if total_remaining_width <= 0 {
            return;
        }
        let is_hinted = (total_remaining_width & (SCALE - 1)) == 0;
        let mut total_space_width = 0;
        let mut added_so_far = 0;

        for adjust in [false, true] {
            added_so_far = 0;
            let mut spaces_so_far = 0i32;

            for run_index in 0..line.runs.len() {
                let offset = line.runs[run_index].item.char_offset;
                let mut cluster_iter = GlyphItemIter::default();
                let mut have_cluster = cluster_iter.init_start(&line.runs[run_index], text);
                while have_cluster {
                    if self
                        .log_attr(offset + cluster_iter.start_char)
                        .is_expandable_space()
                    {
                        let dir = if cluster_iter.start_glyph < cluster_iter.end_glyph {
                            1
                        } else {
                            -1
                        };
                        let glyphs = &mut line.runs[run_index].glyphs;
                        let mut i = cluster_iter.start_glyph;
                        while i != cluster_iter.end_glyph {
                            let glyph_width = glyphs.glyphs[i as usize].geometry.width;
                            if glyph_width != 0 {
                                spaces_so_far += glyph_width;
                                if adjust {
                                    let product = (spaces_so_far as u64)
                                        .wrapping_mul(total_remaining_width as u64);
                                    let mut adjustment = (product / (total_space_width as u64))
                                        .wrapping_sub(added_so_far as u64)
                                        as i32;
                                    if is_hinted {
                                        adjustment = units_round(adjustment);
                                    }
                                    glyphs.glyphs[i as usize].geometry.width += adjustment;
                                    added_so_far += adjustment;
                                }
                            }
                            i += dir;
                        }
                    }
                    have_cluster = cluster_iter.next_cluster(&line.runs[run_index], text);
                }
            }

            if !adjust {
                total_space_width = spaces_so_far;
                if total_space_width == 0 {
                    self.justify_clusters(line, state);
                    return;
                }
            }
        }

        state.remaining_width -= added_so_far;
    }

    fn postprocess(
        &mut self,
        line: &mut LayoutLine,
        state: &mut ParaBreakState,
        wrapped: bool,
        itemize_attrs: Option<&AttrList>,
    ) {
        self.add_missing_hyphen(line, state);

        if wrapped {
            self.zero_line_final_space(line, state);
        }

        Self::apply_baseline_shift(line, state);

        let mut ellipsized = false;
        if state.line_width >= 0 && self.should_ellipsize_current_line(state) {
            let mut flags = shape_flags::NONE;
            if self.ctx.round_glyph_positions() {
                flags |= shape_flags::ROUND_POSITIONS;
            }
            ellipsized = ellipsize_line(
                self.ctx,
                self.layout.text.as_str(),
                &self.log_attrs,
                self.layout.ellipsize,
                line,
                itemize_attrs,
                flags,
                state.line_width,
            );
        }

        reorder_line(line);

        if self.layout.justify && (wrapped || ellipsized || self.layout.justify_last_line) {
            if state.remaining_width < 0 {
                state.remaining_width = state.line_width - line.width();
            }
            self.justify_words(line, state);
        }

        self.is_wrapped |= wrapped;
        self.is_ellipsized |= ellipsized;
    }
}
