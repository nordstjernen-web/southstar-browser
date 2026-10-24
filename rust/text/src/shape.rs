//! Southstar text — shaping: turning an item's characters into positioned glyphs through HarfBuzz, with hyphens, text transforms, hex boxes and the shape cache.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::cell::Cell;
use std::collections::HashMap;

use crate::attrs::AttrValue;
use crate::context::{Context, LayoutHooks, SampleItem, SampleWidths, set_layout_hooks};
use crate::ffi::harfbuzz::{
    BUFFER_FLAG_BOT, BUFFER_FLAG_EOT, BUFFER_FLAG_PRESERVE_DEFAULT_IGNORABLES,
    BUFFER_FLAG_PRODUCE_UNSAFE_TO_CONCAT, DIRECTION_LEFT_TO_RIGHT, DIRECTION_TOP_TO_BOTTOM,
    Feature, GLYPH_FLAG_UNSAFE_TO_CONCAT, GlyphExtents, HbBuffer, HbFont, ShapeFontHandler,
    SubFont, direction_reverse,
};
use crate::font::{Font, get_ignorable, glyph_extents_of};
use crate::font_description::FontDescription;
use crate::glyph::GlyphString;
use crate::item::{Analysis, Item, analysis_flags};
use crate::itemize::itemize_with_font;
use crate::language::Language;
use crate::shape_cache::{
    HYPHEN_ASCII, HYPHEN_MISSING, HYPHEN_NONE, HYPHEN_TRIMMED, HYPHEN_UNICODE, MAX_SEGMENTS,
    ShapeKey, shape_cache_font_map_changed, shape_cache_insert, shape_cache_insert_range,
    shape_cache_key_init, shape_cache_lookup, shape_cache_lookup_at, shape_cache_matches,
    shape_cache_split,
};
use crate::types::{
    GLYPH_EMPTY, GLYPH_UNKNOWN_FLAG, LogAttr, TextTransform, get_unknown_glyph, shape_flags,
    show_flags, units_round,
};
use crate::unicode::{UnicodeType, is_zero_width, unichar_type};
use crate::utf8::{char_at, next_char, pango_utf8_strlen, prev_char};
use crate::utils::{caches_enabled, caches_verifying};

#[cfg(test)]
#[path = "tests/shaping.rs"]
mod tests;

const STAND_IN_END: u32 = 1 << 14;
const MAX_FEATURES: usize = 32;
const KEY_AT_ITEM_START: u32 = 1 << 9;
const KEY_AT_ITEM_END: u32 = 1 << 10;
const KEY_WHOLE_ITEM: u32 = KEY_AT_ITEM_START | KEY_AT_ITEM_END;
const KEY_CAPITALIZE_WITHOUT_LOG_ATTRS: u32 = 1 << 8;

thread_local! {
    static THREAD_BUFFER: Cell<Option<HbBuffer>> = const { Cell::new(None) };
}

fn acquire_buffer() -> HbBuffer {
    THREAD_BUFFER
        .try_with(Cell::take)
        .ok()
        .flatten()
        .unwrap_or_else(HbBuffer::new)
}

fn release_buffer(mut buffer: HbBuffer) {
    buffer.reset();
    let _ = THREAD_BUFFER.try_with(|slot| {
        let previous = slot.replace(Some(buffer));
        if let Some(previous) = previous {
            slot.set(Some(previous));
        }
    });
}

pub fn is_default_ignorable(ch: u32) -> bool {
    let plane = ch >> 16;
    if plane == 0 {
        match ch >> 8 {
            0x00 => ch == 0x00ad,
            0x03 => ch == 0x034f,
            0x06 => ch == 0x061c,
            0x17 => (0x17b4..=0x17b5).contains(&ch),
            0x18 => (0x180b..=0x180e).contains(&ch),
            0x20 => {
                (0x200b..=0x200f).contains(&ch)
                    || (0x202a..=0x202e).contains(&ch)
                    || (0x2060..=0x206f).contains(&ch)
            }
            0xfe => (0xfe00..=0xfe0f).contains(&ch) || ch == 0xfeff,
            0xff => (0xfff0..=0xfff8).contains(&ch),
            _ => false,
        }
    } else {
        match plane {
            0x01 => (0x1d173..=0x1d17a).contains(&ch),
            0x0e => (0xe0000..=0xe0fff).contains(&ch),
            _ => false,
        }
    }
}

struct ShapeContext<'f> {
    font: &'f Font,
    show_flags: u32,
    stand_in_base: u32,
    stand_ins: Vec<u32>,
    stand_in_index: HashMap<u32, u32>,
}

impl ShapeContext<'_> {
    fn stand_in_for(&mut self, made_up: u32) -> u32 {
        if let Some(&index) = self.stand_in_index.get(&made_up) {
            return self.stand_in_base.wrapping_add(index);
        }
        let stand_in = self.stand_in_base.wrapping_add(self.stand_ins.len() as u32);
        if stand_in >= STAND_IN_END {
            return made_up;
        }
        self.stand_in_index
            .insert(made_up, self.stand_ins.len() as u32);
        self.stand_ins.push(made_up);
        stand_in
    }

    fn made_up_for(&self, glyph: u32) -> u32 {
        if glyph >= self.stand_in_base {
            let index = (glyph - self.stand_in_base) as usize;
            if let Some(&made_up) = self.stand_ins.get(index) {
                return made_up;
            }
        }
        glyph
    }

    fn nominal_or_unknown_glyph(&self, parent: &HbFont, mut unicode: u32) -> (bool, u32) {
        if self.show_flags != 0 {
            if self.show_flags & show_flags::SPACES != 0
                && unichar_type(unicode) == UnicodeType::SpaceSeparator
            {
                if unicode == 0x20 {
                    unicode = 0x2423;
                } else {
                    return (true, get_unknown_glyph(unicode));
                }
            }

            if self.show_flags & show_flags::IGNORABLES != 0 && is_default_ignorable(unicode) {
                let glyph = if get_ignorable(unicode).is_some() {
                    get_unknown_glyph(unicode)
                } else {
                    GLYPH_EMPTY
                };
                return (true, glyph);
            }

            if self.show_flags & show_flags::LINE_BREAKS != 0 && unicode == 0x2028 {
                return (true, get_unknown_glyph(unicode));
            }
        }

        let mut glyph = 0;
        if parent.nominal_glyph_into(unicode, &mut glyph) {
            return (true, glyph);
        }

        if unichar_type(unicode) == UnicodeType::SpaceSeparator {
            if unicode == 0x20 {
                return (true, get_unknown_glyph(0x20));
            }
            return (false, glyph);
        }

        (unicode > 0x10FFFF, get_unknown_glyph(unicode))
    }
}

impl ShapeFontHandler for ShapeContext<'_> {
    fn nominal_glyph(&mut self, parent: &HbFont, unicode: u32) -> (bool, u32) {
        let (found, mut glyph) = self.nominal_or_unknown_glyph(parent, unicode);
        if glyph > 0xFFFF {
            glyph = self.stand_in_for(glyph);
        }
        (found, glyph)
    }

    fn h_advance(&mut self, parent: &HbFont, glyph: u32) -> i32 {
        let glyph = self.made_up_for(glyph);
        if glyph & GLYPH_UNKNOWN_FLAG != 0 {
            return self.font.glyph_extents(glyph).1.width;
        }
        parent.glyph_h_advance(glyph)
    }

    fn v_advance(&mut self, parent: &HbFont, glyph: u32) -> i32 {
        let glyph = self.made_up_for(glyph);
        if glyph & GLYPH_UNKNOWN_FLAG != 0 {
            return self.font.glyph_extents(glyph).1.height;
        }
        parent.glyph_v_advance(glyph)
    }

    fn glyph_extents(&mut self, parent: &HbFont, glyph: u32) -> Option<GlyphExtents> {
        let glyph = self.made_up_for(glyph);
        if glyph & GLYPH_UNKNOWN_FLAG != 0 {
            let ink = self.font.glyph_extents(glyph).0;
            return Some(GlyphExtents {
                x_bearing: ink.x,
                y_bearing: ink.y,
                width: ink.width,
                height: ink.height,
            });
        }
        parent.glyph_extents(glyph)
    }
}

fn find_show_flags(analysis: &Analysis) -> u32 {
    analysis
        .extra_attrs
        .iter()
        .filter_map(|attr| match attr.value {
            AttrValue::Show(flags) => Some(flags as u32),
            _ => None,
        })
        .fold(0, |flags, value| flags | value)
}

fn find_text_transform(analysis: &Analysis) -> TextTransform {
    let mut transform = TextTransform::None;
    for attr in &analysis.extra_attrs {
        if let AttrValue::TextTransform(value) = attr.value {
            transform = TextTransform::from_i32(value).unwrap_or(TextTransform::None);
        }
    }
    transform
}

fn log_attr_at(log_attrs: Option<&[LogAttr]>, index: usize) -> Option<LogAttr> {
    log_attrs.and_then(|attrs| attrs.get(index).copied())
}

fn find_hyphen(analysis: &Analysis, log_attrs: Option<&[LogAttr]>, num_chars: usize) -> u32 {
    if analysis.flags & analysis_flags::NEED_HYPHEN == 0 {
        return HYPHEN_NONE;
    }
    let Some(font) = analysis.font.as_ref() else {
        return HYPHEN_NONE;
    };
    let hb_font = font.hb_font();
    let mut glyph = 0;
    let mut hyphen = if hb_font.nominal_glyph_into(0x2010, &mut glyph) {
        HYPHEN_UNICODE
    } else if hb_font.nominal_glyph_into(u32::from('-'), &mut glyph) {
        HYPHEN_ASCII
    } else {
        HYPHEN_MISSING
    };
    if log_attr_at(log_attrs, num_chars).is_some_and(LogAttr::break_removes_preceding) {
        hyphen |= HYPHEN_TRIMMED;
    }
    hyphen
}

struct CutSafety<'s> {
    starts: &'s [u32],
    unsafe_cut: Vec<bool>,
}

impl CutSafety<'_> {
    fn mark_unsafe(&mut self, cluster: i32) {
        let Ok(cluster) = u32::try_from(cluster) else {
            return;
        };
        let index = self.starts.partition_point(|&start| start < cluster);
        if self.starts.get(index) == Some(&cluster) {
            self.unsafe_cut[index] = true;
        }
    }

    fn mark_every_cut_unsafe(&mut self) {
        self.unsafe_cut.fill(true);
    }
}

struct ShapeRequest<'r> {
    paragraph: &'r [u8],
    item_offset: usize,
    item_length: usize,
    analysis: &'r Analysis,
    log_attrs: Option<&'r [LogAttr]>,
    num_chars: usize,
    flags: u32,
}

fn add_transformed_text(
    buffer: &mut HbBuffer,
    request: &ShapeRequest<'_>,
    end: i64,
    transform: TextTransform,
) {
    for_each_transformed_char(request, end, transform, |ch, cluster| {
        buffer.add(ch, cluster)
    });
}

fn for_each_transformed_char(
    request: &ShapeRequest<'_>,
    end: i64,
    transform: TextTransform,
    mut add: impl FnMut(u32, u32),
) {
    let paragraph = request.paragraph;
    let end = usize::try_from(end).unwrap_or(0);
    let mut p = request.item_offset;
    let mut i = 0usize;
    while p < end {
        let next = next_char(paragraph, p).min(paragraph.len());
        let mut ch = char_at(paragraph, p);
        let mut mapped: Option<String> = None;
        let single = core::str::from_utf8(&paragraph[p..next]).unwrap_or("");

        match transform {
            TextTransform::Lowercase => {
                if southstar_unicode::is_alnum(ch) {
                    mapped = Some(southstar_unicode::lower_string(single));
                }
            }
            TextTransform::Uppercase => {
                if southstar_unicode::is_alnum(ch) {
                    mapped = Some(southstar_unicode::upper_string(single));
                }
            }
            TextTransform::Capitalize => {
                if log_attr_at(request.log_attrs, i).is_some_and(LogAttr::is_word_start) {
                    ch = crate::unicode::to_title(ch);
                }
            }
            TextTransform::None => {}
        }

        match mapped {
            Some(text) => {
                for mapped_char in text.chars() {
                    if mapped_char == '\0' {
                        break;
                    }
                    add(u32::from(mapped_char), p as u32);
                }
            }
            None => add(ch, p as u32),
        }

        p = next_char(paragraph, p);
        i += 1;
    }
}

fn hb_shape(
    request: &ShapeRequest<'_>,
    glyphs: &mut GlyphString,
    cuts: Option<&mut CutSafety<'_>>,
) {
    let analysis = request.analysis;
    let Some(font) = analysis.font.as_ref() else {
        return;
    };
    let paragraph = request.paragraph;
    let item_offset = request.item_offset;
    let item_length = request.item_length;
    let hb_parent = font.hb_font();
    let shape_show_flags = find_show_flags(analysis);
    let mut context = ShapeContext {
        font,
        show_flags: shape_show_flags,
        stand_in_base: hb_parent.face_glyph_count(),
        stand_ins: Vec::new(),
        stand_in_index: HashMap::new(),
    };
    let mut buffer = acquire_buffer();
    let transform = find_text_transform(analysis);

    let mut direction = if analysis.gravity.is_vertical() {
        DIRECTION_TOP_TO_BOTTOM
    } else {
        DIRECTION_LEFT_TO_RIGHT
    };
    if analysis.level & 1 != 0 {
        direction = direction_reverse(direction);
    }
    if analysis.gravity.is_improper() {
        direction = direction_reverse(direction);
    }

    let mut buffer_flags = BUFFER_FLAG_BOT | BUFFER_FLAG_EOT;
    if shape_show_flags & show_flags::IGNORABLES != 0 {
        buffer_flags |= BUFFER_FLAG_PRESERVE_DEFAULT_IGNORABLES;
    }
    if cuts.is_some() {
        buffer_flags |= BUFFER_FLAG_PRODUCE_UNSAFE_TO_CONCAT;
    }

    buffer.set_direction(direction);
    buffer.set_script(southstar_unicode::script_tag(analysis.script.0 as u8));
    buffer.set_language(analysis.language.map(Language::as_str));
    buffer.set_cluster_level_monotone_characters();
    buffer.set_flags(buffer_flags);
    buffer.set_invisible_glyph(GLYPH_EMPTY);

    let item_start = item_offset as i64;
    let mut item_length = item_length as i64;
    let mut hyphen_index = 0i64;
    if analysis.flags & analysis_flags::NEED_HYPHEN != 0 {
        let p = item_offset + request.item_length;
        let last_char_len = (p - prev_char(paragraph, p)) as i64;
        hyphen_index = item_start + item_length - last_char_len;
        if log_attr_at(request.log_attrs, request.num_chars)
            .is_some_and(LogAttr::break_removes_preceding)
        {
            item_length -= last_char_len;
        }
    }

    buffer.add_utf8(paragraph, item_start, item_start, 0);

    if transform == TextTransform::None {
        buffer.add_utf8(paragraph, item_start + item_length, item_start, item_length);
    } else {
        add_transformed_text(&mut buffer, request, item_start + item_length, transform);
    }

    buffer.add_utf8(
        paragraph,
        paragraph.len() as i64,
        item_start + item_length,
        0,
    );

    let mut features: Vec<Feature> = Vec::with_capacity(MAX_FEATURES);
    {
        let sub_font = SubFont::new(hb_parent, &mut context);

        if analysis.flags & analysis_flags::NEED_HYPHEN != 0 {
            if sub_font.nominal_glyph(0x2010).is_some() {
                buffer.add(0x2010, hyphen_index as u32);
            } else if sub_font.nominal_glyph(u32::from('-')).is_some() {
                buffer.add(u32::from('-'), hyphen_index as u32);
            }
        }

        analysis.collect_features(&mut features, MAX_FEATURES);
        sub_font.shape(&mut buffer, &features);
    }

    if analysis.gravity.is_improper() {
        buffer.reverse();
    }

    let infos = buffer.glyph_infos();
    let num_glyphs = infos.len();
    glyphs.set_size(num_glyphs);
    let font_is_color = hb_parent.has_color();
    let mut last_cluster = -1;
    let mut cuts = cuts;

    for (i, hb_glyph) in infos.iter().enumerate() {
        let cluster = hb_glyph.cluster.wrapping_sub(item_offset as u32) as i32;
        glyphs.log_clusters[i] = cluster;
        let info = &mut glyphs.glyphs[i];
        info.glyph = context.made_up_for(hb_glyph.codepoint);
        info.attr.is_cluster_start = cluster != last_cluster;
        info.attr.is_color = font_is_color && hb_parent.glyph_has_color(hb_glyph.codepoint);
        if let Some(cuts) = cuts.as_deref_mut()
            && hb_glyph.glyph_flags() & GLYPH_FLAG_UNSAFE_TO_CONCAT != 0
        {
            cuts.mark_unsafe(cluster);
        }
        last_cluster = cluster;
    }

    let positions = buffer.glyph_positions();
    let vertical = analysis.gravity.is_vertical();
    for (info, position) in glyphs.glyphs.iter_mut().zip(positions) {
        if vertical {
            info.geometry.width = position.y_advance.wrapping_neg();
            info.geometry.x_offset = position.y_offset.wrapping_neg();
            info.geometry.y_offset = position.x_offset.wrapping_neg();
        } else {
            info.geometry.width = position.x_advance;
            info.geometry.x_offset = position.x_offset;
            info.geometry.y_offset = position.y_offset.wrapping_neg();
        }
    }

    release_buffer(buffer);
}

#[cfg(not(feature = "rust-shaper"))]
fn rust_shape(_: &ShapeRequest<'_>, _: &mut GlyphString, _: Option<&mut CutSafety<'_>>) -> bool {
    false
}

#[cfg(feature = "rust-shaper")]
fn rust_shaper_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("NS_TEXT_SHAPER").map_or(true, |v| v != "harfbuzz"))
}

#[cfg(feature = "rust-shaper")]
fn rust_shape(
    request: &ShapeRequest<'_>,
    glyphs: &mut GlyphString,
    cuts: Option<&mut CutSafety<'_>>,
) -> bool {
    use crate::rust_shaper::{self, ShapeDirection, buffer_flags, glyph_flags};

    if !rust_shaper_enabled() {
        return false;
    }
    let analysis = request.analysis;
    let Some(font) = analysis.font.as_ref() else {
        return false;
    };
    let Ok(paragraph) = core::str::from_utf8(request.paragraph) else {
        return false;
    };
    let Some(face) = rust_shaper::shaper_face(font) else {
        return false;
    };
    let item_offset = request.item_offset;
    let shape_show_flags = find_show_flags(analysis);
    let transform = find_text_transform(analysis);

    let mut direction = if analysis.gravity.is_vertical() {
        ShapeDirection::TopToBottom
    } else {
        ShapeDirection::LeftToRight
    };
    if analysis.level & 1 != 0 {
        direction = direction.reverse();
    }
    if analysis.gravity.is_improper() {
        direction = direction.reverse();
    }

    let mut flags = buffer_flags::BEGINNING_OF_TEXT | buffer_flags::END_OF_TEXT;
    if shape_show_flags & show_flags::IGNORABLES != 0 {
        flags |= buffer_flags::PRESERVE_DEFAULT_IGNORABLES;
    }
    if cuts.is_some() {
        flags |= buffer_flags::PRODUCE_UNSAFE_TO_CONCAT;
    }

    let mut item_length = request.item_length;
    let mut hyphen = None;
    if analysis.flags & analysis_flags::NEED_HYPHEN != 0 {
        let p = item_offset + request.item_length;
        let last_char_len = p - prev_char(request.paragraph, p);
        let hyphen_index = (item_offset + item_length - last_char_len) as u32;
        if log_attr_at(request.log_attrs, request.num_chars)
            .is_some_and(LogAttr::break_removes_preceding)
        {
            item_length -= last_char_len;
        }
        if face.nominal_glyph(0x2010).is_some() {
            hyphen = Some((0x2010, hyphen_index));
        } else if face.nominal_glyph(u32::from('-')).is_some() {
            hyphen = Some((u32::from('-'), hyphen_index));
        }
    }
    let item_end = item_offset + item_length;
    if !paragraph.is_char_boundary(item_offset) || !paragraph.is_char_boundary(item_end) {
        return false;
    }

    let mut transformed = Vec::new();
    if transform != TextTransform::None {
        for_each_transformed_char(request, item_end as i64, transform, |ch, cluster| {
            transformed.push((ch, cluster));
        });
    }

    let mut features: Vec<Feature> = Vec::with_capacity(MAX_FEATURES);
    analysis.collect_features(&mut features, MAX_FEATURES);

    let shape_request = rust_shaper::ShapeRequest {
        paragraph,
        item_start: item_offset,
        item_end,
        transformed: (transform != TextTransform::None).then_some(transformed.as_slice()),
        hyphen,
        direction,
        script: analysis.script,
        language: analysis.language.map(Language::as_str),
        flags,
        show_flags: shape_show_flags,
        features: &features,
    };
    let Some(mut shaped) = rust_shaper::shape(font, &shape_request) else {
        return false;
    };
    if analysis.gravity.is_improper() {
        shaped.reverse();
    }

    glyphs.set_size(shaped.len());
    let font_is_color = face.has_color();
    let vertical = analysis.gravity.is_vertical();
    let mut last_cluster = -1;
    let mut cuts = cuts;
    for (i, shaped_glyph) in shaped.iter().enumerate() {
        let cluster = shaped_glyph.cluster.wrapping_sub(item_offset as u32) as i32;
        glyphs.log_clusters[i] = cluster;
        let info = &mut glyphs.glyphs[i];
        info.glyph = shaped_glyph.glyph;
        info.attr.is_cluster_start = cluster != last_cluster;
        info.attr.is_color = font_is_color && face.glyph_has_color(shaped_glyph.glyph);
        if let Some(cuts) = cuts.as_deref_mut()
            && shaped_glyph.flags & glyph_flags::UNSAFE_TO_CONCAT != 0
        {
            cuts.mark_unsafe(cluster);
        }
        last_cluster = cluster;
        if vertical {
            info.geometry.width = shaped_glyph.y_advance.wrapping_neg();
            info.geometry.x_offset = shaped_glyph.y_offset.wrapping_neg();
            info.geometry.y_offset = shaped_glyph.x_offset.wrapping_neg();
        } else {
            info.geometry.width = shaped_glyph.x_advance;
            info.geometry.x_offset = shaped_glyph.x_offset;
            info.geometry.y_offset = shaped_glyph.y_offset.wrapping_neg();
        }
    }
    true
}

fn fallback_shape(text: &[u8], analysis: &Analysis, glyphs: &mut GlyphString) {
    let n_chars = pango_utf8_strlen(text, text.len()) as usize;
    glyphs.set_size(n_chars);

    let mut p = 0usize;
    let mut cluster = 0i32;
    for i in 0..n_chars {
        let wc = char_at(text, p);
        if unichar_type(wc) != UnicodeType::NonSpacingMark {
            cluster = p as i32;
        }
        let glyph = if is_zero_width(wc) {
            GLYPH_EMPTY
        } else {
            get_unknown_glyph(wc)
        };
        let (_, logical_rect) = glyph_extents_of(analysis.font.as_ref(), glyph);
        let info = &mut glyphs.glyphs[i];
        info.glyph = glyph;
        info.geometry.x_offset = 0;
        info.geometry.y_offset = 0;
        info.geometry.width = logical_rect.width;
        info.attr.is_color = false;
        glyphs.log_clusters[i] = cluster;
        p = next_char(text, p);
    }

    if analysis.level & 1 != 0 {
        let len = glyphs.glyphs.len();
        glyphs.reverse_range(0, len);
    }
}

fn hint(value: i32, to_device: f64, from_device: f64) -> i32 {
    (f64::from(units_round((f64::from(value) * to_device) as i32)) * from_device) as i32
}

fn shape_internal_uncached(
    request: &ShapeRequest<'_>,
    glyphs: &mut GlyphString,
    cuts: Option<&mut CutSafety<'_>>,
) {
    let analysis = request.analysis;
    glyphs.set_size(0);

    let mut cuts = cuts;
    if analysis.font.is_some() && !rust_shape(request, glyphs, cuts.as_deref_mut()) {
        hb_shape(request, glyphs, cuts.as_deref_mut());
    }

    if glyphs.glyphs.is_empty() {
        if let Some(cuts) = cuts {
            cuts.mark_every_cut_unsafe();
        }
        let item_text =
            &request.paragraph[request.item_offset..request.item_offset + request.item_length];
        fallback_shape(item_text, analysis, glyphs);
        if glyphs.glyphs.is_empty() {
            return;
        }
    }

    let mut last_cluster = glyphs.log_clusters[0].wrapping_sub(1);
    for (info, &cluster) in glyphs.glyphs.iter_mut().zip(&glyphs.log_clusters) {
        if cluster != last_cluster {
            info.attr.is_cluster_start = true;
            last_cluster = cluster;
        } else {
            info.attr.is_cluster_start = false;
        }

        if info.geometry.width < 0 {
            info.geometry.width = info.geometry.width.wrapping_neg();
            info.geometry.x_offset = info.geometry.x_offset.wrapping_add(info.geometry.width);
        }
    }

    let last = glyphs.glyphs.len() - 1;
    if analysis.level & 1 != 0 && glyphs.log_clusters[0] < glyphs.log_clusters[last] {
        glyphs.reverse_range(0, last + 1);
    }

    if request.flags & shape_flags::ROUND_POSITIONS != 0 {
        let hinted_font = analysis.font.as_ref().filter(|font| font.is_hinted());
        if let Some(font) = hinted_font {
            let (mut x_scale_inv, mut y_scale_inv) = font.scale_factors();
            if analysis.gravity.is_improper() {
                x_scale_inv = -x_scale_inv;
                y_scale_inv = -y_scale_inv;
            }
            let x_scale = 1.0 / x_scale_inv;
            let y_scale = 1.0 / y_scale_inv;

            if x_scale == 1.0 && y_scale == 1.0 {
                for info in &mut glyphs.glyphs {
                    info.geometry.width = units_round(info.geometry.width);
                }
            } else {
                for info in &mut glyphs.glyphs {
                    info.geometry.width = hint(info.geometry.width, x_scale_inv, x_scale);
                    info.geometry.x_offset = hint(info.geometry.x_offset, x_scale_inv, x_scale);
                    info.geometry.y_offset = hint(info.geometry.y_offset, y_scale_inv, y_scale);
                }
            }
        } else {
            for info in &mut glyphs.glyphs {
                info.geometry.width = units_round(info.geometry.width);
                info.geometry.x_offset = units_round(info.geometry.x_offset);
                info.geometry.y_offset = units_round(info.geometry.y_offset);
            }
        }
    }
}

fn item_runs_backwards(analysis: &Analysis) -> bool {
    (analysis.level & 1 != 0) != analysis.gravity.is_improper()
}

fn glyph_strings_agree(a: &GlyphString, b: &GlyphString) -> bool {
    a.glyphs.len() == b.glyphs.len()
        && a.glyphs.iter().zip(&b.glyphs).all(|(x, y)| x == y)
        && a.log_clusters == b.log_clusters
}

struct SegmentPlan<'p> {
    request: &'p ShapeRequest<'p>,
    show_flags: u32,
    features: &'p [Feature],
    starts: &'p [u32],
}

impl SegmentPlan<'_> {
    fn segment_count(&self) -> usize {
        self.starts.len()
    }

    fn segment_key(&self, i: usize) -> Option<ShapeKey<'_>> {
        let n = self.starts.len();
        let start = self.starts[i] as usize;
        let end = if i + 1 < n {
            self.starts[i + 1] as usize
        } else {
            self.request.item_length
        };
        let open_edges = if i == 0 { KEY_AT_ITEM_START } else { 0 }
            | if i + 1 == n { KEY_AT_ITEM_END } else { 0 };
        shape_cache_key_init(
            self.request.analysis,
            self.request.paragraph,
            self.request.item_offset + start,
            end - start,
            self.request.flags,
            self.show_flags,
            TextTransform::None as u32 | open_edges,
            HYPHEN_NONE,
            self.features,
        )
    }

    fn whole_item_key(&self) -> Option<ShapeKey<'_>> {
        shape_cache_key_init(
            self.request.analysis,
            self.request.paragraph,
            self.request.item_offset,
            self.request.item_length,
            self.request.flags,
            self.show_flags,
            TextTransform::None as u32 | KEY_WHOLE_ITEM,
            HYPHEN_NONE,
            self.features,
        )
    }

    fn insert_segments(
        &self,
        glyphs: &GlyphString,
        backwards: bool,
        cuts: &CutSafety<'_>,
    ) -> usize {
        let n = self.segment_count();
        let mut first = vec![0usize; n];
        let mut count = vec![0usize; n];
        let item_length = self.request.item_length as i32;

        for (g, &cluster) in glyphs.log_clusters.iter().enumerate() {
            if cluster < 0 || cluster >= item_length {
                return n;
            }
            let cluster = cluster as u32;
            let Some(seg) = (0..n).rev().find(|&seg| cluster >= self.starts[seg]) else {
                return n;
            };
            if count[seg] == 0 {
                first[seg] = g;
            } else if first[seg] + count[seg] != g {
                return n;
            }
            count[seg] += 1;
        }

        if count.iter().sum::<usize>() != glyphs.glyphs.len() {
            return n;
        }

        if count.contains(&0) {
            return n;
        }

        for i in 1..n {
            let (a, b) = if backwards {
                (n - i, n - i - 1)
            } else {
                (i - 1, i)
            };
            if first[a] > first[b] {
                return n;
            }
        }

        let mut refused = 0;
        for i in 0..n {
            if (i > 0 && cuts.unsafe_cut[i]) || (i + 1 < n && cuts.unsafe_cut[i + 1]) {
                refused += 1;
                continue;
            }
            let Some(key) = self.segment_key(i) else {
                refused += 1;
                continue;
            };
            shape_cache_insert_range(&key, glyphs, first[i], count[i], self.starts[i] as i32);
        }
        refused
    }

    fn serve_from_cache(&self, backwards: bool, glyphs: &mut GlyphString) -> bool {
        let n = self.segment_count();
        let mut at = 0usize;
        let mut served = 0;
        while served < n {
            let seg = if backwards { n - 1 - served } else { served };
            let before = at;
            let Some(key) = self.segment_key(seg) else {
                break;
            };
            if !shape_cache_lookup_at(&key, glyphs, at, self.starts[seg] as i32) {
                break;
            }
            at = glyphs.glyphs.len();
            if at == before {
                break;
            }
            served += 1;
        }

        if served == n {
            return true;
        }

        self.whole_item_key()
            .is_some_and(|key| shape_cache_lookup(&key, glyphs))
    }

    fn store_in_cache(&self, glyphs: &GlyphString, backwards: bool, cuts: &CutSafety<'_>) {
        if self.insert_segments(glyphs, backwards, cuts) == 0 {
            return;
        }
        if let Some(key) = self.whole_item_key() {
            shape_cache_insert(&key, glyphs);
        }
    }
}

fn shape_by_segment(
    request: &ShapeRequest<'_>,
    glyphs: &mut GlyphString,
    show_flags: u32,
    features: &[Feature],
) -> bool {
    let item_text =
        &request.paragraph[request.item_offset..request.item_offset + request.item_length];
    let starts = shape_cache_split(
        request.paragraph,
        request.item_offset,
        request.item_length,
        MAX_SEGMENTS,
    );
    if starts.len() < 2 {
        return false;
    }
    let backwards = item_runs_backwards(request.analysis);
    let plan = SegmentPlan {
        request,
        show_flags,
        features,
        starts: &starts,
    };
    let mut cuts = CutSafety {
        starts: &starts,
        unsafe_cut: vec![false; starts.len()],
    };

    if caches_verifying() {
        let mut served = GlyphString::new();
        let was_served = plan.serve_from_cache(backwards, &mut served);
        shape_internal_uncached(request, glyphs, Some(&mut cuts));
        if was_served && !glyph_strings_agree(&served, glyphs) {
            eprintln!(
                "shape cache mismatch on '{}'",
                String::from_utf8_lossy(item_text)
            );
        }
        plan.store_in_cache(glyphs, backwards, &cuts);
        return true;
    }

    if plan.serve_from_cache(backwards, glyphs) {
        return true;
    }

    shape_internal_uncached(request, glyphs, Some(&mut cuts));
    plan.store_in_cache(glyphs, backwards, &cuts);
    true
}

fn shape_internal(request: &ShapeRequest<'_>, glyphs: &mut GlyphString) {
    let analysis = request.analysis;
    if !caches_enabled() || analysis.font.is_none() {
        shape_internal_uncached(request, glyphs, None);
        return;
    }

    let mut features: Vec<Feature> = Vec::with_capacity(MAX_FEATURES);
    analysis.collect_features(&mut features, MAX_FEATURES);

    let shape_show_flags = find_show_flags(analysis);
    let transform = find_text_transform(analysis);
    let hyphen = find_hyphen(analysis, request.log_attrs, request.num_chars);

    if transform == TextTransform::None
        && hyphen == HYPHEN_NONE
        && shape_by_segment(request, glyphs, shape_show_flags, &features)
    {
        return;
    }

    let mut transform_key = transform as u32 | KEY_WHOLE_ITEM;
    if transform == TextTransform::Capitalize && request.log_attrs.is_none() {
        transform_key |= KEY_CAPITALIZE_WITHOUT_LOG_ATTRS;
    }

    let key = shape_cache_key_init(
        analysis,
        request.paragraph,
        request.item_offset,
        request.item_length,
        request.flags,
        shape_show_flags,
        transform_key,
        hyphen,
        &features,
    );

    if let Some(key) = key.as_ref()
        && !caches_verifying()
        && shape_cache_lookup(key, glyphs)
    {
        return;
    }

    shape_internal_uncached(request, glyphs, None);

    let Some(key) = key else {
        return;
    };

    if caches_verifying() && !shape_cache_matches(&key, glyphs) {
        let item_text =
            &request.paragraph[request.item_offset..request.item_offset + request.item_length];
        eprintln!(
            "shape cache mismatch on '{}'",
            String::from_utf8_lossy(item_text)
        );
    }

    shape_cache_insert(&key, glyphs);
}

fn clamp_item(paragraph_text: &str, item_offset: usize, item_length: usize) -> (usize, usize) {
    let item_offset = item_offset.min(paragraph_text.len());
    let item_length = item_length.min(paragraph_text.len() - item_offset);
    (item_offset, item_length)
}

pub fn shape_with_flags(
    paragraph_text: &str,
    item_offset: usize,
    item_length: usize,
    analysis: &Analysis,
    glyphs: &mut GlyphString,
    flags: u32,
) {
    let (item_offset, item_length) = clamp_item(paragraph_text, item_offset, item_length);
    shape_internal(
        &ShapeRequest {
            paragraph: paragraph_text.as_bytes(),
            item_offset,
            item_length,
            analysis,
            log_attrs: None,
            num_chars: 0,
            flags,
        },
        glyphs,
    );
}

pub fn shape_full(
    paragraph_text: &str,
    item_offset: usize,
    item_length: usize,
    analysis: &Analysis,
    glyphs: &mut GlyphString,
) {
    shape_with_flags(
        paragraph_text,
        item_offset,
        item_length,
        analysis,
        glyphs,
        shape_flags::NONE,
    );
}

pub fn shape(text: &str, analysis: &Analysis, glyphs: &mut GlyphString) {
    shape_full(text, 0, text.len(), analysis, glyphs);
}

pub fn shape_item(
    item: &Item,
    paragraph_text: &str,
    log_attrs: Option<&[LogAttr]>,
    glyphs: &mut GlyphString,
    flags: u32,
) {
    let offset = usize::try_from(item.offset).unwrap_or(0);
    let length = usize::try_from(item.length).unwrap_or(0);
    let (item_offset, item_length) = clamp_item(paragraph_text, offset, length);
    shape_internal(
        &ShapeRequest {
            paragraph: paragraph_text.as_bytes(),
            item_offset,
            item_length,
            analysis: &item.analysis,
            log_attrs,
            num_chars: usize::try_from(item.num_chars).unwrap_or(0),
            flags,
        },
        glyphs,
    );
}

pub fn shape_sample(context: &mut Context, desc: &FontDescription, text: &str) -> Vec<SampleItem> {
    let base_dir = context.base_dir();
    let items = itemize_with_font(
        context,
        base_dir,
        text,
        0,
        text.len(),
        None,
        None,
        Some(desc),
    );
    let mut glyphs = GlyphString::new();
    items
        .iter()
        .map(|item| {
            shape_full(
                text,
                item.offset as usize,
                item.length as usize,
                &item.analysis,
                &mut glyphs,
            );
            SampleItem {
                font: item.analysis.font.clone(),
                width: glyphs.get_width(),
            }
        })
        .collect()
}

pub fn install_layout_hooks(
    measure_font_sample: fn(&mut Context, &FontDescription, &str) -> SampleWidths,
) -> bool {
    set_layout_hooks(LayoutHooks {
        measure_font_sample,
        shape_sample,
        font_map_changed: shape_cache_font_map_changed,
    })
}
