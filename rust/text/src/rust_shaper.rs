//! Southstar text — shaping through HarfRust, a Rust port of HarfBuzz, giving the glyphs and positions HarfBuzz gives for a font set up the way web-text-lib sets it up.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};

use harfrust::font::{Blob, NormalizedCoord};
use harfrust::{
    Buffer, BufferFlags, ClusterLevel, Direction, FontFuncs, GlyphExtents, GlyphId, Language,
    ShapeOptions, ShapePlan, ShapePlanKey, ShaperFont, Tag,
};
use read_fonts::TableProvider;
use read_fonts::tables::bitmap::{BitmapContent, BitmapDataFormat};
use read_fonts::types::{F48Dot16, Fixed};

use crate::font::{Feature, Font, FontInner, get_ignorable, parse_variations};
use crate::script::Script;
use crate::types::{GLYPH_EMPTY, GLYPH_UNKNOWN_FLAG, Glyph, get_unknown_glyph, show_flags};
use crate::unicode::{UnicodeType, mirror_char, unichar_type};

#[cfg(test)]
#[path = "tests/rust_shaper.rs"]
mod tests;

pub mod buffer_flags {
    pub const BEGINNING_OF_TEXT: u32 = 0x01;
    pub const END_OF_TEXT: u32 = 0x02;
    pub const PRESERVE_DEFAULT_IGNORABLES: u32 = 0x04;
    pub const REMOVE_DEFAULT_IGNORABLES: u32 = 0x08;
    pub const DO_NOT_INSERT_DOTTED_CIRCLE: u32 = 0x10;
    pub const PRODUCE_UNSAFE_TO_CONCAT: u32 = 0x40;
}

pub mod glyph_flags {
    pub const UNSAFE_TO_BREAK: u32 = 0x01;
    pub const UNSAFE_TO_CONCAT: u32 = 0x02;
    pub const SAFE_TO_INSERT_TATWEEL: u32 = 0x04;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeDirection {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

impl ShapeDirection {
    pub fn reverse(self) -> ShapeDirection {
        match self {
            ShapeDirection::LeftToRight => ShapeDirection::RightToLeft,
            ShapeDirection::RightToLeft => ShapeDirection::LeftToRight,
            ShapeDirection::TopToBottom => ShapeDirection::BottomToTop,
            ShapeDirection::BottomToTop => ShapeDirection::TopToBottom,
        }
    }

    pub fn is_vertical(self) -> bool {
        matches!(
            self,
            ShapeDirection::TopToBottom | ShapeDirection::BottomToTop
        )
    }

    fn to_harfrust(self) -> Direction {
        match self {
            ShapeDirection::LeftToRight => Direction::LeftToRight,
            ShapeDirection::RightToLeft => Direction::RightToLeft,
            ShapeDirection::TopToBottom => Direction::TopToBottom,
            ShapeDirection::BottomToTop => Direction::BottomToTop,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ShapeRequest<'a> {
    pub paragraph: &'a str,
    pub item_start: usize,
    pub item_end: usize,
    pub transformed: Option<&'a [(u32, u32)]>,
    pub hyphen: Option<(u32, u32)>,
    pub direction: ShapeDirection,
    pub script: Script,
    pub language: Option<&'a str>,
    pub flags: u32,
    pub show_flags: u32,
    pub features: &'a [Feature],
}

impl<'a> ShapeRequest<'a> {
    pub fn new(paragraph: &'a str, item_start: usize, item_end: usize) -> ShapeRequest<'a> {
        ShapeRequest {
            paragraph,
            item_start,
            item_end,
            transformed: None,
            hyphen: None,
            direction: ShapeDirection::LeftToRight,
            script: Script::COMMON,
            language: None,
            flags: buffer_flags::BEGINNING_OF_TEXT | buffer_flags::END_OF_TEXT,
            show_flags: show_flags::NONE,
            features: &[],
        }
    }

    fn for_each_input_char(&self, mut visit: impl FnMut(u32, u32)) {
        match self.transformed {
            Some(chars) => chars.iter().for_each(|&(ch, cluster)| visit(ch, cluster)),
            None => self.paragraph[self.item_start..self.item_end]
                .char_indices()
                .for_each(|(offset, ch)| visit(u32::from(ch), (self.item_start + offset) as u32)),
        }
        if let Some((ch, cluster)) = self.hyphen {
            visit(ch, cluster);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShapedGlyph {
    pub glyph: Glyph,
    pub cluster: u32,
    pub flags: u32,
    pub x_advance: i32,
    pub y_advance: i32,
    pub x_offset: i32,
    pub y_offset: i32,
}

const STAND_IN_END: u32 = 1 << 14;
const MAX_CACHED_PLANS: usize = 64;

pub struct ShaperFace {
    font: harfrust::Font,
    default_instance: Option<harfrust::Font>,
    pub(crate) x_scale: i32,
    pub(crate) y_scale: i32,
    point_size: f32,
    slant_xy: f32,
    x_strength: i32,
    pub(crate) y_strength: i32,
    num_glyphs: u32,
    plans: Mutex<Vec<Arc<ShapePlan>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(crate) fn hb_roundf(value: f32) -> i32 {
    (value + 0.5).floor() as i32
}

fn varied_advance_units(default: F48Dot16, varied: F48Dot16) -> i32 {
    let delta = varied.to_bits() - default.to_bits();
    let rounded_delta = if delta < 0 {
        -((-delta + 0x8000) >> 16)
    } else {
        (delta + 0x8000) >> 16
    };
    ((default.to_bits() >> 16) + rounded_delta) as i32
}

type FileBytes = Arc<dyn AsRef<[u8]> + Send + Sync>;

pub(crate) fn file_blob(path: &str) -> Option<Blob> {
    static FILES: OnceLock<Mutex<HashMap<String, std::sync::Weak<Vec<u8>>>>> = OnceLock::new();
    let files = FILES.get_or_init(Default::default);
    let cached = lock(files).get(path).and_then(std::sync::Weak::upgrade);
    let bytes = match cached {
        Some(bytes) => bytes,
        None => {
            let bytes = Arc::new(std::fs::read(path).ok()?);
            let mut files = lock(files);
            files.retain(|_, weak| weak.strong_count() > 0);
            files.insert(path.to_owned(), Arc::downgrade(&bytes));
            bytes
        }
    };
    let shared: FileBytes = bytes;
    Some(Blob::from(shared))
}

fn load_face(path: &str, index: u32) -> Option<harfrust::Font> {
    let blob = file_blob(path)?;
    let is_collection = blob.as_ref().starts_with(b"ttcf");
    harfrust::Font::new(blob.clone(), index).or_else(|| {
        (index != 0 && !is_collection)
            .then(|| harfrust::Font::new(blob, 0))
            .flatten()
    })
}

pub fn shaper_face(font: &Font) -> Option<Arc<ShaperFace>> {
    type Entry = (Weak<FontInner>, Option<Arc<ShaperFace>>);
    static SHAPERS: OnceLock<Mutex<HashMap<usize, Entry>>> = OnceLock::new();
    let shapers = SHAPERS.get_or_init(Default::default);
    if let Some((weak, face)) = lock(shapers).get(&font.id())
        && weak
            .upgrade()
            .is_some_and(|inner| Font::from_inner(inner).ptr_eq(font))
    {
        return face.clone();
    }
    let face = ShaperFace::new(font).map(Arc::new);
    let mut shapers = lock(shapers);
    if shapers.len() >= 256 {
        shapers.retain(|_, (weak, _)| weak.strong_count() > 0);
    }
    shapers.insert(font.id(), (font.downgrade(), face.clone()));
    face
}

pub fn shape(font: &Font, request: &ShapeRequest<'_>) -> Option<Vec<ShapedGlyph>> {
    Some(shaper_face(font)?.shape(font, request))
}

fn normalized_coords(face: &harfrust::Font, font: &Font, point_size: f32) -> Vec<NormalizedCoord> {
    let axes: Vec<_> = face.axes().iter().collect();
    if axes.is_empty() {
        return Vec::new();
    }
    let opsz = Tag::new(b"opsz");
    let mut coords: Vec<f32> = vec![0.; axes.len()];
    for axis in &axes {
        if let Some(coord) = coords.get_mut(axis.index() as usize) {
            *coord = if axis.tag() == opsz {
                point_size
            } else {
                axis.default_value()
            };
        }
    }
    let mut n_axes = axes.len();
    let index = font.face_index();
    if index != 0 {
        let instance = ((index >> 16) as u32).wrapping_sub(1);
        n_axes = match face.named_instances().get(instance) {
            Some(named) => {
                for (coord, value) in coords.iter_mut().zip(named.user_coords()) {
                    *coord = value;
                }
                axes.len()
            }
            None => 0,
        };
    }
    let axis_tags: Vec<(u32, u32)> = axes
        .iter()
        .take(n_axes)
        .map(|axis| (u32::from_be_bytes(axis.tag().to_be_bytes()), axis.index()))
        .collect();
    if let Some(variations) = font.pattern_variations() {
        parse_variations(&variations, &axis_tags, &mut coords);
    }
    if let Some(variations) = font.variations() {
        parse_variations(variations, &axis_tags, &mut coords);
    }
    let tables = face.tables();
    let Ok(fvar) = tables.fvar() else {
        return Vec::new();
    };
    let mut normalized = vec![NormalizedCoord::ZERO; axes.len()];
    fvar.user_to_normalized(
        tables.avar().ok().as_ref(),
        axes.iter().take(n_axes).map(|axis| {
            (
                axis.tag(),
                Fixed::from_f64(f64::from(coords[axis.index() as usize])),
            )
        }),
        &mut normalized,
    );
    normalized
}

impl ShaperFace {
    pub fn new(font: &Font) -> Option<ShaperFace> {
        let path = font.file()?;
        let index = (font.face_index() as u32) & 0xFFFF;
        let face = load_face(&path, index)?;
        let setup = font.hb_font_setup();
        let coords = normalized_coords(&face, font, setup.point_size);
        let face = if coords.iter().all(|&coord| coord == NormalizedCoord::ZERO) {
            face
        } else {
            face.instance_builder().normalized_coords(coords).build()
        };
        let slant_xy = if setup.y_scale != 0 {
            setup.slant * setup.x_scale as f32 / setup.y_scale as f32
        } else {
            0.
        };
        Some(ShaperFace {
            num_glyphs: face.num_glyphs(),
            x_scale: setup.x_scale,
            y_scale: setup.y_scale,
            point_size: setup.point_size,
            slant_xy,
            x_strength: hb_roundf(setup.x_scale.unsigned_abs() as f32 * setup.embolden),
            y_strength: hb_roundf(setup.y_scale.unsigned_abs() as f32 * setup.embolden),
            default_instance: (!face.normalized_coords().is_empty())
                .then(|| face.default_instance()),
            font: face,
            plans: Mutex::new(Vec::new()),
        })
    }

    pub fn font(&self) -> &harfrust::Font {
        &self.font
    }

    pub fn num_glyphs(&self) -> u32 {
        self.num_glyphs
    }

    fn shaper_font<'f>(&self, funcs: Option<&'f dyn FontFuncs>) -> ShaperFont<'_, 'f> {
        ShaperFont::new(&self.font)
            .with_scale_separate(self.x_scale, self.y_scale)
            .with_font_funcs(funcs)
    }

    pub fn nominal_glyph(&self, ch: u32) -> Option<Glyph> {
        self.shaper_font(None)
            .default_nominal_glyph(ch)
            .map(GlyphId::to_u32)
    }

    fn signed_strength(strength: i32, scale: i32) -> i32 {
        if scale >= 0 { strength } else { -strength }
    }

    fn embolden_x(&self, advance: i32) -> i32 {
        if advance != 0 && self.x_strength != 0 {
            advance + Self::signed_strength(self.x_strength, self.x_scale)
        } else {
            advance
        }
    }

    fn synthesize_extents(&self, mut extents: GlyphExtents) -> GlyphExtents {
        if self.slant_xy != 0. {
            let x1 = extents.x_bearing as f32;
            let y1 = extents.y_bearing as f32;
            let x2 = (extents.x_bearing + extents.width) as f32;
            let y2 = (extents.y_bearing + extents.height) as f32;
            let x1 = x1 + (y1 * self.slant_xy).min(y2 * self.slant_xy).floor();
            let x2 = x2 + (y1 * self.slant_xy).max(y2 * self.slant_xy).ceil();
            extents.x_bearing = x1 as i32;
            extents.width = x2 as i32 - extents.x_bearing;
        }
        if self.x_strength != 0 || self.y_strength != 0 {
            let y_shift = Self::signed_strength(self.y_strength, self.y_scale);
            extents.y_bearing += y_shift;
            extents.height -= y_shift;
            extents.width += Self::signed_strength(self.x_strength, self.x_scale);
        }
        extents
    }

    fn h_advance(&self, font: &ShaperFont<'_, '_>, glyph: GlyphId) -> i32 {
        let advance = match &self.default_instance {
            Some(default) => {
                let units = varied_advance_units(
                    default.glyph_metrics().h_advance_exact(glyph),
                    self.font.glyph_metrics().h_advance_exact(glyph),
                );
                font.scale().scale_x(units).max(0)
            }
            None => font.default_glyph_h_advance(glyph),
        };
        self.embolden_x(advance)
    }

    fn v_advance(&self, font: &ShaperFont<'_, '_>, glyph: GlyphId) -> i32 {
        match &self.default_instance {
            Some(default) => {
                let units = varied_advance_units(
                    default.glyph_metrics().v_advance_exact(glyph),
                    self.font.glyph_metrics().v_advance_exact(glyph),
                );
                -font.scale().scale_y(units).max(0)
            }
            None => font.default_glyph_v_advance(glyph),
        }
    }

    fn extents(&self, font: &ShaperFont<'_, '_>, glyph: GlyphId) -> Option<GlyphExtents> {
        font.default_glyph_extents(glyph)
            .map(|extents| self.synthesize_extents(extents))
    }

    pub fn glyph_h_advance(&self, glyph: Glyph) -> i32 {
        self.h_advance(&self.shaper_font(None), GlyphId::new(glyph))
    }

    pub fn glyph_v_advance(&self, glyph: Glyph) -> i32 {
        self.v_advance(&self.shaper_font(None), GlyphId::new(glyph))
    }

    pub fn glyph_extents(&self, glyph: Glyph) -> Option<GlyphExtents> {
        self.extents(&self.shaper_font(None), GlyphId::new(glyph))
    }

    fn plan(
        &self,
        direction: Direction,
        script: Option<harfrust::Script>,
        language: Option<&Language>,
        features: &[harfrust::Feature],
    ) -> Arc<ShapePlan> {
        let key = ShapePlanKey::new(&self.font, script, direction)
            .language(language)
            .features(features);
        let mut plans = lock(&self.plans);
        if let Some(plan) = plans.iter().find(|plan| key.matches(plan)) {
            return Arc::clone(plan);
        }
        let plan = Arc::new(ShapePlan::new(
            &self.font, direction, script, language, features,
        ));
        if plans.len() >= MAX_CACHED_PLANS {
            plans.remove(0);
        }
        plans.push(Arc::clone(&plan));
        plan
    }

    pub fn shape(&self, font: &Font, request: &ShapeRequest<'_>) -> Vec<ShapedGlyph> {
        let features: Vec<harfrust::Feature> = request
            .features
            .iter()
            .map(|feature| harfrust::Feature {
                tag: Tag::from_u32(feature.tag),
                value: feature.value,
                start: feature.start,
                end: feature.end,
            })
            .collect();
        let direction = request.direction.to_harfrust();
        let script = script_to_harfrust(request.script);
        let language = request.language.and_then(Language::new);
        let plan = self.plan(direction, script, language.as_ref(), &features);
        let pass = ShapePass {
            face: self,
            font,
            request,
            plan: &plan,
            features: &features,
            script,
            language: language.as_ref(),
        };
        let (mut glyphs, missing) = pass.run(&[]);
        if !missing.is_empty() && glyphs.iter().any(|glyph| glyph.glyph == 0) {
            let forced = self.forced_unknowns(request, &glyphs, &missing);
            if !forced.is_empty() {
                glyphs = pass.run(&forced).0;
            }
        }
        self.settle_invisible_glyphs(request, &mut glyphs);
        glyphs
    }

    fn forced_unknowns(
        &self,
        request: &ShapeRequest<'_>,
        glyphs: &[ShapedGlyph],
        missing: &[u32],
    ) -> Vec<u32> {
        let notdef_ranges: Vec<(u32, u32)> = glyphs
            .iter()
            .filter(|glyph| glyph.glyph == 0)
            .map(|notdef| {
                let end = glyphs
                    .iter()
                    .map(|glyph| glyph.cluster)
                    .filter(|&cluster| cluster > notdef.cluster)
                    .min()
                    .unwrap_or(u32::MAX);
                (notdef.cluster, end)
            })
            .collect();
        let mut forced = Vec::new();
        request.for_each_input_char(|ch, cluster| {
            let in_notdef = notdef_ranges
                .iter()
                .any(|&(start, end)| (start..end).contains(&cluster));
            if !in_notdef || !missing.contains(&ch) {
                return;
            }
            if !forced.contains(&ch) {
                forced.push(ch);
            }
        });
        forced
    }

    fn settle_invisible_glyphs(&self, request: &ShapeRequest<'_>, glyphs: &mut [ShapedGlyph]) {
        let preserve = request.flags & buffer_flags::PRESERVE_DEFAULT_IGNORABLES != 0;
        for glyph in glyphs.iter_mut() {
            if glyph.glyph != GLYPH_EMPTY || glyph.x_advance != 0 || glyph.y_advance != 0 {
                continue;
            }
            let mut source = None;
            request.for_each_input_char(|ch, cluster| {
                if cluster == glyph.cluster && source.is_none() {
                    source = Some(ch);
                }
            });
            if matches!(source, Some(0x20 | 0xA0)) {
                if request.direction.is_vertical() {
                    glyph.y_advance = -((self.y_scale + 2) / 4);
                } else {
                    glyph.x_advance = (self.x_scale + 2) / 4;
                }
            } else if !preserve && source != Some(0xAD) {
                glyph.x_offset = 0;
                glyph.y_offset = 0;
            }
        }
    }

    pub fn has_color(&self) -> bool {
        let tables = self.font.tables();
        let colr = tables.colr().ok();
        let has_paint = colr.as_ref().is_some_and(|colr| {
            colr.version() >= 1
                && colr
                    .base_glyph_list()
                    .and_then(Result::ok)
                    .is_some_and(|list| list.num_base_glyph_paint_records() > 0)
        });
        let has_layers = colr
            .as_ref()
            .is_some_and(|colr| colr.num_base_glyph_records() > 0);
        has_paint || has_layers || self.has_png() || self.has_svg()
    }

    fn has_png(&self) -> bool {
        let tables = self.font.tables();
        tables.cbdt().is_ok() || tables.sbix().is_ok_and(|sbix| sbix.version() != 0)
    }

    fn has_svg(&self) -> bool {
        self.font.tables().svg().is_ok_and(|svg| {
            svg.svg_document_list()
                .is_ok_and(|list| !list.document_records().is_empty())
        })
    }

    pub fn glyph_has_color(&self, glyph: Glyph) -> bool {
        if glyph >= self.num_glyphs {
            return false;
        }
        let gid = GlyphId::new(glyph);
        let tables = self.font.tables();
        if let Ok(colr) = tables.colr() {
            if colr.version() == 1 && colr.v1_base_glyph(gid).is_some() {
                return true;
            }
            if colr
                .v0_base_glyph(gid)
                .is_some_and(|layers| !layers.is_empty())
            {
                return true;
            }
        }
        if self.has_png() && self.png_glyph_has_data(gid) {
            return true;
        }
        self.has_svg()
            && tables
                .svg()
                .ok()
                .and_then(|svg| svg.glyph_data(gid))
                .is_some_and(|data| !data.is_empty())
    }

    fn png_glyph_has_data(&self, gid: GlyphId) -> bool {
        let tables = self.font.tables();
        if let Ok(sbix) = tables.sbix()
            && sbix.version() != 0
            && sbix_png_has_data(&sbix, gid, self.num_glyphs)
        {
            return true;
        }
        let (Ok(cblc), Ok(cbdt)) = (tables.cblc(), tables.cbdt()) else {
            return false;
        };
        let sizes = cblc.bitmap_sizes();
        let ppem = |size: &read_fonts::tables::bitmap::BitmapSize| {
            u32::from(size.ppem_x().max(size.ppem_y()))
        };
        let Some(first) = sizes.first() else {
            return false;
        };
        let requested = 1u32 << 30;
        let mut best = first;
        let mut best_ppem = ppem(first);
        for size in &sizes[1..] {
            let size_ppem = ppem(size);
            if (requested <= size_ppem && size_ppem < best_ppem)
                || (requested > best_ppem && size_ppem > best_ppem)
            {
                best = size;
                best_ppem = size_ppem;
            }
        }
        if best.ppem_x() == 0 || best.ppem_y() == 0 {
            return false;
        }
        let Ok(location) = best.location(cblc.offset_data(), gid) else {
            return false;
        };
        if !matches!(location.format, 17..=19) {
            return false;
        }
        cbdt.data(&location).is_ok_and(|data| {
            matches!(data.content, BitmapContent::Data(BitmapDataFormat::Png, bytes) if !bytes.is_empty())
        })
    }
}

fn sbix_png_has_data(
    sbix: &read_fonts::tables::sbix::Sbix<'_>,
    gid: GlyphId,
    num_glyphs: u32,
) -> bool {
    let strikes = sbix.strikes();
    let Ok(first) = strikes.get(0).map(|strike| strike.ppem()) else {
        return false;
    };
    let requested = 1u32 << 30;
    let mut best_index = 0;
    let mut best_ppem = u32::from(first);
    for index in 1..strikes.len() {
        let Ok(strike) = strikes.get(index) else {
            continue;
        };
        let ppem = u32::from(strike.ppem());
        if (requested <= ppem && ppem < best_ppem) || (requested > best_ppem && ppem > best_ppem) {
            best_index = index;
            best_ppem = ppem;
        }
    }
    let Ok(strike) = strikes.get(best_index) else {
        return false;
    };
    let png = Tag::new(b"png ");
    let dupe = Tag::new(b"dupe");
    let mut glyph = gid;
    for _ in 0..=8 {
        if glyph.to_u32() >= num_glyphs {
            return false;
        }
        let Ok(Some(data)) = strike.glyph_data(glyph) else {
            return false;
        };
        if data.graphic_type() == dupe {
            match data.data() {
                [high, low, ..] => {
                    glyph = GlyphId::new(u32::from(u16::from_be_bytes([*high, *low])))
                }
                _ => return false,
            }
            continue;
        }
        return data.graphic_type() == png && !data.data().is_empty();
    }
    false
}

pub fn script_to_iso15924(script: Script) -> u32 {
    match u8::try_from(script.0) {
        Err(_) => 0,
        Ok(index) => southstar_unicode::script_tag(index),
    }
}

fn script_to_harfrust(script: Script) -> Option<harfrust::Script> {
    harfrust::Script::from_iso15924_tag(Tag::from_u32(script_to_iso15924(script)))
}

pub fn is_default_ignorable(ch: u32) -> bool {
    match ch >> 16 {
        0 => match ch >> 8 {
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
        },
        0x01 => (0x1d173..=0x1d17a).contains(&ch),
        0x0e => (0xe0000..=0xe0fff).contains(&ch),
        _ => false,
    }
}

struct StandIns {
    base: u32,
    made_up: RefCell<Vec<Glyph>>,
}

impl StandIns {
    fn stand_in_for(&self, made_up: Glyph) -> Glyph {
        let mut list = self.made_up.borrow_mut();
        if let Some(index) = list.iter().position(|&glyph| glyph == made_up) {
            return self.base + index as u32;
        }
        let stand_in = self.base + list.len() as u32;
        if stand_in >= STAND_IN_END {
            return made_up;
        }
        list.push(made_up);
        stand_in
    }

    fn made_up_for(&self, glyph: Glyph) -> Glyph {
        let list = self.made_up.borrow();
        glyph
            .checked_sub(self.base)
            .and_then(|index| list.get(index as usize).copied())
            .unwrap_or(glyph)
    }
}

struct PangoFuncs<'a> {
    face: &'a ShaperFace,
    font: &'a Font,
    show_flags: u32,
    forced: &'a [u32],
    mirrors: Vec<u32>,
    mirror_queries_left: Cell<usize>,
    stand_ins: StandIns,
    missing: RefCell<Vec<u32>>,
}

impl PangoFuncs<'_> {
    fn nominal_or_unknown(&self, font: &ShaperFont<'_, '_>, ch: u32) -> (bool, Glyph) {
        let mut ch = ch;
        if self.show_flags != 0 {
            if self.show_flags & show_flags::SPACES != 0
                && unichar_type(ch) == UnicodeType::SpaceSeparator
            {
                if ch == 0x20 {
                    ch = 0x2423;
                } else {
                    return (true, get_unknown_glyph(ch));
                }
            }
            if self.show_flags & show_flags::IGNORABLES != 0 && is_default_ignorable(ch) {
                return if get_ignorable(ch).is_some() {
                    (true, get_unknown_glyph(ch))
                } else {
                    (true, GLYPH_EMPTY)
                };
            }
            if self.show_flags & show_flags::LINE_BREAKS != 0 && ch == 0x2028 {
                return (true, get_unknown_glyph(ch));
            }
        }
        if let Some(glyph) = font.default_nominal_glyph(ch) {
            return (true, glyph.to_u32());
        }
        if unichar_type(ch) == UnicodeType::SpaceSeparator {
            return if ch == 0x20 {
                (true, get_unknown_glyph(0x20))
            } else {
                (false, 0)
            };
        }
        (ch > 0x10FFFF, get_unknown_glyph(ch))
    }

    fn made_up(&self, glyph: GlyphId) -> Glyph {
        self.stand_ins.made_up_for(glyph.to_u32())
    }

    fn made_up_extents(&self, glyph: Glyph) -> (crate::types::Rectangle, crate::types::Rectangle) {
        self.font.glyph_extents(glyph)
    }
}

impl FontFuncs for PangoFuncs<'_> {
    fn nominal_glyph(&self, font: &ShaperFont, ch: u32) -> Option<GlyphId> {
        let (found, glyph) = self.nominal_or_unknown(font, ch);
        let glyph = if glyph > 0xFFFF {
            self.stand_ins.stand_in_for(glyph)
        } else {
            glyph
        };
        let mirror_query = self.mirror_queries_left.get() > 0 && self.mirrors.contains(&ch);
        if mirror_query {
            self.mirror_queries_left
                .set(self.mirror_queries_left.get() - 1);
        }
        if found || (glyph != 0 && !mirror_query && self.forced.contains(&ch)) {
            return Some(GlyphId::new(glyph));
        }
        if glyph != 0 {
            let mut missing = self.missing.borrow_mut();
            if !missing.contains(&ch) {
                missing.push(ch);
            }
        }
        None
    }

    fn glyph_h_advance(&self, font: &ShaperFont, glyph: GlyphId) -> i32 {
        let glyph = self.made_up(glyph);
        if glyph & GLYPH_UNKNOWN_FLAG != 0 {
            return self.face.embolden_x(self.made_up_extents(glyph).1.width);
        }
        self.face
            .embolden_x(self.face.h_advance(font, GlyphId::new(glyph)))
    }

    fn glyph_v_advance(&self, font: &ShaperFont, glyph: GlyphId) -> i32 {
        let glyph = self.made_up(glyph);
        if glyph & GLYPH_UNKNOWN_FLAG != 0 {
            return self.made_up_extents(glyph).1.height;
        }
        self.face.v_advance(font, GlyphId::new(glyph))
    }

    fn glyph_v_origin(&self, font: &ShaperFont, glyph: GlyphId) -> (i32, i32) {
        let made_up = self.made_up(glyph);
        if made_up != glyph.to_u32() || made_up >= self.face.num_glyphs {
            let tables = self.face.font.tables();
            if tables.vmtx().is_ok() || tables.vorg().is_ok() {
                return (0, 0);
            }
            let outside = GlyphId::new(self.face.num_glyphs);
            return (
                0,
                self.face
                    .shaper_font(None)
                    .default_glyph_v_origin(outside)
                    .1,
            );
        }
        (
            self.face.h_advance(font, glyph) / 2,
            font.default_glyph_v_origin(glyph).1,
        )
    }

    fn glyph_extents(&self, font: &ShaperFont, glyph: GlyphId) -> Option<GlyphExtents> {
        let glyph = self.made_up(glyph);
        if glyph & GLYPH_UNKNOWN_FLAG != 0 {
            let ink = self.made_up_extents(glyph).0;
            return Some(self.face.synthesize_extents(GlyphExtents {
                x_bearing: ink.x,
                y_bearing: ink.y,
                width: ink.width,
                height: ink.height,
            }));
        }
        self.face
            .extents(font, GlyphId::new(glyph))
            .map(|extents| self.face.synthesize_extents(extents))
    }
}

struct ShapePass<'a> {
    face: &'a ShaperFace,
    font: &'a Font,
    request: &'a ShapeRequest<'a>,
    plan: &'a ShapePlan,
    features: &'a [harfrust::Feature],
    script: Option<harfrust::Script>,
    language: Option<&'a Language>,
}

thread_local! {
    static BUFFER: Cell<Option<Buffer>> = const { Cell::new(None) };
}

impl ShapePass<'_> {
    fn fill(&self, buffer: &mut Buffer) {
        let request = self.request;
        buffer.set_direction(request.direction.to_harfrust());
        buffer.set_script(self.script);
        buffer.set_language(self.language.cloned());
        buffer.set_cluster_level(ClusterLevel::MonotoneCharacters);
        buffer.set_flags(BufferFlags::from_bits_truncate(request.flags));
        buffer.set_invisible_glyph(Some(GlyphId::new(GLYPH_EMPTY)));
        buffer.set_pre_context(&request.paragraph[..request.item_start]);
        match request.transformed {
            Some(chars) => chars
                .iter()
                .for_each(|&(ch, cluster)| buffer.push(ch, cluster)),
            None => request.paragraph[request.item_start..request.item_end]
                .char_indices()
                .for_each(|(offset, ch)| {
                    buffer.push(u32::from(ch), (request.item_start + offset) as u32)
                }),
        }
        match request.hyphen {
            Some((ch, cluster)) => {
                buffer.push(ch, cluster);
                buffer.set_post_context("");
            }
            None => buffer.set_post_context(&request.paragraph[request.item_end..]),
        }
    }

    fn run(&self, forced: &[u32]) -> (Vec<ShapedGlyph>, Vec<u32>) {
        let mut mirrors = Vec::new();
        let backward = matches!(
            self.request.direction,
            ShapeDirection::RightToLeft | ShapeDirection::BottomToTop
        );
        if backward && !forced.is_empty() {
            self.request
                .for_each_input_char(|ch, _| mirrors.extend(mirror_char(ch)));
        }
        let funcs = PangoFuncs {
            face: self.face,
            font: self.font,
            show_flags: self.request.show_flags,
            forced,
            mirror_queries_left: Cell::new(mirrors.len()),
            mirrors,
            stand_ins: StandIns {
                base: self.face.num_glyphs,
                made_up: RefCell::new(Vec::new()),
            },
            missing: RefCell::new(Vec::new()),
        };
        let shaper_font = self.face.shaper_font(Some(&funcs));
        let mut buffer = BUFFER.take().unwrap_or_default();
        buffer.clear();
        self.fill(&mut buffer);
        let options = ShapeOptions::new()
            .plan(Some(self.plan))
            .features(self.features)
            .point_size(Some(self.face.point_size));
        let shaped = harfrust::shape(&shaper_font, &mut buffer, options).is_ok();
        let glyphs = if shaped {
            self.read(&buffer, &funcs)
        } else {
            Vec::new()
        };
        buffer.clear();
        BUFFER.set(Some(buffer));
        (glyphs, funcs.missing.into_inner())
    }

    fn read(&self, buffer: &Buffer, funcs: &PangoFuncs<'_>) -> Vec<ShapedGlyph> {
        let slant_xy = self.face.slant_xy;
        buffer
            .glyph_infos()
            .iter()
            .zip(buffer.glyph_positions())
            .map(|(info, position)| {
                let mut flags = 0;
                if info.unsafe_to_break() {
                    flags |= glyph_flags::UNSAFE_TO_BREAK;
                }
                if info.unsafe_to_concat() {
                    flags |= glyph_flags::UNSAFE_TO_CONCAT;
                }
                if info.safe_to_insert_tatweel() {
                    flags |= glyph_flags::SAFE_TO_INSERT_TATWEEL;
                }
                let mut x_offset = position.x_offset;
                if slant_xy != 0. && position.y_offset != 0 {
                    x_offset += hb_roundf(slant_xy * position.y_offset as f32);
                }
                ShapedGlyph {
                    glyph: funcs.stand_ins.made_up_for(info.glyph_id),
                    cluster: info.cluster,
                    flags,
                    x_advance: position.x_advance,
                    y_advance: position.y_advance,
                    x_offset,
                    y_offset: position.y_offset,
                }
            })
            .collect()
    }
}
