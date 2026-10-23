//! Southstar text — ellipsization: growing a gap from the start, middle or end of a line and replacing it with an ellipsis run.
//! Copyright 2004 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::attrs::{AttrIterator, AttrList, AttrType, attr_fallback_new};
use crate::context::Context;
use crate::glyph::GlyphString;
use crate::glyph_item::{GlyphItem, GlyphItemIter};
use crate::item::{Item, analysis_flags};
use crate::itemize::itemize;
use crate::layout::LayoutLine;
use crate::shape::shape_with_flags;
use crate::types::{EllipsizeMode, LogAttr};
use crate::unicode::is_wide;
use crate::utf8::{char_at, pango_utf8_strlen};

#[derive(Clone, Copy, Debug, Default)]
struct LineIter {
    run_iter: GlyphItemIter,
    run_index: usize,
}

struct RunInfo {
    start_offset: i32,
    width: i32,
}

struct EllipsizeState<'a, 'c> {
    ctx: &'c mut Context,
    text: &'a str,
    log_attrs: &'a [LogAttr],
    attrs: &'a AttrList,
    runs: Vec<GlyphItem>,
    run_info: Vec<RunInfo>,
    total_width: i32,
    gap_center: i32,
    ellipsis_run: Option<GlyphItem>,
    ellipsis_width: i32,
    ellipsis_is_cjk: bool,
    line_start_attr: Option<AttrIterator<'a>>,
    gap_start_iter: LineIter,
    gap_start_x: i32,
    gap_start_attr: Option<AttrIterator<'a>>,
    gap_end_iter: LineIter,
    gap_end_x: i32,
    shape_flags: u32,
}

fn utf8_strlen(text: &str, start: i32, length: i32) -> i32 {
    let start = usize::try_from(start).unwrap_or(0).min(text.len());
    pango_utf8_strlen(
        &text.as_bytes()[start..],
        usize::try_from(length).unwrap_or(0),
    )
}

fn advance_iterator_to(iter: &mut AttrIterator<'_>, new_index: i32) {
    loop {
        let (_, end) = iter.range();
        if end > new_index {
            break;
        }
        if !iter.next() {
            break;
        }
    }
}

fn itemize_text(ctx: &mut Context, text: &str, attrs: &AttrList) -> Item {
    itemize(ctx, text, 0, text.len(), Some(attrs), None)
        .into_iter()
        .next()
        .unwrap_or_default()
}

fn is_ellipsis_font_attr(kind: AttrType) -> bool {
    matches!(
        kind,
        AttrType::Language
            | AttrType::Family
            | AttrType::Style
            | AttrType::Weight
            | AttrType::Variant
            | AttrType::Stretch
            | AttrType::Size
            | AttrType::FontDesc
            | AttrType::Scale
            | AttrType::LetterSpacing
            | AttrType::AbsoluteSize
            | AttrType::LineHeight
            | AttrType::AbsoluteLineHeight
    )
}

impl EllipsizeState<'_, '_> {
    fn cluster_width(&self, iter: &LineIter) -> i32 {
        let glyphs = &self.runs[iter.run_index].glyphs.glyphs;
        let run_iter = &iter.run_iter;
        let mut width = 0;
        if run_iter.start_glyph < run_iter.end_glyph {
            for glyph in &glyphs[run_iter.start_glyph as usize..run_iter.end_glyph as usize] {
                width += glyph.geometry.width;
            }
        } else {
            let mut i = run_iter.start_glyph;
            while i > run_iter.end_glyph {
                width += glyphs[i as usize].geometry.width;
                i -= 1;
            }
        }
        width
    }

    fn next_cluster(&self, iter: &mut LineIter) -> bool {
        if !iter
            .run_iter
            .next_cluster(&self.runs[iter.run_index], self.text)
        {
            if iter.run_index == self.runs.len() - 1 {
                return false;
            }
            iter.run_index += 1;
            iter.run_iter
                .init_start(&self.runs[iter.run_index], self.text);
        }
        true
    }

    fn prev_cluster(&self, iter: &mut LineIter) -> bool {
        if !iter
            .run_iter
            .prev_cluster(&self.runs[iter.run_index], self.text)
        {
            if iter.run_index == 0 {
                return false;
            }
            iter.run_index -= 1;
            iter.run_iter
                .init_end(&self.runs[iter.run_index], self.text);
        }
        true
    }

    fn cursor_position(&self, offset: i32) -> bool {
        usize::try_from(offset)
            .ok()
            .and_then(|offset| self.log_attrs.get(offset))
            .is_some_and(|attr| attr.is_cursor_position())
    }

    fn starts_at_ellipsization_boundary(&self, iter: &LineIter) -> bool {
        if iter.run_iter.start_char == 0 && iter.run_index == 0 {
            return true;
        }
        let info = &self.run_info[iter.run_index];
        self.cursor_position(info.start_offset + iter.run_iter.start_char)
    }

    fn ends_at_ellipsization_boundary(&self, iter: &LineIter) -> bool {
        let info = &self.run_info[iter.run_index];
        if iter.run_iter.end_char == self.runs[iter.run_index].item.num_chars
            && iter.run_index == self.runs.len() - 1
        {
            return true;
        }
        self.cursor_position(info.start_offset + iter.run_iter.end_char + 1)
    }

    fn ellipsis_attrs(&self, fallback: bool) -> AttrList {
        let mut attrs = AttrList::new();
        if let Some(gap_start_attr) = self.gap_start_attr.as_ref() {
            for mut attr in gap_start_attr.get_attrs() {
                if is_ellipsis_font_attr(attr.kind()) {
                    attr.start_index = 0;
                    attr.end_index = i32::MAX as u32;
                    attrs.insert(attr);
                }
            }
        }
        attrs.insert(attr_fallback_new(fallback).with_range(0, i32::MAX as u32));
        attrs
    }

    fn shape_ellipsis(&mut self) {
        let attrs = self.ellipsis_attrs(false);
        let mut ellipsis_text = if self.ellipsis_is_cjk {
            "\u{22EF}"
        } else {
            "\u{2026}"
        };

        let mut item = itemize_text(self.ctx, ellipsis_text, &attrs);
        let first_char = char_at(ellipsis_text.as_bytes(), 0);
        if !item
            .analysis
            .font
            .as_ref()
            .is_some_and(|font| font.has_char(first_char))
        {
            let attrs = self.ellipsis_attrs(true);
            ellipsis_text = "...";
            item = itemize_text(self.ctx, ellipsis_text, &attrs);
        }

        let run = self
            .ellipsis_run
            .get_or_insert_with(|| GlyphItem::new(Item::new(), GlyphString::new()));
        shape_with_flags(
            ellipsis_text,
            0,
            ellipsis_text.len(),
            &item.analysis,
            &mut run.glyphs,
            self.shape_flags,
        );
        run.item = item;

        self.ellipsis_width = run
            .glyphs
            .glyphs
            .iter()
            .fold(0, |width, glyph| width + glyph.geometry.width);
    }

    fn update_ellipsis_shape(&mut self) {
        let mut recompute = false;

        if self.line_start_attr.is_none() {
            let mut iter = self.attrs.iterator();
            advance_iterator_to(&mut iter, self.runs[0].item.offset);
            self.line_start_attr = Some(iter);
        }

        if let Some(gap_start_attr) = self.gap_start_attr.as_ref() {
            let (start, _) = gap_start_attr.range();
            if self.gap_start_iter.run_iter.start_index < start {
                self.gap_start_attr = None;
            }
        }

        if self.gap_start_attr.is_none()
            && let Some(line_start_attr) = self.line_start_attr.as_ref()
        {
            let mut iter = line_start_attr.clone();
            advance_iterator_to(
                &mut iter,
                self.runs[self.gap_start_iter.run_index].item.offset,
            );
            self.gap_start_attr = Some(iter);
            recompute = true;
        }

        let start_wc = char_at(
            self.text.as_bytes(),
            self.gap_start_iter.run_iter.start_index.max(0) as usize,
        );
        let is_cjk = is_wide(start_wc);
        if is_cjk != self.ellipsis_is_cjk {
            self.ellipsis_is_cjk = is_cjk;
            recompute = true;
        }

        if recompute {
            self.shape_ellipsis();
        }
    }

    fn find_initial_span(&mut self, mode: EllipsizeMode) {
        self.gap_center = match mode {
            EllipsizeMode::Start => 0,
            EllipsizeMode::Middle => self.total_width / 2,
            EllipsizeMode::End | EllipsizeMode::None => self.total_width,
        };

        let n_runs = self.runs.len();
        let mut x = 0;
        let mut i = 0;
        while i < n_runs {
            if x + self.run_info[i].width > self.gap_center {
                break;
            }
            x += self.run_info[i].width;
            i += 1;
        }
        if i == n_runs {
            i -= 1;
            x -= self.run_info[i].width;
        }

        self.gap_start_iter.run_index = i;
        let mut cluster_width = 0;
        let mut have_cluster = self
            .gap_start_iter
            .run_iter
            .init_start(&self.runs[i], self.text);
        while have_cluster {
            cluster_width = self.cluster_width(&self.gap_start_iter);
            if x + cluster_width > self.gap_center {
                break;
            }
            x += cluster_width;
            have_cluster = self
                .gap_start_iter
                .run_iter
                .next_cluster(&self.runs[i], self.text);
        }
        if !have_cluster {
            x -= cluster_width;
        }

        self.gap_end_iter = self.gap_start_iter;
        self.gap_start_x = x;
        self.gap_end_x = x + cluster_width;

        while !self.starts_at_ellipsization_boundary(&self.gap_start_iter) {
            let mut iter = self.gap_start_iter;
            self.prev_cluster(&mut iter);
            self.gap_start_iter = iter;
            self.gap_start_x -= self.cluster_width(&self.gap_start_iter);
        }

        while !self.ends_at_ellipsization_boundary(&self.gap_end_iter) {
            let mut iter = self.gap_end_iter;
            self.next_cluster(&mut iter);
            self.gap_end_iter = iter;
            self.gap_end_x += self.cluster_width(&self.gap_end_iter);
        }

        self.update_ellipsis_shape();
    }

    fn remove_one_span(&mut self) -> bool {
        let mut new_gap_start_iter = self.gap_start_iter;
        let mut new_gap_start_x = self.gap_start_x;
        loop {
            if !self.prev_cluster(&mut new_gap_start_iter) {
                break;
            }
            let width = self.cluster_width(&new_gap_start_iter);
            new_gap_start_x -= width;
            if self.starts_at_ellipsization_boundary(&new_gap_start_iter) && width != 0 {
                break;
            }
        }

        let mut new_gap_end_iter = self.gap_end_iter;
        let mut new_gap_end_x = self.gap_end_x;
        loop {
            if !self.next_cluster(&mut new_gap_end_iter) {
                break;
            }
            let width = self.cluster_width(&new_gap_end_iter);
            new_gap_end_x += width;
            if self.ends_at_ellipsization_boundary(&new_gap_end_iter) && width != 0 {
                break;
            }
        }

        if self.gap_end_x == new_gap_end_x && self.gap_start_x == new_gap_start_x {
            return false;
        }

        if self.gap_end_x == new_gap_end_x
            || (self.gap_start_x != new_gap_start_x
                && self.gap_center - new_gap_start_x < new_gap_end_x - self.gap_center)
        {
            self.gap_start_iter = new_gap_start_iter;
            self.gap_start_x = new_gap_start_x;
            self.update_ellipsis_shape();
        } else {
            self.gap_end_iter = new_gap_end_iter;
            self.gap_end_x = new_gap_end_x;
        }
        true
    }

    fn fixup_ellipsis_run(&mut self, extra_width: i32) {
        let start_index = self.gap_start_iter.run_iter.start_index;
        let end_index = self.gap_end_iter.run_iter.end_index;
        let level = self.runs[self.gap_start_iter.run_index..=self.gap_end_iter.run_index]
            .iter()
            .map(|run| run.item.analysis.level)
            .min()
            .unwrap_or(0);
        let text = self.text;
        let Some(run) = self.ellipsis_run.as_mut() else {
            return;
        };

        let glyphs = &mut run.glyphs;
        for (info, cluster) in glyphs.glyphs.iter_mut().zip(glyphs.log_clusters.iter_mut()) {
            *cluster = 0;
            info.attr.is_cluster_start = false;
        }
        if let Some(first) = glyphs.glyphs.first_mut() {
            first.attr.is_cluster_start = true;
        }
        if let Some(last) = glyphs.glyphs.last_mut() {
            last.geometry.width += extra_width;
        }

        let item = &mut run.item;
        item.offset = start_index;
        item.length = end_index - item.offset;
        item.num_chars = utf8_strlen(text, item.offset, item.length);
        item.analysis.level = level;
        item.analysis.flags |= analysis_flags::IS_ELLIPSIS;
    }

    fn run_list(&mut self) -> Vec<GlyphItem> {
        let text = self.text;
        let gap_start = self.gap_start_iter;
        let gap_end = self.gap_end_iter;
        let mut partial_start_run = None;
        let mut partial_end_run = None;

        let end_run = &mut self.runs[gap_end.run_index];
        if gap_end.run_iter.end_char != end_run.item.num_chars {
            let split_index = gap_end.run_iter.end_index - end_run.item.offset;
            if let Some(first) = end_run.split(text, split_index) {
                partial_end_run = Some(core::mem::replace(end_run, first));
            }
        }

        let start_run = &mut self.runs[gap_start.run_index];
        if gap_start.run_iter.start_char != 0 {
            let split_index = gap_start.run_iter.start_index - start_run.item.offset;
            partial_start_run = start_run.split(text, split_index);
        }

        let runs = core::mem::take(&mut self.runs);
        let mut result = Vec::with_capacity(runs.len() + 3);
        for (i, run) in runs.into_iter().enumerate() {
            if i == gap_start.run_index {
                result.extend(partial_start_run.take());
                result.extend(self.ellipsis_run.take());
                result.extend(partial_end_run.take());
            }
            if i < gap_start.run_index || i > gap_end.run_index {
                result.push(run);
            }
        }
        result
    }

    fn current_width(&self) -> i32 {
        self.total_width - (self.gap_end_x - self.gap_start_x) + self.ellipsis_width
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ellipsize_line(
    ctx: &mut Context,
    text: &str,
    log_attrs: &[LogAttr],
    mode: EllipsizeMode,
    line: &mut LayoutLine,
    attrs: Option<&AttrList>,
    shape_flags: u32,
    goal_width: i32,
) -> bool {
    if mode == EllipsizeMode::None || goal_width < 0 {
        return false;
    }

    let empty = AttrList::new();
    let mut start_offset = utf8_strlen(text, 0, line.start_index);
    let mut run_info = Vec::with_capacity(line.runs.len());
    let mut total_width = 0;
    for run in &line.runs {
        let width = run.glyphs.get_width();
        run_info.push(RunInfo {
            start_offset,
            width,
        });
        total_width += width;
        start_offset += run.item.num_chars;
    }

    if total_width <= goal_width || line.runs.is_empty() {
        return false;
    }

    let mut state = EllipsizeState {
        ctx,
        text,
        log_attrs,
        attrs: attrs.unwrap_or(&empty),
        runs: core::mem::take(&mut line.runs),
        run_info,
        total_width,
        gap_center: 0,
        ellipsis_run: None,
        ellipsis_width: 0,
        ellipsis_is_cjk: false,
        line_start_attr: None,
        gap_start_iter: LineIter::default(),
        gap_start_x: 0,
        gap_start_attr: None,
        gap_end_iter: LineIter::default(),
        gap_end_x: 0,
        shape_flags,
    };

    state.find_initial_span(mode);
    while state.current_width() > goal_width {
        if !state.remove_one_span() {
            break;
        }
    }
    let extra_width = (goal_width - state.current_width()).max(0);
    state.fixup_ellipsis_run(extra_width);
    line.runs = state.run_list();
    true
}
