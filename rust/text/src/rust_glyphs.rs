//! Southstar text — outlines, extents and font extents of Rust-backed fonts, read through skrifa the way cairo-ft and FreeType load them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::sync::{Arc, OnceLock};

use harfrust::font::Blob;
use read_fonts::TableProvider;
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{
    DrawSettings, Engine, HintingInstance, HintingOptions, OutlineGlyphFormat, OutlinePen,
    SmoothMode, Target,
};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::ffi::cairo::{
    CairoContext, CairoMatrix, FontExtents, FontOptions, ScaledFont, TextExtents, UserFontHandler,
};

const EMBOLDEN_DELTA: i64 = 0x0AAA;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HintMode {
    None,
    Light,
    Normal,
    Mono,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct GlyphSettings {
    pub ppem: f32,
    pub hinting: HintMode,
    pub force_autohint: bool,
    pub embolden: bool,
    pub hint_metrics: bool,
}

impl GlyphSettings {
    fn hinting_options(&self, format: Option<OutlineGlyphFormat>) -> Option<HintingOptions> {
        let target = match self.hinting {
            HintMode::None => return None,
            HintMode::Light => Target::from(SmoothMode::Light),
            HintMode::Normal => Target::from(SmoothMode::Normal),
            HintMode::Mono => Target::Mono,
        };
        let light_truetype =
            self.hinting == HintMode::Light && format == Some(OutlineGlyphFormat::Glyf);
        let engine = if self.force_autohint || light_truetype {
            Engine::Auto(None)
        } else {
            Engine::AutoFallback
        };
        Some(HintingOptions { engine, target })
    }

    fn integer_ppem(&self) -> i64 {
        (f64::from(self.ppem) * 64. + 32.) as i64 >> 6
    }

    fn embolden_strength(&self) -> f64 {
        let strength = self.integer_ppem() * EMBOLDEN_DELTA / 1024;
        strength as f64 / 64.
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointKind {
    On,
    Quad,
    Cubic,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Outline {
    points: Vec<(f64, f64, PointKind)>,
    ends: Vec<usize>,
}

impl Outline {
    fn end_contour(&mut self) {
        let start = self.ends.last().copied().unwrap_or(0);
        if self.points.len() > start {
            self.ends.push(self.points.len());
        }
    }

    fn push(&mut self, x: f32, y: f32, kind: PointKind) {
        self.points.push((f64::from(x), f64::from(y), kind));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    fn contours(&self) -> impl Iterator<Item = core::ops::Range<usize>> + '_ {
        let mut start = 0;
        self.ends.iter().map(move |&end| {
            let range = start..end;
            start = end;
            range
        })
    }

    fn control_box(&self) -> Option<(f64, f64, f64, f64)> {
        let mut points = self.points.iter();
        let &(x, y, _) = points.next()?;
        Some(points.fold((x, y, x, y), |(x0, y0, x1, y1), &(x, y, _)| {
            (x0.min(x), y0.min(y), x1.max(x), y1.max(y))
        }))
    }

    fn is_truetype_oriented(&self) -> Option<bool> {
        let (x0, y0, x1, y1) = self.control_box()?;
        if x0 == x1 || y0 == y1 {
            return None;
        }
        let mut area = 0.;
        for range in self.contours() {
            let contour = &self.points[range];
            let Some(&(mut prev_x, mut prev_y, _)) = contour.last() else {
                continue;
            };
            for &(x, y, _) in contour {
                area += (y - prev_y) * (x + prev_x);
                (prev_x, prev_y) = (x, y);
            }
        }
        if area > 0. {
            Some(false)
        } else if area < 0. {
            Some(true)
        } else {
            None
        }
    }

    fn embolden(&mut self, x_strength: f64, y_strength: f64) {
        let (x_strength, y_strength) = (x_strength / 2., y_strength / 2.);
        if x_strength == 0. && y_strength == 0. {
            return;
        }
        let Some(truetype) = self.is_truetype_oriented() else {
            return;
        };
        let ranges: Vec<_> = self.contours().collect();
        for range in ranges {
            embolden_contour(&mut self.points[range], truetype, x_strength, y_strength);
        }
    }

    pub(crate) fn draw(&self, cr: &CairoContext, scale: f64) {
        let at = |&(x, y, _): &(f64, f64, PointKind)| (x * scale, -y * scale);
        for range in self.contours() {
            let contour = &self.points[range];
            let Some(first) = contour.first() else {
                continue;
            };
            let (start_x, start_y) = at(first);
            cr.move_to(start_x, start_y);
            let (mut last_x, mut last_y) = (start_x, start_y);
            let mut index = 1;
            while index < contour.len() {
                let point = &contour[index];
                match point.2 {
                    PointKind::On => {
                        (last_x, last_y) = at(point);
                        cr.line_to(last_x, last_y);
                        index += 1;
                    }
                    PointKind::Quad => {
                        let (cx, cy) = at(point);
                        let (x, y) = contour.get(index + 1).map_or((start_x, start_y), at);
                        cr.curve_to(
                            last_x + 2. / 3. * (cx - last_x),
                            last_y + 2. / 3. * (cy - last_y),
                            x + 2. / 3. * (cx - x),
                            y + 2. / 3. * (cy - y),
                            x,
                            y,
                        );
                        (last_x, last_y) = (x, y);
                        index += 2;
                    }
                    PointKind::Cubic => {
                        let (c1x, c1y) = at(point);
                        let (c2x, c2y) = contour.get(index + 1).map_or((start_x, start_y), at);
                        let (x, y) = contour.get(index + 2).map_or((start_x, start_y), at);
                        cr.curve_to(c1x, c1y, c2x, c2y, x, y);
                        (last_x, last_y) = (x, y);
                        index += 3;
                    }
                }
            }
            cr.close_path();
        }
    }
}

fn unit_vector(x: f64, y: f64) -> Option<(f64, f64, f64)> {
    let length = x.hypot(y);
    (length > 0.).then(|| (x / length, y / length, length))
}

fn embolden_contour(
    points: &mut [(f64, f64, PointKind)],
    truetype: bool,
    x_strength: f64,
    y_strength: f64,
) {
    let count = points.len();
    if count == 0 {
        return;
    }
    let last = count - 1;
    let next = |j: usize| if j < last { j + 1 } else { 0 };
    let (mut in_x, mut in_y, mut in_length) = (0., 0., 0.);
    let (mut anchor_x, mut anchor_y, mut anchor_length) = (0., 0., 0.);
    let mut i = last;
    let mut j = 0;
    let mut k: Option<usize> = None;
    while j != i && Some(i) != k {
        let (out_x, out_y, out_length);
        if Some(j) != k {
            match unit_vector(points[j].0 - points[i].0, points[j].1 - points[i].1) {
                Some(found) => (out_x, out_y, out_length) = found,
                None => {
                    j = next(j);
                    continue;
                }
            }
        } else {
            (out_x, out_y, out_length) = (anchor_x, anchor_y, anchor_length);
        }
        if in_length != 0. {
            if k.is_none() {
                k = Some(i);
                (anchor_x, anchor_y, anchor_length) = (in_x, in_y, in_length);
            }
            let mut d = in_x * out_x + in_y * out_y;
            let (mut shift_x, mut shift_y) = (0., 0.);
            if d > -(0xF000 as f64) / 65536. {
                d += 1.;
                shift_x = in_y + out_y;
                shift_y = in_x + out_x;
                if truetype {
                    shift_x = -shift_x;
                } else {
                    shift_y = -shift_y;
                }
                let mut q = out_x * in_y - out_y * in_x;
                if truetype {
                    q = -q;
                }
                let l = in_length.min(out_length);
                shift_x = if x_strength * q <= l * d {
                    shift_x * x_strength / d
                } else {
                    shift_x * l / q
                };
                shift_y = if y_strength * q <= l * d {
                    shift_y * y_strength / d
                } else {
                    shift_y * l / q
                };
            }
            while i != j {
                points[i].0 += x_strength + shift_x;
                points[i].1 += y_strength + shift_y;
                i = next(i);
            }
        } else {
            i = j;
        }
        (in_x, in_y, in_length) = (out_x, out_y, out_length);
        j = next(j);
    }
}

impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.end_contour();
        self.push(x, y, PointKind::On);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.push(x, y, PointKind::On);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.push(cx0, cy0, PointKind::Quad);
        self.push(x, y, PointKind::On);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.push(cx0, cy0, PointKind::Cubic);
        self.push(cx1, cy1, PointKind::Cubic);
        self.push(x, y, PointKind::On);
    }

    fn close(&mut self) {
        self.end_contour();
    }
}

pub(crate) struct GlyphSource {
    blob: Blob,
    index: u32,
    coords: Vec<NormalizedCoord>,
    settings: GlyphSettings,
    hinter: OnceLock<Option<HintingInstance>>,
}

impl GlyphSource {
    pub(crate) fn new(
        blob: Blob,
        index: u32,
        coords: Vec<NormalizedCoord>,
        settings: GlyphSettings,
    ) -> Option<GlyphSource> {
        FontRef::from_index(blob.as_ref(), index).ok()?;
        Some(GlyphSource {
            blob,
            index,
            coords,
            settings,
            hinter: OnceLock::new(),
        })
    }

    fn font(&self) -> Option<FontRef<'_>> {
        FontRef::from_index(self.blob.as_ref(), self.index).ok()
    }

    fn location(&self) -> LocationRef<'_> {
        LocationRef::new(&self.coords)
    }

    fn size(&self) -> Size {
        Size::new(self.settings.ppem)
    }

    fn hinter(&self, font: &FontRef<'_>) -> Option<&HintingInstance> {
        self.hinter
            .get_or_init(|| {
                let outlines = font.outline_glyphs();
                let options = self.settings.hinting_options(outlines.format())?;
                HintingInstance::new(&outlines, self.size(), self.location(), options).ok()
            })
            .as_ref()
    }

    fn draw(
        &self,
        font: &FontRef<'_>,
        glyph: u32,
        size: Size,
        hinter: Option<&HintingInstance>,
    ) -> Option<(Outline, Option<f32>)> {
        let outlines = font.outline_glyphs();
        let mut outline = Outline::default();
        let Some(source) = outlines.get(GlyphId::new(glyph)) else {
            return Some((outline, None));
        };
        let metrics = match hinter {
            Some(hinter) => source.draw(DrawSettings::hinted(hinter, false), &mut outline),
            None => source.draw(DrawSettings::unhinted(size, self.location()), &mut outline),
        }
        .ok()?;
        outline.end_contour();
        Some((outline, metrics.advance_width))
    }

    fn advance(&self, font: &FontRef<'_>, glyph: u32, size: Size) -> f64 {
        font.glyph_metrics(size, self.location())
            .advance_width(GlyphId::new(glyph))
            .map_or(0., f64::from)
    }

    fn em_extents_unscaled(&self, font: &FontRef<'_>, glyph: u32) -> Option<TextExtents> {
        let upem = f64::from(font.head().ok()?.units_per_em().max(1));
        let (outline, _) = self.draw(font, glyph, Size::unscaled(), None)?;
        let (mut x0, mut y0, mut x1, mut y1) = outline.control_box().unwrap_or_default();
        if self.settings.hinting != HintMode::None {
            (x0, y0, x1, y1) = (x0.floor(), y0.floor(), x1.ceil(), y1.ceil());
        }
        if self.settings.embolden && !outline.is_empty() {
            let strength = (upem as i64 * EMBOLDEN_DELTA / 1024) as f64 / 64.;
            x1 += strength;
            y1 += strength;
        }
        Some(scale_extents(
            TextExtents {
                x_bearing: x0,
                y_bearing: -y1,
                width: x1 - x0,
                height: y1 - y0,
                x_advance: self.advance(font, glyph, Size::unscaled()),
                y_advance: 0.,
            },
            1. / upem,
        ))
    }

    fn em_extents_hinted(&self, font: &FontRef<'_>, glyph: u32) -> Option<TextExtents> {
        let (outline, hinted_advance) = self.draw(font, glyph, self.size(), self.hinter(font))?;
        let (mut x0, mut y0, mut x1, mut y1) = outline.control_box().unwrap_or_default();
        if self.settings.hinting != HintMode::None {
            (x0, y0, x1, y1) = (x0.floor(), y0.floor(), x1.ceil(), y1.ceil());
        }
        let mut advance = match hinted_advance {
            Some(hinted) => f64::from(hinted),
            None => self.advance(font, glyph, self.size()).round(),
        };
        if self.settings.embolden {
            let strength = self.settings.embolden_strength();
            if !outline.is_empty() {
                x1 += strength;
                y1 += strength;
            }
            advance += strength;
        }
        Some(scale_extents(
            TextExtents {
                x_bearing: x0,
                y_bearing: -y1,
                width: x1 - x0,
                height: y1 - y0,
                x_advance: advance,
                y_advance: 0.,
            },
            em_scale(self),
        ))
    }

    pub(crate) fn em_extents(&self, glyph: u32) -> TextExtents {
        let Some(font) = self.font() else {
            return TextExtents::default();
        };
        let extents = if self.settings.hint_metrics {
            self.em_extents_hinted(&font, glyph)
        } else {
            self.em_extents_unscaled(&font, glyph)
        };
        extents.unwrap_or_default()
    }

    pub(crate) fn render_outline(&self, glyph: u32) -> Option<(Outline, TextExtents)> {
        let font = self.font()?;
        let (mut outline, _) = self.draw(&font, glyph, self.size(), self.hinter(&font))?;
        if self.settings.embolden {
            let strength = self.settings.embolden_strength();
            outline.embolden(strength, strength);
        }
        let layout = self.em_extents(glyph);
        let scale = em_scale(self);
        let mut extents = scale_extents(layout, 1. / scale.max(f64::MIN_POSITIVE));
        if let Some((x0, y0, x1, y1)) = outline.control_box() {
            let (left, top) = (extents.x_bearing.min(x0), extents.y_bearing.min(-y1));
            let right = (extents.x_bearing + extents.width).max(x1);
            let bottom = (extents.y_bearing + extents.height).max(-y0);
            extents.x_bearing = left;
            extents.y_bearing = top;
            extents.width = right - left;
            extents.height = bottom - top;
        }
        Some((outline, extents))
    }

    pub(crate) fn font_extents(&self) -> FontExtents {
        let Some(font) = self.font() else {
            return FontExtents::default();
        };
        let upem = f64::from(font.head().map_or(1000, |head| head.units_per_em()).max(1));
        let scale = f64::from(self.settings.ppem) / upem;
        let Ok(hhea) = font.hhea() else {
            return FontExtents::default();
        };
        let ascent = f64::from(hhea.ascender().to_i16());
        let descent = -f64::from(hhea.descender().to_i16());
        let height = ascent + descent + f64::from(hhea.line_gap().to_i16());
        FontExtents {
            ascent: ascent * scale,
            descent: descent * scale,
            height: height * scale,
            max_x_advance: f64::from(hhea.advance_width_max().to_u16()) * scale,
            max_y_advance: 0.,
        }
    }
}

fn scale_extents(extents: TextExtents, scale: f64) -> TextExtents {
    TextExtents {
        x_bearing: extents.x_bearing * scale,
        y_bearing: extents.y_bearing * scale,
        width: extents.width * scale,
        height: extents.height * scale,
        x_advance: extents.x_advance * scale,
        y_advance: extents.y_advance * scale,
    }
}

fn em_scale(source: &GlyphSource) -> f64 {
    let ppem = f64::from(source.settings.ppem);
    if ppem > 0. { 1. / ppem } else { 0. }
}

pub(crate) fn user_extents(source: &GlyphSource, glyph: u32, matrix: &CairoMatrix) -> TextExtents {
    let em = source.em_extents(glyph);
    let transform = |x: f64, y: f64| (matrix.xx * x + matrix.xy * y, matrix.yx * x + matrix.yy * y);
    let (advance_x, advance_y) = transform(em.x_advance, em.y_advance);
    if em.width == 0. && em.height == 0. {
        return TextExtents {
            x_advance: advance_x,
            y_advance: advance_y,
            ..TextExtents::default()
        };
    }
    let corners = [
        transform(em.x_bearing, em.y_bearing),
        transform(em.x_bearing + em.width, em.y_bearing),
        transform(em.x_bearing, em.y_bearing + em.height),
        transform(em.x_bearing + em.width, em.y_bearing + em.height),
    ];
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (x, y) in corners {
        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
    }
    TextExtents {
        x_bearing: x0,
        y_bearing: y0,
        width: x1 - x0,
        height: y1 - y0,
        x_advance: advance_x,
        y_advance: advance_y,
    }
}

struct OutlineRenderer {
    source: Arc<GlyphSource>,
}

impl UserFontHandler for OutlineRenderer {
    fn render_glyph(&self, glyph: u64, cr: &CairoContext, extents: &mut TextExtents) -> bool {
        let Ok(glyph) = u32::try_from(glyph) else {
            return false;
        };
        let Some((outline, glyph_extents)) = self.source.render_outline(glyph) else {
            return false;
        };
        let scale = em_scale(&self.source);
        *extents = scale_extents(glyph_extents, scale);
        if !outline.is_empty() {
            outline.draw(cr, scale);
            cr.fill();
        }
        true
    }

    fn unicode_to_glyph(&self, unicode: u64) -> Option<u64> {
        let font = self.source.font()?;
        let ch = char::from_u32(u32::try_from(unicode).ok()?)?;
        font.charmap()
            .map(ch)
            .map(|glyph| u64::from(glyph.to_u32()))
    }

    fn font_extents(&self) -> Option<FontExtents> {
        let extents = self.source.font_extents();
        let scale = em_scale(&self.source);
        Some(FontExtents {
            ascent: extents.ascent * scale,
            descent: extents.descent * scale,
            height: extents.height * scale,
            max_x_advance: extents.max_x_advance * scale,
            max_y_advance: extents.max_y_advance * scale,
        })
    }
}

pub(crate) fn scaled_font(
    source: &Arc<GlyphSource>,
    font_matrix: &CairoMatrix,
    ctm: &CairoMatrix,
    options: &FontOptions,
) -> Option<ScaledFont> {
    let handler = Box::new(OutlineRenderer {
        source: source.clone(),
    });
    ScaledFont::user_font(handler, font_matrix, ctm, options)
}
