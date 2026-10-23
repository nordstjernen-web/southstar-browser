//! Southstar text — run and line extents: glyph boxes, decorations, CSS line-height boxes, struts and the extents of empty lines.
//! Copyright 2000, 2001, 2006 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::attrs::{AttrType, AttrValue};
use crate::context::Context;
use crate::font::{Font, metrics_of, scale_factors_of};
use crate::glyph_item::GlyphItem;
use crate::item::{Item, analysis_flags};
use crate::metrics::FontMetrics;
use crate::types::{
    GLYPH_EMPTY, Overline, Rectangle, SCALE, Underline, show_flags, units_floor, units_round,
};

use super::{Layout, LayoutLine};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Extents {
    pub baseline: i32,
    pub ink_rect: Rectangle,
    pub logical_rect: Rectangle,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ItemProperties {
    pub uline_single: bool,
    pub uline_double: bool,
    pub uline_low: bool,
    pub uline_error: bool,
    pub strikethrough: bool,
    pub oline_single: bool,
    pub showing_space: bool,
    pub letter_spacing: i32,
    pub word_spacing: i32,
    pub shape_set: bool,
    pub shape_ink_rect: Rectangle,
    pub shape_logical_rect: Rectangle,
    pub line_height: f64,
    pub absolute_line_height: i32,
}

impl ItemProperties {
    pub(crate) fn of(item: &Item) -> ItemProperties {
        let mut properties = ItemProperties::default();
        for attr in &item.analysis.extra_attrs {
            match &attr.value {
                AttrValue::Underline(value) => match Underline::from_i32(*value) {
                    Some(Underline::Single | Underline::SingleLine) => {
                        properties.uline_single = true;
                    }
                    Some(Underline::Double | Underline::DoubleLine) => {
                        properties.uline_double = true;
                    }
                    Some(Underline::Low) => properties.uline_low = true,
                    Some(Underline::Error | Underline::ErrorLine) => {
                        properties.uline_error = true;
                    }
                    Some(Underline::None) | None => {}
                },
                AttrValue::Overline(value) => {
                    if Overline::from_i32(*value) == Some(Overline::Single) {
                        properties.oline_single = true;
                    }
                }
                AttrValue::Strikethrough(value) => properties.strikethrough = *value & 1 != 0,
                AttrValue::LetterSpacing(value) => properties.letter_spacing = *value,
                AttrValue::WordSpacing(value) => properties.word_spacing = *value,
                AttrValue::Shape(shape) => {
                    properties.shape_set = true;
                    properties.shape_logical_rect = shape.logical_rect;
                    properties.shape_ink_rect = shape.ink_rect;
                }
                AttrValue::LineHeight(value) => properties.line_height = *value,
                AttrValue::AbsoluteLineHeight(value) => properties.absolute_line_height = *value,
                AttrValue::Show(value) => {
                    properties.showing_space = (*value as u32 & show_flags::SPACES) != 0;
                }
                _ => {}
            }
        }
        properties
    }
}

pub(crate) fn shape_get_extents(
    n_chars: i32,
    shape_ink: &Rectangle,
    shape_logical: &Rectangle,
) -> (Rectangle, Rectangle) {
    if n_chars <= 0 {
        return (Rectangle::default(), Rectangle::default());
    }
    let ink = Rectangle {
        x: shape_ink
            .x
            .min(shape_ink.x + shape_logical.width * (n_chars - 1)),
        width: shape_ink
            .width
            .max(shape_ink.width + shape_logical.width * (n_chars - 1)),
        y: shape_ink.y,
        height: shape_ink.height,
    };
    let logical = Rectangle {
        x: shape_logical
            .x
            .min(shape_logical.x + shape_logical.width * (n_chars - 1)),
        width: shape_logical
            .width
            .max(shape_logical.width + shape_logical.width * (n_chars - 1)),
        y: shape_logical.y,
        height: shape_logical.height,
    };
    (ink, logical)
}

#[cfg(target_os = "macos")]
pub fn font_get_legacy_ascent(font: Option<&Font>, content_height: i32) -> i32 {
    let Some(font) = font else {
        return 0;
    };
    let desc = font.describe();
    match desc.family() {
        Some(family)
            if family.eq_ignore_ascii_case("Times")
                || family.eq_ignore_ascii_case("Helvetica")
                || family.eq_ignore_ascii_case("Courier") =>
        {
            (f64::from(content_height) * 0.15 / f64::from(SCALE) + 0.5).floor() as i32 * SCALE
        }
        _ => 0,
    }
}

#[cfg(not(target_os = "macos"))]
pub fn font_get_legacy_ascent(_font: Option<&Font>, _content_height: i32) -> i32 {
    0
}

pub(crate) fn round_content_area(rect: &mut Rectangle, baseline_shift: i32, font: Option<&Font>) {
    let mut ascent = units_round(baseline_shift - rect.y);
    let descent = units_round(rect.y + rect.height - baseline_shift);
    ascent += font_get_legacy_ascent(font, ascent + descent);
    rect.y = baseline_shift - ascent;
    rect.height = ascent + descent;
}

pub(crate) fn apply_line_height(rect: &mut Rectangle, line_height: i32) {
    let above = units_floor((line_height - rect.height) / 2);
    rect.y -= above;
    rect.height = line_height;
}

fn font_line_gap(font: Option<&Font>) -> i32 {
    let metrics = metrics_of(font, None);
    let gap = metrics.height - metrics.ascent - metrics.descent;
    units_round(gap).max(0)
}

fn same_font(a: Option<&Font>, b: Option<&Font>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.ptr_eq(b),
        (None, None) => true,
        _ => false,
    }
}

fn run_css_box(
    run: &GlyphItem,
    rect: &mut Rectangle,
    absolute_line_height: i32,
    line_height_factor: f32,
) {
    let analysis = &run.item.analysis;
    let font = analysis.font.as_ref();
    let strut_font = analysis.strut_font();
    let fallback = absolute_line_height != 0
        && strut_font.is_some()
        && !same_font(strut_font, font)
        && analysis.flags & analysis_flags::CENTERED_BASELINE == 0;
    let y_offset = run.y_offset;
    let mut own = *rect;

    if fallback && let Some(strut_font) = strut_font {
        let (_, strut) = strut_font.glyph_extents(GLYPH_EMPTY);
        rect.y = strut.y - y_offset;
        rect.height = strut.height;
    }

    round_content_area(rect, -y_offset, if fallback { strut_font } else { font });

    let line_height = if absolute_line_height != 0 {
        absolute_line_height
    } else {
        (line_height_factor * rect.height as f32).ceil() as i32
    };
    apply_line_height(rect, line_height);

    if fallback && line_height_factor != 0.0 {
        round_content_area(&mut own, -y_offset, font);
        let spacing = ((line_height_factor * own.height as f32).ceil() as i32)
            .max(own.height + font_line_gap(font));
        apply_line_height(&mut own, spacing);
        let top = rect.y.min(own.y);
        let bottom = (rect.y + rect.height).max(own.y + own.height);
        rect.y = top;
        rect.height = bottom - top;
    }
}

pub fn run_css_extents(run: &GlyphItem, line_height: i32, normal: bool) -> (i32, i32) {
    let (_, mut rect) = run.glyphs.extents(run.item.analysis.font.as_ref());
    rect.y -= run.y_offset;
    run_css_box(run, &mut rect, line_height, if normal { 1.0 } else { 0.0 });
    (-rect.y, rect.y + rect.height)
}

pub(crate) struct RunExtents {
    pub ink: Rectangle,
    pub logical: Rectangle,
    pub line_logical: Rectangle,
    pub height: i32,
}

fn larger(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub(crate) fn run_extents_and_height(run: &GlyphItem, want_height: bool) -> RunExtents {
    let properties = ItemProperties::of(&run.item);
    let analysis = &run.item.analysis;
    let font = analysis.font.as_ref();
    let has_underline = properties.uline_single
        || properties.uline_double
        || properties.uline_low
        || properties.uline_error;
    let has_overline = properties.oline_single;
    let mut metrics: Option<FontMetrics> = None;

    let (mut ink, mut logical) =
        if properties.shape_set && analysis.flags & analysis_flags::IS_ELLIPSIS == 0 {
            shape_get_extents(
                run.item.num_chars,
                &properties.shape_ink_rect,
                &properties.shape_logical_rect,
            )
        } else {
            run.glyphs.extents(font)
        };

    if has_underline || has_overline || properties.strikethrough {
        let m = *metrics.get_or_insert_with(|| metrics_of(font, analysis.language));
        let underline_thickness = m.underline_thickness;
        let underline_position = m.underline_position;
        let strikethrough_thickness = m.strikethrough_thickness;
        let strikethrough_position = m.strikethrough_position;

        let new_pos = ink.x.min(logical.x);
        ink.width = (ink.x + ink.width).max(logical.x + logical.width) - new_pos;
        ink.x = new_pos;

        if properties.strikethrough && ink.height == 0 {
            ink.height = strikethrough_thickness;
            ink.y = -strikethrough_position;
        }
        if properties.oline_single {
            ink.y -= underline_thickness;
            ink.height += underline_thickness;
        }
        if properties.uline_low {
            ink.height += 2 * underline_thickness;
        }
        if properties.uline_single {
            ink.height = ink
                .height
                .max(underline_thickness - underline_position - ink.y);
        }
        if properties.uline_double {
            ink.height = ink
                .height
                .max(3 * underline_thickness - underline_position - ink.y);
        }
        if properties.uline_error {
            ink.height = ink
                .height
                .max(3 * underline_thickness - underline_position - ink.y);
        }
    }

    let mut height = 0;
    if want_height {
        if let Some(size_font) = analysis.size_font() {
            let height_metrics = size_font.metrics(analysis.language);
            let (xscale, yscale) = size_font.scale_factors();
            height = (f64::from(height_metrics.height) * larger(xscale, yscale)) as i32;
        } else {
            let m = *metrics.get_or_insert_with(|| metrics_of(font, analysis.language));
            let (xscale, yscale) = scale_factors_of(font);
            height = (f64::from(m.height) * larger(xscale, yscale)) as i32;
        }
    }

    let mut y_offset = run.y_offset;
    if analysis.flags & analysis_flags::CENTERED_BASELINE != 0 {
        let is_hinted = (logical.y & logical.height & (SCALE - 1)) == 0;
        let mut adjustment = logical.y + logical.height / 2;
        if is_hinted {
            adjustment = units_round(adjustment);
        }
        y_offset += adjustment;
    }

    ink.y -= y_offset;
    logical.y -= y_offset;

    let mut line_logical = logical;
    if properties.absolute_line_height != 0 || properties.line_height != 0.0 {
        if properties.shape_set {
            let line_height = if properties.absolute_line_height != 0 {
                properties.absolute_line_height
            } else {
                ((properties.line_height * f64::from(line_logical.height)) as f32).ceil() as i32
            };
            let leading = line_height - line_logical.height;
            line_logical.y -= leading / 2;
            line_logical.height += leading;
        } else {
            run_css_box(
                run,
                &mut line_logical,
                properties.absolute_line_height,
                properties.line_height as f32,
            );
        }
    }

    RunExtents {
        ink,
        logical,
        line_logical,
        height,
    }
}

fn base_font_description(
    layout: &Layout,
    ctx: &Context,
) -> crate::font_description::FontDescription {
    let mut font_desc = ctx.font_description().clone();
    if let Some(desc) = layout.font_desc.as_ref() {
        font_desc.merge(Some(desc), true);
    }
    font_desc
}

pub(crate) fn empty_extents_and_height_at_index(
    layout: &Layout,
    ctx: &Context,
    index: i32,
    apply_line_heights: bool,
) -> (Rectangle, i32) {
    let mut font_desc = base_font_description(layout, ctx);
    let mut line_height_factor = 0.0f64;
    let mut absolute_line_height = 0;
    let mut height = 0;

    if let Some(attrs) = layout.attrs.as_ref() {
        let mut iter = attrs.iterator();
        loop {
            let (start, end) = iter.range();
            if start <= index && index < end {
                iter.get_font(&mut font_desc, None, None);
                if let Some(attr) = iter.get(AttrType::LineHeight) {
                    line_height_factor = attr.value.as_float().unwrap_or(0.0);
                }
                if let Some(attr) = iter.get(AttrType::AbsoluteLineHeight) {
                    absolute_line_height = attr.value.as_int().unwrap_or(0);
                }
                break;
            }
            if !iter.next() {
                break;
            }
        }
    }

    let mut logical = Rectangle::default();
    if let Some(font) = ctx.load_font(&font_desc) {
        let metrics = font.metrics(ctx.language());
        logical.y = -metrics.ascent;
        logical.height = -logical.y + metrics.descent;
        height = metrics.height;

        if apply_line_heights && (absolute_line_height != 0 || line_height_factor != 0.0) {
            round_content_area(&mut logical, 0, Some(&font));
            let scaled = ((line_height_factor * f64::from(logical.height)) as f32).ceil();
            let absolute = absolute_line_height as f32;
            let line_height = if absolute > scaled { absolute } else { scaled } as i32;
            apply_line_height(&mut logical, line_height);
        }
    }
    logical.x = 0;
    logical.width = 0;
    (logical, height)
}

fn strut(layout: &Layout, ctx: &Context) -> Option<Rectangle> {
    if layout.strut_line_height <= 0 {
        return None;
    }
    let rect = match layout.strut_rect.get() {
        Some(rect) => rect,
        None => {
            let mut rect = Rectangle::default();
            let font_desc = base_font_description(layout, ctx);
            if let Some(font) = ctx.load_font(&font_desc) {
                let metrics = font.metrics(ctx.language());
                rect.y = -metrics.ascent;
                rect.height = metrics.ascent + metrics.descent;
                round_content_area(&mut rect, 0, Some(&font));
                apply_line_height(&mut rect, layout.strut_line_height);
            }
            layout.strut_rect.set(Some(rect));
            rect
        }
    };
    (rect.height > 0).then_some(rect)
}

fn unite_strut(layout: &Layout, ctx: &Context, logical: &mut Rectangle) {
    let Some(strut) = strut(layout, ctx) else {
        return;
    };
    let top = logical.y.min(strut.y);
    let bottom = (logical.y + logical.height).max(strut.y + strut.height);
    logical.y = top;
    logical.height = bottom - top;
}

pub(crate) fn line_extents_and_height(
    layout: &Layout,
    ctx: &Context,
    line: &LayoutLine,
) -> (Rectangle, Rectangle, i32) {
    if let Some(cached) = line.extents_cache.get() {
        return cached;
    }

    let mut ink = Rectangle::default();
    let mut logical = Rectangle::default();
    let mut height = 0;
    let mut x_pos = 0;

    for (i, run) in line.runs.iter().enumerate() {
        let extents = run_extents_and_height(run, true);
        let run_ink = extents.ink;
        let run_logical = extents.line_logical;

        if ink.width == 0 || ink.height == 0 {
            ink = run_ink;
            ink.x += x_pos;
        } else if run_ink.width != 0 && run_ink.height != 0 {
            let new_pos = ink.x.min(x_pos + run_ink.x);
            ink.width = (ink.x + ink.width).max(x_pos + run_ink.x + run_ink.width) - new_pos;
            ink.x = new_pos;

            let new_pos = ink.y.min(run_ink.y);
            ink.height = (ink.y + ink.height).max(run_ink.y + run_ink.height) - new_pos;
            ink.y = new_pos;
        }

        let new_pos = logical.x.min(x_pos + run_logical.x);
        logical.width =
            (logical.x + logical.width).max(x_pos + run_logical.x + run_logical.width) - new_pos;
        logical.x = new_pos;

        if i == 0 {
            logical.y = run_logical.y;
            logical.height = run_logical.height;
        } else {
            let new_pos = logical.y.min(run_logical.y);
            logical.height =
                (logical.y + logical.height).max(run_logical.y + run_logical.height) - new_pos;
            logical.y = new_pos;
        }

        height = height.max(extents.height.wrapping_abs());
        x_pos += run_logical.width;
    }

    if line.runs.is_empty() {
        let (empty, empty_height) =
            empty_extents_and_height_at_index(layout, ctx, line.start_index, true);
        logical = empty;
        height = empty_height;
    }

    unite_strut(layout, ctx, &mut logical);

    let result = (ink, logical, height);
    line.extents_cache.set(Some(result));
    result
}
