//! Southstar text — the font-table queries layout makes of a font, answered by HarfBuzz or, for Rust-backed fonts, by read-fonts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#[cfg(feature = "rust-shaper")]
use std::sync::Arc;

#[cfg(feature = "rust-shaper")]
use crate::ffi::cairo::{Antialias, CairoMatrix, FontOptions, HintStyle};
#[cfg(feature = "rust-shaper")]
use crate::ffi::fontconfig::{
    FC_ANTIALIAS, FC_AUTOHINT, FC_EMBOLDEN, FC_HINT_FULL, FC_HINT_STYLE, FC_HINTING,
};
use crate::ffi::harfbuzz::FontExtents;
use crate::font::Font;
#[cfg(feature = "rust-shaper")]
use crate::rust_glyphs::{GlyphSettings, GlyphSource, HintMode};
#[cfg(feature = "rust-shaper")]
use crate::rust_shaper::ShaperFace;
use crate::types::Glyph;

pub const WEB_FONT_OBJECT: &core::ffi::CStr = c"southstarwebfont";

#[cfg(feature = "rust-shaper")]
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RustFonts {
    Off,
    Web,
    All,
}

#[cfg(feature = "rust-shaper")]
pub fn rust_fonts() -> RustFonts {
    static POLICY: std::sync::OnceLock<RustFonts> = std::sync::OnceLock::new();
    *POLICY.get_or_init(|| match std::env::var("NS_RUST_FONTS").as_deref() {
        Ok("all") => RustFonts::All,
        Ok("web") => RustFonts::Web,
        Ok("off") => RustFonts::Off,
        _ if std::env::var("NS_TEXT_SHAPER").as_deref() == Ok("harfbuzz") => RustFonts::Off,
        _ => RustFonts::Web,
    })
}

#[cfg(feature = "rust-shaper")]
pub fn accepts_web_font(data: &[u8]) -> bool {
    use read_fonts::{FileRef, TableProvider};
    let Ok(file) = FileRef::new(data) else {
        return false;
    };
    let mut fonts = file.fonts().peekable();
    fonts.peek().is_some()
        && fonts.all(|font| {
            font.is_ok_and(|font| {
                font.head().is_ok()
                    && font.maxp().is_ok()
                    && font.cmap().is_ok()
                    && font.hhea().is_ok()
                    && font.hmtx().is_ok()
            })
        })
}

#[cfg(not(feature = "rust-shaper"))]
pub fn accepts_web_font(_data: &[u8]) -> bool {
    true
}

#[cfg(feature = "rust-shaper")]
pub fn install_web_font(path: &str, data: &[u8], family: &str, weight: i32, slant: i32) -> bool {
    crate::web_font::install_web_font(
        path,
        data,
        family,
        crate::web_font::Descriptors { weight, slant },
    )
}

#[cfg(not(feature = "rust-shaper"))]
pub fn install_web_font(
    _path: &str,
    _data: &[u8],
    _family: &str,
    _weight: i32,
    _slant: i32,
) -> bool {
    false
}

impl Font {
    #[cfg(feature = "rust-shaper")]
    pub(crate) fn is_web_font(&self) -> bool {
        self.pattern().get_bool(WEB_FONT_OBJECT, 0) == Some(true)
    }

    #[cfg(feature = "rust-shaper")]
    pub(crate) fn is_rust_backed(&self) -> bool {
        match rust_fonts() {
            RustFonts::All => true,
            RustFonts::Web => self.is_web_font(),
            RustFonts::Off => false,
        }
    }

    #[cfg(feature = "rust-shaper")]
    pub(crate) fn rust_face(&self) -> Option<&Arc<ShaperFace>> {
        if !self.is_rust_backed() {
            return None;
        }
        self.rust_face_slot()
            .get_or_init(|| crate::rust_shaper::shaper_face(self))
            .as_ref()
    }

    #[cfg(feature = "rust-shaper")]
    pub(crate) fn glyph_source(&self) -> Option<&Arc<GlyphSource>> {
        self.rust_face()?;
        self.glyph_source_slot()
            .get_or_init(|| self.new_glyph_source())
            .as_ref()
    }

    #[cfg(feature = "rust-shaper")]
    pub(crate) fn new_glyph_source(&self) -> Option<Arc<GlyphSource>> {
        let face = crate::rust_shaper::shaper_face(self)?;
        let blob = crate::rust_shaper::file_blob(&self.file()?)?;
        let index = (self.face_index() as u32) & 0xFFFF;
        let coords = face.font().normalized_coords().to_vec();
        GlyphSource::new(blob, index, coords, self.glyph_settings()).map(Arc::new)
    }

    #[cfg(feature = "rust-shaper")]
    fn glyph_settings(&self) -> GlyphSettings {
        let combined = CairoMatrix::multiply(self.font_matrix(), self.cairo_ctm());
        let x_scale = combined.xx.hypot(combined.yx);
        let determinant = (combined.xx * combined.yy - combined.xy * combined.yx).abs();
        let ppem = if x_scale > 0. {
            determinant / x_scale
        } else {
            0.
        };
        let pattern = self.pattern();
        let options = self.font_options();
        let antialias = pattern.get_bool(FC_ANTIALIAS, 0).unwrap_or(true)
            && options.is_none_or(|options| options.antialias() != Antialias::None as i32);
        let pattern_style = if !pattern.get_bool(FC_ANTIALIAS, 0).unwrap_or(true) {
            HintStyle::Default
        } else if !pattern.get_bool(FC_HINTING, 0).unwrap_or(true) {
            HintStyle::None
        } else {
            match pattern
                .get_integer(FC_HINT_STYLE, 0)
                .unwrap_or(FC_HINT_FULL)
            {
                0 => HintStyle::None,
                1 => HintStyle::Slight,
                2 => HintStyle::Medium,
                _ => HintStyle::Full,
            }
        };
        let mut style = options.map_or(HintStyle::Default as i32, FontOptions::hint_style);
        if style == HintStyle::Default as i32 {
            style = pattern_style as i32;
        }
        if pattern_style == HintStyle::None {
            style = HintStyle::None as i32;
        }
        let hinting = match (antialias, style) {
            (_, s) if s == HintStyle::None as i32 => HintMode::None,
            (false, _) => HintMode::Mono,
            (true, s) if s == HintStyle::Slight as i32 => HintMode::Light,
            _ => HintMode::Normal,
        };
        GlyphSettings {
            ppem: ppem as f32,
            hinting,
            force_autohint: pattern.get_bool(FC_AUTOHINT, 0).unwrap_or(false),
            embolden: pattern.get_bool(FC_EMBOLDEN, 0).unwrap_or(false),
            hint_metrics: options.is_none_or(|options| !options.hint_metrics_off()),
        }
    }

    pub(crate) fn nominal_glyph(&self, ch: u32) -> Option<Glyph> {
        #[cfg(feature = "rust-shaper")]
        if let Some(face) = self.rust_face() {
            return face.nominal_glyph(ch);
        }
        let mut glyph = 0;
        self.hb_font()
            .nominal_glyph_into(ch, &mut glyph)
            .then_some(glyph)
    }

    pub(crate) fn glyph_h_advance(&self, glyph: Glyph) -> i32 {
        #[cfg(feature = "rust-shaper")]
        if let Some(face) = self.rust_face() {
            return face.glyph_h_advance(glyph);
        }
        self.hb_font().glyph_h_advance(glyph)
    }

    pub(crate) fn glyph_class(&self, glyph: Glyph) -> i32 {
        #[cfg(feature = "rust-shaper")]
        if let Some(face) = self.rust_face() {
            return face.glyph_class(glyph);
        }
        self.hb_font().glyph_class(glyph)
    }

    pub(crate) fn ligature_caret(
        &self,
        direction: i32,
        glyph: Glyph,
        start_offset: u32,
    ) -> (u32, u32, i32) {
        #[cfg(feature = "rust-shaper")]
        if let Some(face) = self.rust_face() {
            return face.ligature_caret(direction & !1 == 4, glyph, start_offset);
        }
        self.hb_font()
            .ligature_caret(direction, glyph, start_offset)
    }

    pub(crate) fn ot_metric(&self, metrics_tag: u32) -> Option<i32> {
        #[cfg(feature = "rust-shaper")]
        if let Some(face) = self.rust_face() {
            return face.ot_metric(metrics_tag);
        }
        self.hb_font().ot_metrics_position(metrics_tag)
    }

    pub(crate) fn y_scale(&self) -> i32 {
        #[cfg(feature = "rust-shaper")]
        if let Some(face) = self.rust_face() {
            return face.y_scale;
        }
        self.hb_font().y_scale()
    }

    pub(crate) fn gsub_features_supported(
        &self,
        script_tag: u32,
        language: Option<&str>,
        features: &[u32],
    ) -> bool {
        #[cfg(feature = "rust-shaper")]
        if let Some(face) = self.rust_face() {
            return face.gsub_features_supported(script_tag, language, features);
        }
        self.hb_font()
            .gsub_features_supported(script_tag, language, features)
    }

    pub(crate) fn h_extents(&self) -> FontExtents {
        #[cfg(feature = "rust-shaper")]
        if let Some(face) = self.rust_face() {
            return face.h_extents();
        }
        self.hb_font().h_extents()
    }

    pub(crate) fn extents_ltr(&self) -> FontExtents {
        #[cfg(feature = "rust-shaper")]
        if let Some(face) = self.rust_face() {
            return face.extents_ltr();
        }
        self.hb_font().extents_ltr()
    }
}
