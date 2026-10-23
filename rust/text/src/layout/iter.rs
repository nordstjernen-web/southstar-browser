//! Southstar text — layout iterators: walking the lines and runs of a layout in visual order with their positions and extents.
//! Copyright 2000, 2001, 2006 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::context::Context;
use crate::glyph::GlyphString;
use crate::glyph_item::GlyphItem;
use crate::types::Rectangle;
use crate::utf8::prev_char;

use super::extents::{Extents, empty_extents_and_height_at_index, run_extents_and_height};
use super::{Layout, LayoutLine, LineRef, lock_context, utf8_strlen_at};

pub struct LayoutIter<'a> {
    layout: &'a Layout,
    line_index: usize,
    run_index: Option<usize>,
    index: i32,
    line_extents: Vec<Extents>,
    run_x: i32,
    run_width: i32,
    end_x_offset: i32,
    ltr: bool,
    cluster_x: i32,
    cluster_width: i32,
    cluster_start: i32,
    next_cluster_glyph: i32,
    cluster_num_chars: i32,
    character_position: i32,
    layout_width: i32,
}

fn next_cluster_start(glyphs: &GlyphString, cluster_start: i32) -> i32 {
    let num_glyphs = glyphs.glyphs.len() as i32;
    let mut i = cluster_start + 1;
    while i < num_glyphs {
        if glyphs.glyphs[i as usize].attr.is_cluster_start {
            return i;
        }
        i += 1;
    }
    num_glyphs
}

fn cluster_width(glyphs: &GlyphString, cluster_start: i32) -> i32 {
    let Some(first) = glyphs.glyphs.get(cluster_start as usize) else {
        return 0;
    };
    let mut width = first.geometry.width;
    for glyph in &glyphs.glyphs[cluster_start as usize + 1..] {
        if glyph.attr.is_cluster_start {
            break;
        }
        width += glyph.geometry.width;
    }
    width
}

impl<'a> LayoutIter<'a> {
    pub(crate) fn new(layout: &'a Layout, ctx: &Context) -> LayoutIter<'a> {
        let (logical, line_extents) = if layout.width == -1 {
            let (_, logical, lines) = layout.extents_internal(ctx, false, true, true);
            (logical.unwrap_or_default().width, lines)
        } else {
            let (_, _, lines) = layout.extents_internal(ctx, false, false, true);
            (layout.width, lines)
        };

        let lines = layout.computed_lines();
        let first = &lines[0];
        let run_index = (!first.runs.is_empty()).then_some(0);
        let run_start_index = first
            .runs
            .first()
            .map_or(first.start_index, |run| run.item.offset);

        let mut iter = LayoutIter {
            layout,
            line_index: 0,
            run_index,
            index: 0,
            line_extents: line_extents.unwrap_or_default(),
            run_x: 0,
            run_width: 0,
            end_x_offset: 0,
            ltr: true,
            cluster_x: 0,
            cluster_width: 0,
            cluster_start: 0,
            next_cluster_glyph: 0,
            cluster_num_chars: 0,
            character_position: 0,
            layout_width: logical,
        };
        iter.update_run(run_start_index);
        iter
    }

    fn current_line(&self) -> &'a LayoutLine {
        &self.layout.computed_lines()[self.line_index]
    }

    fn current_run(&self) -> Option<&'a GlyphItem> {
        let line = self.current_line();
        self.run_index.and_then(|index| line.runs.get(index))
    }

    fn update_cluster(&mut self, cluster_start_index: i32) {
        let Some(run) = self.current_run() else {
            return;
        };
        self.character_position = 0;
        let glyphs = &run.glyphs;
        let num_glyphs = glyphs.glyphs.len() as i32;
        self.cluster_width = cluster_width(glyphs, self.cluster_start);
        self.next_cluster_glyph = next_cluster_start(glyphs, self.cluster_start);

        let cluster_length = if self.ltr {
            if self.next_cluster_glyph < num_glyphs {
                glyphs.log_clusters[self.next_cluster_glyph as usize] - cluster_start_index
            } else {
                run.item.length - cluster_start_index
            }
        } else {
            let mut i = self.cluster_start;
            while i > 0 && glyphs.log_clusters[(i - 1) as usize] == cluster_start_index {
                i -= 1;
            }
            if i == 0 {
                run.item.length - cluster_start_index
            } else {
                glyphs.log_clusters[(i - 1) as usize] - cluster_start_index
            }
        };

        let text = self.layout.text.as_str();
        let cluster_text = run.item.offset + cluster_start_index;
        self.cluster_num_chars = utf8_strlen_at(text, cluster_text, cluster_length);
        self.index = if self.ltr {
            cluster_text
        } else {
            prev_char(text.as_bytes(), (cluster_text + cluster_length) as usize) as i32
        };
    }

    fn update_run(&mut self, run_start_index: i32) {
        let line_ext = self.line_extents[self.line_index];
        let line = self.current_line();
        let run = self.current_run();
        let is_first_run = match self.run_index {
            Some(index) => index == 0,
            None => line.runs.is_empty(),
        };

        if is_first_run {
            self.run_x = line_ext.logical_rect.x;
        } else {
            self.run_x += self.end_x_offset + self.run_width;
            if let Some(run) = run {
                self.run_x += run.start_x_offset;
            }
        }

        match run {
            Some(run) => {
                self.run_width = run.glyphs.get_width();
                self.end_x_offset = run.end_x_offset;
                self.ltr = run.item.analysis.level % 2 == 0;
            }
            None => {
                self.run_width = 0;
                self.end_x_offset = 0;
                self.ltr = true;
            }
        }

        self.cluster_start = 0;
        self.cluster_x = self.run_x;

        match run {
            Some(run) => {
                let first_cluster = run.glyphs.log_clusters.first().copied().unwrap_or(0);
                self.update_cluster(first_cluster);
            }
            None => {
                self.cluster_width = 0;
                self.character_position = 0;
                self.cluster_num_chars = 0;
                self.index = run_start_index;
            }
        }
    }

    pub fn index(&self) -> i32 {
        self.index
    }

    pub fn line_index(&self) -> usize {
        self.line_index
    }

    pub fn line(&self) -> LineRef<'a> {
        LineRef::new(self.layout, self.line_index)
    }

    pub fn run(&self) -> Option<&'a GlyphItem> {
        self.current_run()
    }

    pub fn at_last_line(&self) -> bool {
        self.line_index + 1 >= self.layout.computed_lines().len()
    }

    pub fn next_run(&mut self) -> bool {
        let Some(run_index) = self.run_index else {
            return self.next_line();
        };
        let line = self.current_line();
        let next_run_start = if run_index + 1 >= line.runs.len() {
            let run = &line.runs[run_index];
            self.run_index = None;
            run.item.offset + run.item.length
        } else {
            self.run_index = Some(run_index + 1);
            line.runs[run_index + 1].item.offset
        };
        self.update_run(next_run_start);
        true
    }

    pub fn next_line(&mut self) -> bool {
        if self.at_last_line() {
            return false;
        }
        self.line_index += 1;
        let line = self.current_line();
        self.run_index = (!line.runs.is_empty()).then_some(0);
        self.update_run(line.start_index);
        true
    }

    fn offset_y(&self, y: &mut i32) {
        *y += self.line_extents[self.line_index].baseline;
    }

    pub fn run_extents(&self) -> (Rectangle, Rectangle) {
        let (mut ink, mut logical) = match self.current_run() {
            Some(run) => {
                let extents = run_extents_and_height(run, false);
                let mut ink = extents.ink;
                let mut logical = extents.logical;
                self.offset_y(&mut ink.y);
                ink.x += self.run_x;
                self.offset_y(&mut logical.y);
                logical.x += self.run_x;
                return (ink, logical);
            }
            None => match self.current_line().runs.last() {
                Some(run) => {
                    let extents = run_extents_and_height(run, false);
                    (extents.ink, extents.logical)
                }
                None => {
                    let ctx = lock_context(&self.layout.context);
                    let (rect, _) = empty_extents_and_height_at_index(self.layout, &ctx, 0, false);
                    (rect, rect)
                }
            },
        };
        self.offset_y(&mut ink.y);
        ink.x = self.run_x;
        ink.width = 0;
        self.offset_y(&mut logical.y);
        logical.x = self.run_x;
        logical.width = 0;
        (ink, logical)
    }

    pub fn line_extents(&self) -> (Rectangle, Rectangle) {
        let ext = self.line_extents[self.line_index];
        let ctx = lock_context(&self.layout.context);
        let (ink, _) = self.layout.line_extents_layout_coords(
            &ctx,
            self.line_index,
            self.layout_width,
            ext.logical_rect.y,
            None,
        );
        (ink, ext.logical_rect)
    }

    pub fn line_logical_extents(&self) -> Rectangle {
        self.line_extents[self.line_index].logical_rect
    }

    pub fn line_yrange(&self) -> (i32, i32) {
        let ext = self.line_extents[self.line_index];
        let spacing = self.layout.spacing;
        let half_spacing = spacing / 2;
        let y0 = if self.line_index == 0 {
            ext.logical_rect.y
        } else {
            ext.logical_rect.y - (spacing - half_spacing)
        };
        let y1 = if self.line_index + 1 == self.layout.computed_lines().len() {
            ext.logical_rect.y + ext.logical_rect.height
        } else {
            ext.logical_rect.y + ext.logical_rect.height + half_spacing
        };
        (y0, y1)
    }

    pub fn baseline(&self) -> i32 {
        self.line_extents[self.line_index].baseline
    }

    pub fn layout_extents(&self) -> (Rectangle, Rectangle) {
        let ctx = lock_context(&self.layout.context);
        let (ink, logical, _) = self.layout.extents_internal(&ctx, true, true, false);
        (ink.unwrap_or_default(), logical.unwrap_or_default())
    }

    pub fn cluster_width(&self) -> i32 {
        self.cluster_width
    }

    pub fn cluster_x(&self) -> i32 {
        self.cluster_x
    }

    pub fn cluster_num_chars(&self) -> i32 {
        self.cluster_num_chars
    }

    pub fn character_position(&self) -> i32 {
        self.character_position
    }

    pub fn next_cluster_glyph(&self) -> i32 {
        self.next_cluster_glyph
    }

    pub fn layout_width(&self) -> i32 {
        self.layout_width
    }
}
