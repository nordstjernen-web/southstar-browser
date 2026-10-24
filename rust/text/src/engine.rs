//! Southstar text — the font-table queries layout makes of a font, answered by HarfBuzz or, for Rust-backed fonts, by read-fonts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#[cfg(feature = "rust-shaper")]
use std::sync::Arc;

use crate::ffi::harfbuzz::FontExtents;
use crate::font::Font;
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
        _ => RustFonts::All,
    })
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
