//! Southstar text — layout lines: the runs of one line and the conversions between byte indices and x positions on it.
//! Copyright 2000, 2001, 2006 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::cell::Cell;
use std::ops::Deref;

use crate::glyph_item::GlyphItem;
use crate::types::{Alignment, Direction, Rectangle, extents_to_pixels};
use crate::utf8::{next_char, prev_char};

use super::extents::line_extents_and_height;
use super::{Layout, char_offset_of, lock_context};

#[derive(Clone, Debug)]
pub struct LayoutLine {
    pub start_index: i32,
    pub length: i32,
    pub runs: Vec<GlyphItem>,
    pub is_paragraph_start: bool,
    pub resolved_dir: Direction,
    pub(crate) extents_cache: Cell<Option<(Rectangle, Rectangle, i32)>>,
}

impl LayoutLine {
    pub(crate) fn new(start_index: i32, is_paragraph_start: bool) -> LayoutLine {
        LayoutLine {
            start_index,
            length: 0,
            runs: Vec::new(),
            is_paragraph_start,
            resolved_dir: Direction::Ltr,
            extents_cache: Cell::new(None),
        }
    }

    pub fn width(&self) -> i32 {
        self.runs.iter().map(|run| run.glyphs.get_width()).sum()
    }
}

#[derive(Clone, Copy)]
pub struct LineRef<'a> {
    layout: &'a Layout,
    index: usize,
}

impl Deref for LineRef<'_> {
    type Target = LayoutLine;

    fn deref(&self) -> &LayoutLine {
        &self.layout.computed_lines()[self.index]
    }
}

impl core::fmt::Debug for LineRef<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("LineRef")
            .field("index", &self.index)
            .field("line", &**self)
            .finish()
    }
}

impl<'a> LineRef<'a> {
    pub(crate) fn new(layout: &'a Layout, index: usize) -> LineRef<'a> {
        LineRef { layout, index }
    }

    pub fn layout(&self) -> &'a Layout {
        self.layout
    }

    pub fn line_index(&self) -> usize {
        self.index
    }

    pub fn line(&self) -> &'a LayoutLine {
        &self.layout.computed_lines()[self.index]
    }

    pub fn extents_and_height(&self) -> (Rectangle, Rectangle, i32) {
        let ctx = lock_context(&self.layout.context);
        line_extents_and_height(self.layout, &ctx, self.line())
    }

    pub fn extents(&self) -> (Rectangle, Rectangle) {
        let (ink, logical, _) = self.extents_and_height();
        (ink, logical)
    }

    pub fn pixel_extents(&self) -> (Rectangle, Rectangle) {
        let (mut ink, mut logical) = self.extents();
        extents_to_pixels(Some(&mut ink), None);
        extents_to_pixels(Some(&mut logical), None);
        (ink, logical)
    }

    pub fn height(&self) -> i32 {
        self.extents_and_height().2
    }

    pub fn index_to_x(&self, index: i32, trailing: bool) -> i32 {
        let layout = self.layout;
        let line = self.line();
        let text = layout.text.as_str();
        let bytes = text.as_bytes();
        let log_attrs = layout.computed_log_attrs();
        let mut width = 0;

        for run in &line.runs {
            let item = &run.item;
            if item.offset <= index && item.offset + item.length > index {
                let mut index = index;
                let mut offset = char_offset_of(text, index);
                if trailing {
                    while index < line.start_index + line.length
                        && offset + 1 < layout.n_chars
                        && !log_attrs
                            .get((offset + 1) as usize)
                            .is_some_and(|attr| attr.is_cursor_position())
                    {
                        offset += 1;
                        index = next_char(bytes, index as usize) as i32;
                    }
                } else {
                    while index > line.start_index
                        && !log_attrs
                            .get(offset as usize)
                            .is_some_and(|attr| attr.is_cursor_position())
                    {
                        offset -= 1;
                        index = prev_char(bytes, index as usize) as i32;
                    }
                }

                let attr_offset = (item.char_offset.max(0) as usize).min(log_attrs.len());
                let start = item.offset as usize;
                let end = (item.offset + item.length) as usize;
                let x = run.glyphs.index_to_x_full(
                    &text[start..end],
                    &item.analysis,
                    Some(&log_attrs[attr_offset..]),
                    index - item.offset,
                    trailing,
                );
                return x + width;
            }
            width += run.glyphs.get_width();
        }
        width
    }

    pub fn x_to_index(&self, x_pos: i32) -> (bool, i32, i32) {
        let layout = self.layout;
        let line = self.line();
        let text = layout.text.as_str();
        let bytes = text.as_bytes();
        let log_attrs = layout.computed_log_attrs();
        let cursor_at = |offset: i32| {
            usize::try_from(offset)
                .ok()
                .and_then(|offset| log_attrs.get(offset))
                .is_some_and(|attr| attr.is_cursor_position())
        };

        let first_index = line.start_index;
        if line.length == 0 {
            return (false, first_index, 0);
        }

        let first_offset = char_offset_of(text, line.start_index);
        let end_index = first_index + line.length;
        let end_offset = first_offset + char_offset_of(&text[first_index as usize..], line.length);

        let mut last_index = end_index;
        let mut last_offset = end_offset;
        let mut last_trailing = 0;
        loop {
            last_index = prev_char(bytes, last_index as usize) as i32;
            last_offset -= 1;
            last_trailing += 1;
            if last_offset <= first_offset || cursor_at(last_offset) {
                break;
            }
        }

        let lines = layout.computed_lines();
        let suppress_last_trailing = lines
            .get(self.index + 1)
            .is_some_and(|next| line.start_index + line.length == next.start_index);

        if x_pos < 0 {
            let index = if line.resolved_dir == Direction::Ltr {
                first_index
            } else {
                last_index
            };
            let trailing = if line.resolved_dir == Direction::Ltr || suppress_last_trailing {
                0
            } else {
                last_trailing
            };
            return (false, index, trailing);
        }

        let mut start_pos = 0;
        for run in &line.runs {
            let logical_width = run.glyphs.get_width();
            if x_pos >= start_pos && x_pos < start_pos + logical_width {
                let item = &run.item;
                let start = item.offset as usize;
                let end = (item.offset + item.length) as usize;
                let (pos, char_trailing) =
                    run.glyphs
                        .x_to_index(&text[start..end], &item.analysis, x_pos - start_pos);
                let char_index = item.offset + pos;
                let offset = char_offset_of(text, char_index);

                let mut grapheme_start_offset = offset;
                let mut grapheme_start_index = char_index;
                while grapheme_start_offset > first_offset && !cursor_at(grapheme_start_offset) {
                    grapheme_start_index = prev_char(bytes, grapheme_start_index as usize) as i32;
                    grapheme_start_offset -= 1;
                }

                let mut grapheme_end_offset = offset;
                loop {
                    grapheme_end_offset += 1;
                    if grapheme_end_offset >= end_offset || cursor_at(grapheme_end_offset) {
                        break;
                    }
                }

                let trailing = if (grapheme_end_offset == end_offset && suppress_last_trailing)
                    || offset + i32::from(char_trailing)
                        <= (grapheme_start_offset + grapheme_end_offset) / 2
                {
                    0
                } else {
                    grapheme_end_offset - grapheme_start_offset
                };
                return (true, grapheme_start_index, trailing);
            }
            start_pos += logical_width;
        }

        let index = if line.resolved_dir == Direction::Ltr {
            last_index
        } else {
            first_index
        };
        let trailing = if line.resolved_dir == Direction::Ltr && !suppress_last_trailing {
            last_trailing
        } else {
            0
        };
        (false, index, trailing)
    }

    pub fn x_ranges(&self, start_index: i32, end_index: i32) -> Vec<(i32, i32)> {
        let mut ranges = Vec::new();
        if start_index > end_index {
            return ranges;
        }
        let layout = self.layout;
        let line = self.line();
        let text = layout.text.as_str();
        let bytes = text.as_bytes();
        let log_attrs = layout.computed_log_attrs();
        let ctx = lock_context(&layout.context);

        let alignment = layout.alignment_for(line, ctx.base_dir());
        let mut width = layout.width;
        if width == -1 && alignment != Alignment::Left {
            width = layout
                .extents_internal(&ctx, false, true, false)
                .1
                .unwrap_or_default()
                .width;
        }

        let (_, line_logical, _) = line_extents_and_height(layout, &ctx, line);
        let line_width = line_logical.width;
        let x_offset = layout.x_offset(line, ctx.base_dir(), width, line_width);
        drop(ctx);

        let line_start_index = line.start_index;
        if x_offset > 0
            && ((line.resolved_dir == Direction::Ltr && start_index < line_start_index)
                || (line.resolved_dir == Direction::Rtl
                    && end_index > line_start_index + line.length))
        {
            ranges.push((0, x_offset));
        }

        let mut accumulated_width = 0;
        for (i, run) in line.runs.iter().enumerate() {
            let item = &run.item;
            if start_index < item.offset + item.length && end_index > item.offset {
                let run_start_index = start_index.max(item.offset);
                let run_end_index = end_index.min(item.offset + item.length);
                let run_end_index = prev_char(bytes, run_end_index as usize) as i32;
                let attr_offset = (item.char_offset.max(0) as usize).min(log_attrs.len());
                let start = item.offset as usize;
                let end = (item.offset + item.length) as usize;
                let item_text = &text[start..end];
                let run_start_x = run.glyphs.index_to_x_full(
                    item_text,
                    &item.analysis,
                    Some(&log_attrs[attr_offset..]),
                    run_start_index - item.offset,
                    false,
                );
                let run_end_x = run.glyphs.index_to_x_full(
                    item_text,
                    &item.analysis,
                    Some(&log_attrs[attr_offset..]),
                    run_end_index - item.offset,
                    true,
                );
                ranges.push((
                    x_offset + accumulated_width + run_start_x.min(run_end_x),
                    x_offset + accumulated_width + run_start_x.max(run_end_x),
                ));
            }
            if i + 1 < line.runs.len() {
                accumulated_width += run.glyphs.get_width();
            }
        }

        if x_offset + line_width < layout.width
            && ((line.resolved_dir == Direction::Ltr && end_index > line_start_index + line.length)
                || (line.resolved_dir == Direction::Rtl && start_index < line_start_index))
        {
            ranges.push((x_offset + line_width, layout.width));
        }
        ranges
    }
}
