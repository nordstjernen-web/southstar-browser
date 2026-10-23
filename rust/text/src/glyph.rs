//! Southstar text — glyph strings: shaped glyphs with their positions and clusters, their extents and the mapping between x positions and text.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::ffi::harfbuzz::{
    DIRECTION_LEFT_TO_RIGHT, DIRECTION_RIGHT_TO_LEFT, OT_LAYOUT_GLYPH_CLASS_MARK,
};
use crate::font::{Font, glyph_extents_of};
use crate::item::Analysis;
use crate::types::{Glyph, GlyphUnit, LogAttr, Rectangle};
use crate::utf8::{next_char, pointer_to_offset};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GlyphGeometry {
    pub width: GlyphUnit,
    pub x_offset: GlyphUnit,
    pub y_offset: GlyphUnit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GlyphVisAttr {
    pub is_cluster_start: bool,
    pub is_color: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GlyphInfo {
    pub glyph: Glyph,
    pub geometry: GlyphGeometry,
    pub attr: GlyphVisAttr,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlyphString {
    pub glyphs: Vec<GlyphInfo>,
    pub log_clusters: Vec<i32>,
}

fn clamp_index(text: &[u8], index: i32) -> usize {
    usize::try_from(index).unwrap_or(0).min(text.len())
}

impl GlyphString {
    pub fn new() -> GlyphString {
        GlyphString::default()
    }

    pub fn num_glyphs(&self) -> usize {
        self.glyphs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    pub fn set_size(&mut self, new_len: usize) {
        self.glyphs.resize(new_len, GlyphInfo::default());
        self.log_clusters.resize(new_len, 0);
    }

    pub fn reverse_range(&mut self, start: usize, end: usize) {
        if start < end {
            self.glyphs[start..end].reverse();
            self.log_clusters[start..end].reverse();
        }
    }

    pub fn extents_range(
        &self,
        start: usize,
        end: usize,
        font: Option<&Font>,
    ) -> (Rectangle, Rectangle) {
        let mut ink_rect = Rectangle::default();
        let mut logical_rect = Rectangle::default();
        let mut x_pos = 0;

        for i in start..end.min(self.glyphs.len()) {
            let info = &self.glyphs[i];
            let geometry = &info.geometry;
            let (glyph_ink, glyph_logical) = glyph_extents_of(font, info.glyph);

            if glyph_ink.width != 0 && glyph_ink.height != 0 {
                if ink_rect.width == 0 || ink_rect.height == 0 {
                    ink_rect.x = x_pos + glyph_ink.x + geometry.x_offset;
                    ink_rect.width = glyph_ink.width;
                    ink_rect.y = glyph_ink.y + geometry.y_offset;
                    ink_rect.height = glyph_ink.height;
                } else {
                    let new_x = ink_rect.x.min(x_pos + glyph_ink.x + geometry.x_offset);
                    ink_rect.width = (ink_rect.x + ink_rect.width)
                        .max(x_pos + glyph_ink.x + glyph_ink.width + geometry.x_offset)
                        - new_x;
                    ink_rect.x = new_x;

                    let new_y = ink_rect.y.min(glyph_ink.y + geometry.y_offset);
                    ink_rect.height = (ink_rect.y + ink_rect.height)
                        .max(glyph_ink.y + glyph_ink.height + geometry.y_offset)
                        - new_y;
                    ink_rect.y = new_y;
                }
            }

            logical_rect.width += geometry.width;
            if i == start {
                logical_rect.y = glyph_logical.y;
                logical_rect.height = glyph_logical.height;
            } else {
                let new_y = logical_rect.y.min(glyph_logical.y);
                logical_rect.height = (logical_rect.y + logical_rect.height)
                    .max(glyph_logical.y + glyph_logical.height)
                    - new_y;
                logical_rect.y = new_y;
            }

            x_pos += geometry.width;
        }

        (ink_rect, logical_rect)
    }

    pub fn extents(&self, font: Option<&Font>) -> (Rectangle, Rectangle) {
        self.extents_range(0, self.glyphs.len(), font)
    }

    pub fn get_width(&self) -> i32 {
        self.glyphs
            .iter()
            .fold(0i32, |width, info| width.wrapping_add(info.geometry.width))
    }

    pub fn index_to_x(&self, text: &str, analysis: &Analysis, index: i32, trailing: bool) -> i32 {
        self.index_to_x_full(text, analysis, None, index, trailing)
    }

    pub fn index_to_x_full(
        &self,
        text: &str,
        analysis: &Analysis,
        attrs: Option<&[LogAttr]>,
        index: i32,
        trailing: bool,
    ) -> i32 {
        let bytes = text.as_bytes();
        let length = bytes.len() as i32;
        let num_glyphs = self.glyphs.len() as i32;
        let mut start_xpos = 0;
        let mut end_xpos = 0;
        let mut width = 0;
        let mut start_index = -1;
        let mut end_index = -1;
        let mut cluster_chars = 0;
        let mut cluster_offset = 0;
        let mut start_glyph_pos: i32 = -1;
        let mut end_glyph_pos: i32 = -1;
        let rtl = analysis.level & 1 != 0;

        if num_glyphs == 0 {
            return 0;
        }

        let geometry_width = |i: i32| self.glyphs[i as usize].geometry.width;
        let cluster = |i: i32| self.log_clusters[i as usize];

        if rtl {
            for i in (0..num_glyphs).rev() {
                width += geometry_width(i);
            }
            for i in (0..num_glyphs).rev() {
                if cluster(i) > index {
                    end_index = cluster(i);
                    end_xpos = width;
                    break;
                }
                if cluster(i) != start_index {
                    start_index = cluster(i);
                    start_xpos = width;
                }
                width -= geometry_width(i);
            }
            for i in (0..num_glyphs).rev() {
                if cluster(i) == start_index {
                    if end_glyph_pos < 0 {
                        end_glyph_pos = i;
                    }
                    start_glyph_pos = i;
                }
            }
        } else {
            for i in 0..num_glyphs {
                if cluster(i) > index {
                    end_index = cluster(i);
                    end_xpos = width;
                    break;
                }
                if cluster(i) != start_index {
                    start_index = cluster(i);
                    start_xpos = width;
                }
                width += geometry_width(i);
            }
            for i in 0..num_glyphs {
                if cluster(i) == start_index {
                    if start_glyph_pos < 0 {
                        start_glyph_pos = i;
                    }
                    end_glyph_pos = i;
                }
            }
        }

        if end_index == -1 {
            end_index = length;
            end_xpos = if rtl { 0 } else { width };
        }

        let mut p = clamp_index(bytes, start_index);
        let end = clamp_index(bytes, end_index);
        let mut char_index = if attrs.is_some() {
            pointer_to_offset(bytes, 0, p)
        } else {
            0
        };
        let index_pos = clamp_index(bytes, index);
        while p < end {
            let is_cursor_position = attrs.is_none_or(|attrs| {
                attrs
                    .get(char_index as usize)
                    .is_some_and(|attr| attr.is_cursor_position())
            });
            if is_cursor_position {
                if p < index_pos {
                    cluster_offset += 1;
                }
                cluster_chars += 1;
            }
            p = next_char(bytes, p);
            char_index += 1;
        }

        if trailing {
            cluster_offset = (cluster_offset + 1).min(cluster_chars);
        }

        if cluster_chars == 0 {
            return start_xpos;
        }

        if cluster_offset > 0
            && cluster_offset < cluster_chars
            && let Some(x_pos) = self.ligature_caret_x(
                analysis,
                start_glyph_pos,
                end_glyph_pos,
                cluster_offset,
                start_xpos,
                end_xpos,
            )
        {
            return x_pos;
        }

        ((cluster_chars - cluster_offset) * start_xpos + cluster_offset * end_xpos) / cluster_chars
    }

    fn ligature_caret_x(
        &self,
        analysis: &Analysis,
        start_glyph_pos: i32,
        end_glyph_pos: i32,
        cluster_offset: i32,
        start_xpos: i32,
        end_xpos: i32,
    ) -> Option<i32> {
        let font = analysis.font.as_ref()?;
        let hb_font = font.hb_font();

        let glyph_pos = if start_glyph_pos == end_glyph_pos {
            start_glyph_pos
        } else {
            let mut glyph_pos = -1;
            for i in start_glyph_pos..=end_glyph_pos {
                let glyph = self.glyphs[i as usize].glyph;
                if hb_font.glyph_class(glyph) != OT_LAYOUT_GLYPH_CLASS_MARK {
                    if glyph_pos != -1 {
                        return None;
                    }
                    glyph_pos = i;
                }
            }
            if glyph_pos == -1 {
                return None;
            }
            glyph_pos
        };

        let rtl = analysis.level & 1 != 0;
        let info = &self.glyphs[glyph_pos as usize];
        let direction = if rtl {
            DIRECTION_RIGHT_TO_LEFT
        } else {
            DIRECTION_LEFT_TO_RIGHT
        };
        let (num_carets, caret_count, caret) =
            hb_font.ligature_caret(direction, info.glyph, (cluster_offset - 1) as u32);
        if caret_count == 0 || num_carets == 0 {
            return None;
        }

        let x_pos = if rtl {
            end_xpos + caret
        } else {
            start_xpos + caret
        };
        Some(x_pos + info.geometry.x_offset)
    }

    pub fn x_to_index(&self, text: &str, analysis: &Analysis, x_pos: i32) -> (i32, bool) {
        let bytes = text.as_bytes();
        let length = bytes.len() as i32;
        let num_glyphs = self.glyphs.len() as i32;
        let mut start_xpos = 0;
        let mut end_xpos = 0;
        let mut width = 0;
        let mut start_index = -1;
        let mut end_index = -1;
        let mut cluster_chars = 0;
        let mut found = false;
        let rtl = analysis.level & 1 != 0;

        let geometry_width = |i: i32| self.glyphs[i as usize].geometry.width;
        let cluster = |i: i32| self.log_clusters[i as usize];

        if rtl {
            for i in (0..num_glyphs).rev() {
                width += geometry_width(i);
            }
            for i in (0..num_glyphs).rev() {
                if cluster(i) != start_index {
                    if found {
                        end_index = cluster(i);
                        end_xpos = width;
                        break;
                    }
                    start_index = cluster(i);
                    start_xpos = width;
                }
                width -= geometry_width(i);
                if width <= x_pos && x_pos < width + geometry_width(i) {
                    found = true;
                }
            }
        } else {
            for i in 0..num_glyphs {
                if cluster(i) != start_index {
                    if found {
                        end_index = cluster(i);
                        end_xpos = width;
                        break;
                    }
                    start_index = cluster(i);
                    start_xpos = width;
                }
                if width <= x_pos && x_pos < width + geometry_width(i) {
                    found = true;
                }
                width += geometry_width(i);
            }
        }

        if end_index == -1 {
            end_index = length;
            end_xpos = if rtl { 0 } else { width };
        }

        let start = clamp_index(bytes, start_index);
        let end = clamp_index(bytes, end_index);
        let mut p = start;
        while p < end {
            p = next_char(bytes, p);
            cluster_chars += 1;
        }

        if start_xpos == end_xpos {
            return (start_index, false);
        }

        let cp = (f64::from(x_pos - start_xpos) * f64::from(cluster_chars))
            / f64::from(end_xpos - start_xpos);

        if start_xpos < end_xpos {
            let mut p = start;
            let mut i = 0;
            while f64::from(i + 1) <= cp {
                p = next_char(bytes, p);
                i += 1;
            }
            let trailing = cp - f64::from(cp as i32) >= 0.5;
            (start_index + (p - start) as i32, trailing)
        } else {
            let mut p = start;
            let mut i = 0;
            while f64::from(i + 1) < cp {
                p = next_char(bytes, p);
                i += 1;
            }
            let cp_flip = f64::from(cluster_chars) - cp;
            let trailing = cp_flip - f64::from(cp_flip as i32) < 0.5;
            (start_index + (p - start) as i32, trailing)
        }
    }
}
