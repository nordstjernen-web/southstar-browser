//! Southstar — drawing southstar-text layouts, lines and glyph ranges onto a cairo context.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ops::Range;

use southstar_cairo::ContextRef;
use southstar_text::SCALE_F;
use southstar_text::font::Font;
use southstar_text::glyph::{GlyphInfo, GlyphString};
use southstar_text::layout::{Layout, LineRef};

pub fn show_layout(_cr: ContextRef, _layout: &mut Layout) {}

pub fn show_layout_line(_cr: ContextRef, _line: LineRef<'_>) {}

fn glyph_string_path(_cr: ContextRef, _font: Option<&Font>, _glyphs: &GlyphString) {}

fn glyph_path(cr: ContextRef, font: Option<&Font>, info: &GlyphInfo, x: f64, baseline: f64) {
    let one = GlyphString {
        glyphs: vec![info.clone()],
        log_clusters: vec![0],
    };
    cr.move_to(x, baseline);
    glyph_string_path(cr, font, &one);
}

pub fn line_range_path(
    cr: ContextRef,
    line: LineRef<'_>,
    x: f64,
    baseline: f64,
    range: Range<c_int>,
) {
    let mut run_x = x;
    for run in &line.runs {
        let item = &run.item;
        let glyphs = &run.glyphs;
        let start_x = run_x + f64::from(run.start_x_offset) / SCALE_F;
        let item_range = item.offset..item.offset + item.length;
        let font = item.analysis.font.as_ref();
        if item_range.start >= range.start && item_range.end <= range.end {
            cr.move_to(start_x, baseline);
            glyph_string_path(cr, font, glyphs);
        } else if item_range.start < range.end && item_range.end > range.start {
            let mut glyph_x = start_x;
            for (info, cluster) in glyphs.glyphs.iter().zip(&glyphs.log_clusters) {
                if range.contains(&(item.offset + cluster)) {
                    glyph_path(cr, font, info, glyph_x, baseline);
                }
                glyph_x += f64::from(info.geometry.width) / SCALE_F;
            }
        }
        let width: c_int = glyphs.glyphs.iter().map(|g| g.geometry.width).sum();
        run_x += f64::from(run.start_x_offset + width + run.end_x_offset) / SCALE_F;
    }
}
