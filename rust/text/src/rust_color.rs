//! Southstar text — color glyphs of Rust-backed fonts: COLR v0 and v1 painted through cairo, and CBDT and sbix PNG strikes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use skrifa::bitmap::{BitmapData, BitmapFormat, BitmapGlyph, BitmapStrikes, Origin};
use skrifa::color::{Brush, Color, ColorPainter, ColorStop, CompositeMode, Extend, Transform};
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::DrawSettings;
use skrifa::raw::TableProvider;
use skrifa::raw::types::{BoundingBox, Point};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use std::cell::OnceCell;

use crate::ffi::cairo::{CairoContext, CairoMatrix, CairoPattern, ColorForeground, ImageSurface};
use crate::rust_glyphs::Outline;

const FOREGROUND_INDEX: u16 = 0xFFFF;
const MAX_BITMAP_SIDE: u32 = 4096;
const SWEEP_SLICE_DEGREES: f64 = 2.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Rgba {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub alpha: f64,
}

impl Rgba {
    fn from_color(color: Color) -> Rgba {
        Rgba {
            red: f64::from(color.red) / 255.,
            green: f64::from(color.green) / 255.,
            blue: f64::from(color.blue) / 255.,
            alpha: f64::from(color.alpha) / 255.,
        }
    }

    fn with_alpha(self, alpha: f32) -> Rgba {
        Rgba {
            alpha: self.alpha * f64::from(alpha),
            ..self
        }
    }

    fn tuple(self) -> (f64, f64, f64, f64) {
        (self.red, self.green, self.blue, self.alpha)
    }

    fn mix(self, other: Rgba, t: f64) -> Rgba {
        let lerp = |a: f64, b: f64| a + (b - a) * t;
        Rgba {
            red: lerp(self.red, other.red),
            green: lerp(self.green, other.green),
            blue: lerp(self.blue, other.blue),
            alpha: lerp(self.alpha, other.alpha),
        }
    }
}

pub(crate) fn has_color_glyph(font: &FontRef<'_>, glyph: u32) -> bool {
    let gid = GlyphId::new(glyph);
    font.color_glyphs().get(gid).is_some() || color_bitmap(font, gid, Size::unscaled()).is_some()
}

fn color_bitmap<'a>(font: &FontRef<'a>, gid: GlyphId, size: Size) -> Option<BitmapGlyph<'a>> {
    [BitmapFormat::Sbix, BitmapFormat::Cbdt]
        .into_iter()
        .filter_map(|format| BitmapStrikes::with_format(font, format))
        .find_map(|strikes| strikes.glyph_for_size(size, gid))
        .filter(|glyph| matches!(glyph.data, BitmapData::Png(_) | BitmapData::Bgra(_)))
}

pub(crate) fn paint(
    cr: &CairoContext,
    font: &FontRef<'_>,
    location: LocationRef<'_>,
    glyph: u32,
    ppem: f32,
    foreground: &ColorForeground,
) -> bool {
    let gid = GlyphId::new(glyph);
    let upem = f64::from(font.head().map_or(1000, |head| head.units_per_em()).max(1));
    if let Some(color_glyph) = font.color_glyphs().get(gid) {
        let palette: Vec<Rgba> = font
            .color_palettes()
            .get(0)
            .map(|palette| {
                palette
                    .colors()
                    .iter()
                    .copied()
                    .map(Rgba::from_color)
                    .collect()
            })
            .unwrap_or_default();
        cr.save();
        cr.transform(&CairoMatrix {
            xx: 1. / upem,
            yx: 0.,
            xy: 0.,
            yy: -1. / upem,
            x0: 0.,
            y0: 0.,
        });
        let mut painter = CairoPainter {
            cr,
            font,
            location,
            palette,
            foreground,
            foreground_color: OnceCell::new(),
            frames: Vec::new(),
        };
        let _ = color_glyph.paint(location, &mut painter);
        painter.unwind();
        cr.restore();
        return true;
    }
    let Some(bitmap) = color_bitmap(font, gid, Size::new(ppem)) else {
        return false;
    };
    paint_bitmap(cr, &bitmap, upem)
}

fn paint_bitmap(cr: &CairoContext, bitmap: &BitmapGlyph<'_>, upem: f64) -> bool {
    let Some((width, height, pixels)) = decode_bitmap(bitmap) else {
        return false;
    };
    let Some(surface) = ImageSurface::from_argb(width, height, &pixels) else {
        return false;
    };
    let (ppem_x, ppem_y) = (f64::from(bitmap.ppem_x), f64::from(bitmap.ppem_y));
    if ppem_x <= 0. || ppem_y <= 0. {
        return false;
    }
    let (left, top) = match bitmap.placement_origin {
        Origin::TopLeft => (
            f64::from(bitmap.bearing_x) / upem + f64::from(bitmap.inner_bearing_x) / ppem_x,
            f64::from(bitmap.bearing_y) / upem + f64::from(bitmap.inner_bearing_y) / ppem_y,
        ),
        Origin::BottomLeft => {
            let bottom =
                f64::from(bitmap.bearing_y) / upem + f64::from(bitmap.inner_bearing_y) / ppem_y;
            (
                f64::from(bitmap.bearing_x) / upem + f64::from(bitmap.inner_bearing_x) / ppem_x,
                bottom + f64::from(height) / ppem_y,
            )
        }
    };
    cr.save();
    cr.transform(&CairoMatrix {
        xx: 1. / ppem_x,
        yx: 0.,
        xy: 0.,
        yy: 1. / ppem_y,
        x0: left,
        y0: -top,
    });
    cr.set_source_surface(&surface, 0., 0.);
    cr.paint();
    cr.restore();
    true
}

fn premultiply(red: u8, green: u8, blue: u8, alpha: u8) -> u32 {
    let scale = |channel: u8| (u32::from(channel) * u32::from(alpha) + 127) / 255;
    (u32::from(alpha) << 24) | (scale(red) << 16) | (scale(green) << 8) | scale(blue)
}

fn decode_bitmap(bitmap: &BitmapGlyph<'_>) -> Option<(u32, u32, Vec<u32>)> {
    match bitmap.data {
        BitmapData::Png(bytes) => decode_png(bytes),
        BitmapData::Bgra(bytes) => {
            let (width, height) = (bitmap.width, bitmap.height);
            if width == 0 || height == 0 || width > MAX_BITMAP_SIDE || height > MAX_BITMAP_SIDE {
                return None;
            }
            let count = (width * height) as usize;
            let pixels: Vec<u32> = bytes
                .chunks_exact(4)
                .take(count)
                .map(|bgra| u32::from_le_bytes([bgra[0], bgra[1], bgra[2], bgra[3]]))
                .collect();
            (pixels.len() == count).then_some((width, height, pixels))
        }
        BitmapData::Mask(_) => None,
    }
}

fn decode_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u32>)> {
    let mut decoder = png::Decoder::new_with_limits(
        std::io::Cursor::new(bytes),
        png::Limits {
            bytes: (MAX_BITMAP_SIDE * MAX_BITMAP_SIDE * 4) as usize,
        },
    );
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buffer = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buffer).ok()?;
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 || width > MAX_BITMAP_SIDE || height > MAX_BITMAP_SIDE {
        return None;
    }
    let channels = info.color_type.samples();
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for row in buffer.chunks(info.line_size).take(height as usize) {
        for pixel in row.chunks_exact(channels).take(width as usize) {
            let argb = match pixel {
                [gray] => premultiply(*gray, *gray, *gray, 255),
                [gray, alpha] => premultiply(*gray, *gray, *gray, *alpha),
                [red, green, blue] => premultiply(*red, *green, *blue, 255),
                [red, green, blue, alpha] => premultiply(*red, *green, *blue, *alpha),
                _ => return None,
            };
            pixels.push(argb);
        }
    }
    (pixels.len() == (width * height) as usize).then_some((width, height, pixels))
}

struct CairoPainter<'a, 'f> {
    cr: &'a CairoContext,
    font: &'a FontRef<'f>,
    location: LocationRef<'a>,
    palette: Vec<Rgba>,
    foreground: &'a ColorForeground,
    foreground_color: OnceCell<Rgba>,
    frames: Vec<Frame>,
}

enum Frame {
    Saved,
    Layer(CompositeMode),
}

fn cairo_matrix(transform: Transform) -> CairoMatrix {
    CairoMatrix {
        xx: f64::from(transform.xx),
        yx: f64::from(transform.yx),
        xy: f64::from(transform.xy),
        yy: f64::from(transform.yy),
        x0: f64::from(transform.dx),
        y0: f64::from(transform.dy),
    }
}

fn cairo_operator(mode: CompositeMode) -> i32 {
    match mode {
        CompositeMode::Clear => 0,
        CompositeMode::Src => 1,
        CompositeMode::Dest => 6,
        CompositeMode::DestOver => 7,
        CompositeMode::SrcIn => 3,
        CompositeMode::DestIn => 8,
        CompositeMode::SrcOut => 4,
        CompositeMode::DestOut => 9,
        CompositeMode::SrcAtop => 5,
        CompositeMode::DestAtop => 10,
        CompositeMode::Xor => 11,
        CompositeMode::Plus => 12,
        CompositeMode::Screen => 15,
        CompositeMode::Overlay => 16,
        CompositeMode::Darken => 17,
        CompositeMode::Lighten => 18,
        CompositeMode::ColorDodge => 19,
        CompositeMode::ColorBurn => 20,
        CompositeMode::HardLight => 21,
        CompositeMode::SoftLight => 22,
        CompositeMode::Difference => 23,
        CompositeMode::Exclusion => 24,
        CompositeMode::Multiply => 14,
        CompositeMode::HslHue => 25,
        CompositeMode::HslSaturation => 26,
        CompositeMode::HslColor => 27,
        CompositeMode::HslLuminosity => 28,
        _ => 2,
    }
}

fn cairo_extend(extend: Extend) -> i32 {
    match extend {
        Extend::Repeat => 1,
        Extend::Reflect => 2,
        _ => 3,
    }
}

fn point(p: Point<f32>) -> (f64, f64) {
    (f64::from(p.x), f64::from(p.y))
}

impl CairoPainter<'_, '_> {
    fn foreground_color(&self) -> Rgba {
        *self.foreground_color.get_or_init(|| {
            let (red, green, blue, alpha) = self.foreground.rgba().unwrap_or((0., 0., 0., 1.));
            Rgba {
                red,
                green,
                blue,
                alpha,
            }
        })
    }

    fn color(&self, palette_index: u16, alpha: f32) -> Rgba {
        let base = match self.palette.get(usize::from(palette_index)) {
            Some(color) if palette_index != FOREGROUND_INDEX => *color,
            _ => self.foreground_color(),
        };
        base.with_alpha(alpha)
    }

    fn stops(&self, stops: &[ColorStop]) -> Vec<(f64, Rgba)> {
        stops
            .iter()
            .map(|stop| {
                (
                    f64::from(stop.offset),
                    self.color(stop.palette_index, stop.alpha),
                )
            })
            .collect()
    }

    fn fill_solid(&self, palette_index: u16, alpha: f32) {
        let foreground =
            palette_index == FOREGROUND_INDEX || usize::from(palette_index) >= self.palette.len();
        if foreground && let Some(marker) = self.foreground.marker() {
            self.cr.set_source(&marker);
            self.cr.paint_with_alpha(f64::from(alpha));
            return;
        }
        let color = self.color(palette_index, alpha);
        self.cr
            .set_source_rgba(color.red, color.green, color.blue, color.alpha);
        self.cr.paint();
    }

    fn fill_pattern(&self, pattern: Option<CairoPattern>) {
        if let Some(pattern) = pattern {
            self.cr.set_source(&pattern);
            self.cr.paint();
        }
    }
}

fn add_stops(pattern: &CairoPattern, stops: &[(f64, Rgba)]) {
    for &(offset, color) in stops {
        pattern.add_color_stop(offset, color.red, color.green, color.blue, color.alpha);
    }
}

fn color_at(stops: &[(f64, Rgba)], t: f64) -> Rgba {
    let Some(&(first_offset, first)) = stops.first() else {
        return Rgba {
            red: 0.,
            green: 0.,
            blue: 0.,
            alpha: 0.,
        };
    };
    if t <= first_offset {
        return first;
    }
    for pair in stops.windows(2) {
        let ((t0, c0), (t1, c1)) = (pair[0], pair[1]);
        if t <= t1 {
            return if t1 > t0 {
                c0.mix(c1, (t - t0) / (t1 - t0))
            } else {
                c1
            };
        }
    }
    stops.last().map_or(first, |&(_, color)| color)
}

fn extend_position(t: f64, extend: Extend) -> f64 {
    match extend {
        Extend::Repeat => t - t.floor(),
        Extend::Reflect => {
            let period = t.rem_euclid(2.);
            if period > 1. { 2. - period } else { period }
        }
        _ => t.clamp(0., 1.),
    }
}

struct Radial {
    c0: (f64, f64),
    r0: f64,
    c1: (f64, f64),
    r1: f64,
    stops: Vec<(f64, Rgba)>,
}

fn radial_without_negative_radius(
    (mut c0, mut r0): ((f64, f64), f64),
    (mut c1, mut r1): ((f64, f64), f64),
    stops: &[(f64, Rgba)],
) -> Option<Radial> {
    if r0 >= 0. && r1 >= 0. {
        return Some(Radial {
            c0,
            r0,
            c1,
            r1,
            stops: stops.to_vec(),
        });
    }
    if r0 < 0. && r1 < 0. || r1 == r0 {
        return None;
    }
    let t = -r0 / (r1 - r0);
    let at = (c0.0 + (c1.0 - c0.0) * t, c0.1 + (c1.1 - c0.1) * t);
    let pivot = color_at(stops, t);
    let mut remapped = Vec::new();
    if r0 < 0. {
        let span = 1. - t;
        remapped.push((0., pivot));
        remapped.extend(
            stops
                .iter()
                .filter(|(offset, _)| *offset > t)
                .map(|&(offset, color)| ((offset - t) / span, color)),
        );
        (c0, r0) = (at, 0.);
    } else {
        remapped.extend(
            stops
                .iter()
                .filter(|(offset, _)| *offset < t)
                .map(|&(offset, color)| (offset / t, color)),
        );
        remapped.push((1., pivot));
        (c1, r1) = (at, 0.);
    }
    Some(Radial {
        c0,
        r0,
        c1,
        r1,
        stops: remapped,
    })
}

fn sweep_pattern(
    center: (f64, f64),
    start_angle: f64,
    end_angle: f64,
    stops: &[(f64, Rgba)],
    extend: Extend,
    radius: f64,
) -> Option<CairoPattern> {
    let pattern = CairoPattern::mesh()?;
    let span = end_angle - start_angle;
    let color_for = |degrees: f64| {
        if span == 0. {
            return if degrees < start_angle {
                color_at(stops, 0.)
            } else {
                color_at(stops, 1.)
            };
        }
        let t = (degrees - start_angle) / span;
        if matches!(extend, Extend::Pad) || matches!(extend, Extend::Unknown) {
            color_at(stops, t.clamp(0., 1.))
        } else {
            color_at(stops, extend_position(t, extend))
        }
    };
    let mut boundaries: Vec<f64> = vec![0., 360.];
    if span != 0. {
        for &(offset, _) in stops {
            let degrees = start_angle + offset * span;
            if (0. ..=360.).contains(&degrees) {
                boundaries.push(degrees);
            }
        }
    }
    boundaries.sort_by(f64::total_cmp);
    boundaries.dedup();
    for pair in boundaries.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let slices = ((to - from) / SWEEP_SLICE_DEGREES).ceil().max(1.) as usize;
        for slice in 0..slices {
            let a0 = from + (to - from) * slice as f64 / slices as f64;
            let a1 = from + (to - from) * (slice + 1) as f64 / slices as f64;
            let mid = (a0 + a1) / 2.;
            let (c0, c1) = if matches!(extend, Extend::Pad) && span != 0. {
                (color_for(a0), color_for(a1))
            } else {
                (
                    color_for(a0.min(mid - 1e-9).max(a0)),
                    color_for(a1.max(mid + 1e-9).min(a1)),
                )
            };
            let corner = |degrees: f64| {
                let radians = degrees.to_radians();
                (
                    center.0 + radius * radians.cos(),
                    center.1 - radius * radians.sin(),
                )
            };
            let (p0, p1) = (corner(a0), corner(a1));
            pattern.mesh_patch(center, p0, p1, c0.tuple(), c1.tuple());
        }
    }
    Some(pattern)
}

impl CairoPainter<'_, '_> {
    fn save(&mut self) {
        self.cr.save();
        self.frames.push(Frame::Saved);
    }

    fn restore(&mut self) {
        if matches!(self.frames.last(), Some(Frame::Saved)) {
            self.frames.pop();
            self.cr.restore();
        }
    }

    fn composite(&self, mode: CompositeMode) {
        self.cr.pop_group_to_source();
        self.cr.save();
        self.cr.set_operator(cairo_operator(mode));
        self.cr.paint();
        self.cr.restore();
    }

    fn unwind(&mut self) {
        while let Some(frame) = self.frames.pop() {
            match frame {
                Frame::Saved => self.cr.restore(),
                Frame::Layer(mode) => self.composite(mode),
            }
        }
    }
}

impl ColorPainter for CairoPainter<'_, '_> {
    fn push_transform(&mut self, transform: Transform) {
        self.save();
        self.cr.transform(&cairo_matrix(transform));
    }

    fn pop_transform(&mut self) {
        self.restore();
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        self.save();
        let mut outline = Outline::default();
        if let Some(glyph) = self.font.outline_glyphs().get(glyph_id) {
            let _ = glyph.draw(
                DrawSettings::unhinted(Size::unscaled(), self.location),
                &mut outline,
            );
        }
        outline.end_contour();
        self.cr.new_path();
        outline.draw_font_units(self.cr);
        self.cr.clip();
    }

    fn push_clip_box(&mut self, clip_box: BoundingBox<f32>) {
        self.save();
        self.cr.new_path();
        self.cr.rectangle(
            f64::from(clip_box.x_min),
            f64::from(clip_box.y_min),
            f64::from(clip_box.x_max - clip_box.x_min),
            f64::from(clip_box.y_max - clip_box.y_min),
        );
        self.cr.clip();
    }

    fn pop_clip(&mut self) {
        self.restore();
    }

    fn fill(&mut self, brush: Brush<'_>) {
        match brush {
            Brush::Solid {
                palette_index,
                alpha,
            } => self.fill_solid(palette_index, alpha),
            Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => {
                let stops = self.stops(color_stops);
                if stops.len() == 1 {
                    let color = stops[0].1;
                    self.cr
                        .set_source_rgba(color.red, color.green, color.blue, color.alpha);
                    self.cr.paint();
                    return;
                }
                let ((x0, y0), (x1, y1)) = (point(p0), point(p1));
                let pattern = CairoPattern::linear(x0, y0, x1, y1).inspect(|pattern| {
                    add_stops(pattern, &stops);
                    pattern.set_extend(cairo_extend(extend));
                });
                self.fill_pattern(pattern);
            }
            Brush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => {
                let stops = self.stops(color_stops);
                let Some(Radial {
                    c0,
                    r0,
                    c1,
                    r1,
                    stops,
                }) = radial_without_negative_radius(
                    (point(c0), f64::from(r0)),
                    (point(c1), f64::from(r1)),
                    &stops,
                )
                else {
                    return;
                };
                let pattern =
                    CairoPattern::radial(c0.0, c0.1, r0, c1.0, c1.1, r1).inspect(|pattern| {
                        add_stops(pattern, &stops);
                        pattern.set_extend(cairo_extend(extend));
                    });
                self.fill_pattern(pattern);
            }
            Brush::SweepGradient {
                c0,
                start_angle,
                end_angle,
                color_stops,
                extend,
            } => {
                let stops = self.stops(color_stops);
                let (x0, y0, x1, y1) = self.cr.clip_extents();
                let center = point(c0);
                let radius = [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
                    .iter()
                    .map(|&(x, y)| (x - center.0).hypot(y - center.1))
                    .fold(1., f64::max);
                let pattern = sweep_pattern(
                    center,
                    f64::from(start_angle),
                    f64::from(end_angle),
                    &stops,
                    extend,
                    radius * 1.5,
                );
                self.fill_pattern(pattern);
            }
        }
    }

    fn push_layer(&mut self, composite_mode: CompositeMode) {
        self.frames.push(Frame::Layer(composite_mode));
        self.cr.push_group();
    }

    fn pop_layer(&mut self) {
        if let Some(Frame::Layer(mode)) = self.frames.last() {
            let mode = *mode;
            self.frames.pop();
            self.composite(mode);
        }
    }
}
