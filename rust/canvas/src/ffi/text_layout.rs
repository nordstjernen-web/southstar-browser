//! Southstar — the southstar-text calls behind canvas text: a layout on a cairo context, its extents and the font's metrics.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::c_int;
use std::sync::PoisonError;

use southstar_text::Rectangle;
use southstar_text::context::Context as TextContext;
use southstar_text::font::CairoContext;
use southstar_text::font_description::FontDescription as TextFontDescription;
use southstar_text::fontmap::FontMap;
use southstar_text::layout::{Layout as TextLayout, shared_context};

use super::cairo::Context;

pub const PANGO_SCALE: f64 = 1024.0;

pub struct FontDescription(TextFontDescription);

impl FontDescription {
    pub fn new(families: &[u8], size_px: f64) -> FontDescription {
        let families = if families.contains(&0) {
            &[][..]
        } else {
            families
        };
        let mut desc = TextFontDescription::from_string(&String::from_utf8_lossy(families));
        let size = southstar_text::font_size_units(size_px);
        desc.set_absolute_size(f64::from(size));
        FontDescription(desc)
    }
}

pub struct Layout(RefCell<TextLayout>);

pub struct Extents {
    pub ink: Rectangle,
    pub logical: Rectangle,
    pub baseline: c_int,
}

fn text_cairo(cr: Context) -> Option<CairoContext> {
    // SAFETY: a canvas Context wraps the canvas's live cairo_t.
    unsafe { CairoContext::from_raw(cr.raw().cast()) }
}

impl Layout {
    pub fn new(cr: Context, desc: &FontDescription, text: &[u8]) -> Layout {
        let context = match text_cairo(cr) {
            Some(cr) => TextContext::create_for_cairo(&cr),
            None => FontMap::default_for_thread().create_context(),
        };
        let mut layout = TextLayout::new(&shared_context(context));
        layout.set_font_description(Some(&desc.0));
        let text = if text.contains(&0) { &[][..] } else { text };
        layout.set_text(&String::from_utf8_lossy(text));
        Layout(RefCell::new(layout))
    }

    pub fn extents(&self) -> Extents {
        let mut layout = self.0.borrow_mut();
        let (ink, logical) = layout.extents();
        Extents {
            ink,
            logical,
            baseline: layout.baseline(),
        }
    }

    pub fn font_ascent_descent(&self, desc: &FontDescription) -> Option<(c_int, c_int)> {
        let layout = self.0.borrow();
        let mut context = layout
            .context()
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let metrics = context.metrics(Some(&desc.0), None);
        Some((metrics.ascent(), metrics.descent()))
    }

    pub fn show(&self, cr: Context) {
        if let Some(cr) = text_cairo(cr) {
            southstar_text::render::show_layout(&cr, &mut self.0.borrow_mut());
        }
    }

    pub fn path(&self, cr: Context) {
        if let Some(cr) = text_cairo(cr) {
            southstar_text::render::layout_path(&cr, &mut self.0.borrow_mut());
        }
    }
}
