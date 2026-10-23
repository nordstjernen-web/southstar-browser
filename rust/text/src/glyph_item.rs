//! Southstar text — glyph items: an item with its shaped glyphs, split at clusters, re-attributed, letter- and word-spaced and walked cluster by cluster.
//! Copyright 2002 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::attrs::{AttrList, Attribute};
use crate::glyph::GlyphString;
use crate::item::{Item, analysis_flags};
use crate::types::LogAttr;
use crate::utf8::{self, pango_utf8_strlen, pointer_to_offset};

#[derive(Clone, Debug, Default)]
pub struct GlyphItem {
    pub item: Item,
    pub glyphs: GlyphString,
    pub y_offset: i32,
    pub start_x_offset: i32,
    pub end_x_offset: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphItemIter {
    pub start_glyph: i32,
    pub start_index: i32,
    pub start_char: i32,
    pub end_glyph: i32,
    pub end_index: i32,
    pub end_char: i32,
}

fn utf8_strlen(text: &[u8], start: i32, end: i32) -> i32 {
    let start = usize::try_from(start).unwrap_or(0);
    let length = usize::try_from(end - start as i32).unwrap_or(0);
    pango_utf8_strlen(text.get(start..).unwrap_or(&[]), length)
}

fn char_at(text: &[u8], index: i32) -> u32 {
    usize::try_from(index).map_or(0, |index| utf8::char_at(text, index))
}

fn is_word_separator(ch: u32) -> bool {
    matches!(
        ch,
        0x0020 | 0x00A0 | 0x1361 | 0x10100 | 0x10101 | 0x1039F | 0x103D0
    )
}

impl GlyphItemIter {
    pub fn next_cluster(&mut self, glyph_item: &GlyphItem, text: &str) -> bool {
        let mut glyph_index = self.end_glyph;
        let glyphs = &glyph_item.glyphs;
        let num_glyphs = glyphs.glyphs.len() as i32;
        let item = &glyph_item.item;
        let bytes = text.as_bytes();
        let ltr = glyph_item.is_ltr();

        if ltr {
            if glyph_index == num_glyphs {
                return false;
            }
        } else if glyph_index < 0 {
            return false;
        }

        self.start_glyph = self.end_glyph;
        self.start_index = self.end_index;
        self.start_char = self.end_char;

        let cluster = glyphs.log_clusters[glyph_index as usize];
        loop {
            if ltr {
                glyph_index += 1;
                if glyph_index == num_glyphs {
                    self.end_index = item.offset + item.length;
                    self.end_char = item.num_chars;
                    break;
                }
            } else {
                glyph_index -= 1;
                if glyph_index < 0 {
                    self.end_index = item.offset + item.length;
                    self.end_char = item.num_chars;
                    break;
                }
            }

            let next = glyphs.log_clusters[glyph_index as usize];
            if next > cluster {
                self.end_index = item.offset + next;
                self.end_char += utf8_strlen(bytes, self.start_index, self.end_index);
                break;
            }
        }

        self.end_glyph = glyph_index;
        true
    }

    pub fn prev_cluster(&mut self, glyph_item: &GlyphItem, text: &str) -> bool {
        let mut glyph_index = self.start_glyph;
        let glyphs = &glyph_item.glyphs;
        let num_glyphs = glyphs.glyphs.len() as i32;
        let item = &glyph_item.item;
        let bytes = text.as_bytes();
        let ltr = glyph_item.is_ltr();

        if ltr {
            if glyph_index == 0 {
                return false;
            }
        } else if glyph_index == num_glyphs - 1 {
            return false;
        }

        self.end_glyph = self.start_glyph;
        self.end_index = self.start_index;
        self.end_char = self.start_char;

        if ltr {
            let cluster = glyphs.log_clusters[(glyph_index - 1) as usize];
            loop {
                if glyph_index == 0 {
                    self.start_index = item.offset;
                    self.start_char = 0;
                    break;
                }
                glyph_index -= 1;
                if glyphs.log_clusters[glyph_index as usize] < cluster {
                    glyph_index += 1;
                    self.start_index = item.offset + glyphs.log_clusters[glyph_index as usize];
                    self.start_char -= utf8_strlen(bytes, self.start_index, self.end_index);
                    break;
                }
            }
        } else {
            let cluster = glyphs.log_clusters[(glyph_index + 1) as usize];
            loop {
                if glyph_index == num_glyphs - 1 {
                    self.start_index = item.offset;
                    self.start_char = 0;
                    break;
                }
                glyph_index += 1;
                if glyphs.log_clusters[glyph_index as usize] < cluster {
                    glyph_index -= 1;
                    self.start_index = item.offset + glyphs.log_clusters[glyph_index as usize];
                    self.start_char -= utf8_strlen(bytes, self.start_index, self.end_index);
                    break;
                }
            }
        }

        self.start_glyph = glyph_index;
        true
    }

    pub fn init_start(&mut self, glyph_item: &GlyphItem, text: &str) -> bool {
        self.end_glyph = if glyph_item.is_ltr() {
            0
        } else {
            glyph_item.glyphs.glyphs.len() as i32 - 1
        };
        self.end_index = glyph_item.item.offset;
        self.end_char = 0;
        self.start_glyph = self.end_glyph;
        self.start_index = self.end_index;
        self.start_char = self.end_char;
        self.next_cluster(glyph_item, text)
    }

    pub fn init_end(&mut self, glyph_item: &GlyphItem, text: &str) -> bool {
        self.start_glyph = if glyph_item.is_ltr() {
            glyph_item.glyphs.glyphs.len() as i32
        } else {
            -1
        };
        self.start_index = glyph_item.item.offset + glyph_item.item.length;
        self.start_char = glyph_item.item.num_chars;
        self.end_glyph = self.start_glyph;
        self.end_index = self.start_index;
        self.end_char = self.start_char;
        self.prev_cluster(glyph_item, text)
    }
}

fn split_before_cluster_start(
    glyph_item: &mut GlyphItem,
    iter: &mut GlyphItemIter,
    text: &str,
    segment_attrs: Vec<Attribute>,
) -> Option<GlyphItem> {
    let split_len = iter.start_index - glyph_item.item.offset;
    let mut split_item = glyph_item.split(text, split_len)?;
    split_item.item.analysis.extra_attrs.extend(segment_attrs);

    let split_glyphs = split_item.glyphs.glyphs.len() as i32;
    if glyph_item.is_ltr() {
        iter.start_glyph -= split_glyphs;
        iter.end_glyph -= split_glyphs;
    }
    iter.start_char -= split_item.item.num_chars;
    iter.end_char -= split_item.item.num_chars;

    Some(split_item)
}

impl GlyphItem {
    pub fn new(item: Item, glyphs: GlyphString) -> GlyphItem {
        GlyphItem {
            item,
            glyphs,
            y_offset: 0,
            start_x_offset: 0,
            end_x_offset: 0,
        }
    }

    pub fn is_ltr(&self) -> bool {
        self.item.analysis.level & 1 == 0
    }

    pub fn split(&mut self, text: &str, split_index: i32) -> Option<GlyphItem> {
        if self.item.length <= 0 || split_index <= 0 || split_index >= self.item.length {
            return None;
        }

        let total = self.glyphs.glyphs.len();
        let clusters = &self.glyphs.log_clusters;
        let (cut, num_glyphs) = if self.is_ltr() {
            let position = clusters.iter().position(|&c| c >= split_index)?;
            (clusters[position], position)
        } else {
            let position = clusters.iter().rposition(|&c| c >= split_index)?;
            (clusters[position], total - 1 - position)
        };
        let num_remaining = total - num_glyphs;

        let start = usize::try_from(self.item.offset).unwrap_or(0);
        let split_offset = pointer_to_offset(text.as_bytes(), start, start + cut as usize);
        let new_item = self.item.split(cut, split_offset)?;

        let mut new_glyphs = GlyphString::new();
        if self.is_ltr() {
            new_glyphs.glyphs = self.glyphs.glyphs.drain(..num_glyphs).collect();
            new_glyphs.log_clusters = self.glyphs.log_clusters.drain(..num_glyphs).collect();
        } else {
            new_glyphs.glyphs = self.glyphs.glyphs.drain(num_remaining..).collect();
            new_glyphs.log_clusters = self.glyphs.log_clusters.drain(num_remaining..).collect();
        }
        for cluster in &mut self.glyphs.log_clusters {
            *cluster -= cut;
        }

        Some(GlyphItem {
            item: new_item,
            glyphs: new_glyphs,
            y_offset: self.y_offset,
            start_x_offset: self.start_x_offset,
            end_x_offset: -self.start_x_offset,
        })
    }

    pub fn apply_attrs(mut self, text: &str, list: &AttrList) -> Vec<GlyphItem> {
        let mut iter = list.iterator();
        let mut result: Vec<GlyphItem> = Vec::new();

        let (mut range_start, mut range_end) = loop {
            let range = iter.range();
            if range.1 > self.item.offset || !iter.next() {
                break range;
            }
        };

        let mut segment_attrs = iter.get_attrs();

        let is_ellipsis = self.item.analysis.flags & analysis_flags::IS_ELLIPSIS != 0;
        let covers_item =
            range_start <= self.item.offset && range_end >= self.item.offset + self.item.length;

        if !is_ellipsis && !covers_item {
            let mut start_new_segment = false;
            let mut cluster_iter = GlyphItemIter::default();
            let mut have_cluster = cluster_iter.init_start(&self, text);
            while have_cluster {
                if start_new_segment {
                    let attrs = core::mem::take(&mut segment_attrs);
                    if let Some(split) =
                        split_before_cluster_start(&mut self, &mut cluster_iter, text, attrs)
                    {
                        result.push(split);
                    }
                    segment_attrs = iter.get_attrs();
                }

                start_new_segment = false;

                loop {
                    if range_end > cluster_iter.end_index {
                        break;
                    }

                    start_new_segment = true;

                    let have_next = iter.next();
                    (range_start, range_end) = iter.range();

                    if range_start >= cluster_iter.end_index {
                        break;
                    }

                    if range_start > cluster_iter.start_index
                        && cluster_iter.start_index != self.item.offset
                    {
                        let new_attrs = segment_attrs.clone();
                        let attrs = core::mem::take(&mut segment_attrs);
                        if let Some(split) =
                            split_before_cluster_start(&mut self, &mut cluster_iter, text, attrs)
                        {
                            result.push(split);
                        }
                        segment_attrs = new_attrs;
                    }

                    segment_attrs.extend(iter.get_attrs());

                    if !have_next {
                        break;
                    }
                }

                have_cluster = cluster_iter.next_cluster(&self, text);
            }
        }

        self.item.analysis.extra_attrs.extend(segment_attrs);
        let ltr = self.is_ltr();
        result.push(self);
        if !ltr {
            result.reverse();
        }
        result
    }

    pub fn letter_space(&mut self, text: &str, log_attrs: &[LogAttr], letter_spacing: i32) {
        let mut iter = GlyphItemIter::default();
        let mut have_cluster = iter.init_start(self, text);
        while have_cluster {
            let is_cursor_position = log_attrs
                .get(iter.start_char as usize)
                .is_some_and(|attr| attr.is_cursor_position());
            let glyphs = &mut self.glyphs.glyphs;
            if !is_cursor_position {
                if glyphs[iter.start_glyph as usize].geometry.width == 0
                    && iter.start_glyph < iter.end_glyph
                {
                    glyphs[iter.start_glyph as usize].geometry.x_offset -= letter_spacing;
                }
            } else if iter.start_glyph < iter.end_glyph {
                glyphs[(iter.end_glyph - 1) as usize].geometry.width += letter_spacing;
            } else {
                let glyph = &mut glyphs[(iter.end_glyph + 1) as usize].geometry;
                glyph.x_offset += letter_spacing;
                glyph.width += letter_spacing;
            }
            have_cluster = iter.next_cluster(self, text);
        }
    }

    pub fn word_space(&mut self, text: &str, word_spacing: i32) {
        if word_spacing == 0 {
            return;
        }
        let bytes = text.as_bytes();
        let mut iter = GlyphItemIter::default();
        let mut have_cluster = iter.init_start(self, text);
        while have_cluster {
            if is_word_separator(char_at(bytes, iter.start_index)) {
                let glyphs = &mut self.glyphs.glyphs;
                if iter.start_glyph < iter.end_glyph {
                    glyphs[(iter.end_glyph - 1) as usize].geometry.width += word_spacing;
                } else {
                    glyphs[(iter.end_glyph + 1) as usize].geometry.width += word_spacing;
                }
            }
            have_cluster = iter.next_cluster(self, text);
        }
    }

    pub fn get_logical_widths(&self, text: &str, logical_widths: &mut [i32]) {
        let dir: i32 = if self.item.analysis.level & 1 == 0 {
            1
        } else {
            -1
        };
        let mut iter = GlyphItemIter::default();
        let mut has_cluster = iter.init_start(self, text);
        while has_cluster {
            let mut cluster_width = 0;
            let mut glyph_index = iter.start_glyph;
            while glyph_index != iter.end_glyph {
                cluster_width += self.glyphs.glyphs[glyph_index as usize].geometry.width;
                glyph_index += dir;
            }

            let num_chars = iter.end_char - iter.start_char;
            if num_chars != 0 {
                let char_width = cluster_width / num_chars;
                for char_index in iter.start_char..iter.end_char {
                    if let Some(width) = logical_widths.get_mut(char_index as usize) {
                        *width = char_width;
                    }
                }
                if let Some(width) = logical_widths.get_mut(iter.start_char as usize) {
                    *width += cluster_width - char_width * num_chars;
                }
            }

            has_cluster = iter.next_cluster(self, text);
        }
    }
}
