//! Southstar text — the cairo side of the text renderer: glyph runs with their text clusters, hex boxes for missing glyphs, filled rectangles, trapezoids and error squiggles.
//! Copyright 2004 Red Hat, Inc.; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::ffi::CString;

use super::{CairoRenderer, render_component};
use crate::ffi::cairo::{CairoGlyph, TextCluster};
use crate::font::{CairoContext, Font, UNKNOWN_GLYPH_HEIGHT, get_ignorable_size};
use crate::glyph::{GlyphInfo, GlyphString};
use crate::glyph_item::{GlyphItem, GlyphItemIter};
use crate::types::{GLYPH_EMPTY, GLYPH_INVALID_INPUT, GLYPH_UNKNOWN_FLAG, RenderPart, SCALE_F};
use crate::unicode::{UnicodeType, unichar_type};

const HEIGHT_SQUARES: f64 = 2.5;

fn units_to_user(value: i32) -> f64 {
    f64::from(value) / SCALE_F
}

fn glyph_string_has_unknown_glyphs(glyphs: &GlyphString) -> bool {
    glyphs
        .glyphs
        .iter()
        .any(|info| info.glyph & GLYPH_UNKNOWN_FLAG != 0)
}

fn glyph_walk(glyph_start: i32, glyph_end: i32) -> impl Iterator<Item = i32> {
    let dir = if glyph_start < glyph_end { 1 } else { -1 };
    let mut i = glyph_start;
    core::iter::from_fn(move || {
        if i == glyph_end {
            return None;
        }
        let current = i;
        i += dir;
        Some(current)
    })
}

fn glyph_at(glyphs: &GlyphString, index: i32) -> Option<&GlyphInfo> {
    usize::try_from(index)
        .ok()
        .and_then(|index| glyphs.glyphs.get(index))
}

fn glyph_range_has_unknown_glyphs(glyphs: &GlyphString, glyph_start: i32, glyph_end: i32) -> bool {
    glyph_walk(glyph_start, glyph_end)
        .filter_map(|i| glyph_at(glyphs, i))
        .any(|info| info.glyph & GLYPH_UNKNOWN_FLAG != 0)
}

fn count_glyphs(
    glyphs: &GlyphString,
    glyph_start: i32,
    glyph_end: i32,
    use_hex_box_scaled_font: bool,
) -> i32 {
    glyph_walk(glyph_start, glyph_end)
        .filter_map(|i| glyph_at(glyphs, i))
        .filter(|info| {
            info.glyph != GLYPH_EMPTY
                && (use_hex_box_scaled_font || info.glyph & GLYPH_UNKNOWN_FLAG == 0)
        })
        .count() as i32
}

fn glyph_range(glyph_start: i32, glyph_end: i32) -> (i32, i32) {
    if glyph_start < glyph_end {
        (glyph_start, glyph_end)
    } else {
        (glyph_end + 1, glyph_start + 1)
    }
}

fn byte_range(text: &[u8], start: i32, end: i32) -> &[u8] {
    let start = usize::try_from(start).unwrap_or(0);
    let end = usize::try_from(end).unwrap_or(0);
    text.get(start..end).unwrap_or(&[])
}

pub fn draw_error_underline_path(cr: &CairoContext, x: f64, y: f64, width: f64, height: f64) {
    let square = height / HEIGHT_SQUARES;
    let unit_width = (HEIGHT_SQUARES - 1.) * square;
    let double_width = 2. * unit_width;
    let width_units = ((width + unit_width / 2.) / unit_width) as i32;

    let x = x + (width - f64::from(width_units) * unit_width) / 2.;
    let y_top = y;
    let y_bottom = y + height;

    let mut x_middle = x + unit_width;
    let mut x_right = x + double_width;
    cr.move_to(x - square / 2., y_top + square / 2.);
    let mut i = 0;
    while i < width_units - 2 {
        cr.line_to(x_middle, y_bottom);
        cr.line_to(x_right, y_top + square);
        x_middle += double_width;
        x_right += double_width;
        i += 2;
    }
    cr.line_to(x_middle, y_bottom);

    if i + 1 == width_units {
        cr.line_to(x_middle + square / 2., y_bottom - square / 2.);
    } else if i + 2 == width_units {
        cr.line_to(x_right + square / 2., y_top + square / 2.);
        cr.line_to(x_right, y_top);
    }

    let mut x_left = x_middle - unit_width;
    while i >= 0 {
        cr.line_to(x_middle, y_bottom - square);
        cr.line_to(x_left, y_top);
        x_left -= double_width;
        x_middle -= double_width;
        i -= 2;
    }
}

fn inner_triangle_offset(half_line_width: f64, angle_tangent: f64, room: f64) -> Option<f64> {
    if angle_tangent < 1e-5 {
        return None;
    }
    let offset = half_line_width / angle_tangent;
    if 2. * offset > room {
        return None;
    }
    Some(offset)
}

impl CairoRenderer<'_> {
    pub(super) fn apply_part_color(&self, part: RenderPart) {
        let color = self.color[part as usize];
        let a = self.alpha[part as usize];

        if a == 0 && color.is_none() {
            return;
        }

        let (red, green, blue, mut alpha) = match color {
            Some(color) => (
                f64::from(color.red) / 65535.,
                f64::from(color.green) / 65535.,
                f64::from(color.blue) / 65535.,
                1.,
            ),
            None => self.cr.solid_source_rgba().unwrap_or((0., 0., 0., 1.)),
        };

        if a != 0 {
            alpha = f64::from(a) / 65535.;
        }

        self.cr.set_source_rgba(red, green, blue, alpha);
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_frame(&self, x: f64, y: f64, width: f64, height: f64, line_width: f64, invalid: bool) {
        let cr = self.cr;

        if !self.do_path {
            cr.rectangle(x, y, width, height);
            if invalid {
                cr.new_sub_path();
                cr.move_to(x, y);
                cr.rel_line_to(width, height);
                cr.new_sub_path();
                cr.move_to(x + width, y);
                cr.rel_line_to(-width, height);
                cr.set_line_cap_butt();
            }
            cr.set_line_width(line_width);
            cr.set_line_join_miter();
            cr.set_miter_limit(2.);
            cr.stroke();
            return;
        }

        let d2 = line_width * 0.5;
        let d = line_width;

        cr.rectangle(x - d2, y - d2, width + d, height + d);

        if !invalid {
            cr.rectangle(x + width - d2, y + d2, -(width - d), height - d);
            return;
        }

        let mut alpha = height.atan2(width);

        let sx =
            inner_triangle_offset(d2, (alpha * 0.5).tan(), width - d).unwrap_or((width - d) * 0.5);
        let cos_alpha = alpha.cos();
        let sy = inner_triangle_offset(d2, cos_alpha, height - d).unwrap_or((height - d) * 0.5);

        cr.new_sub_path();
        cr.line_to(x + width - sx, y + d2);
        cr.line_to(x + sx, y + d2);
        cr.line_to(x + 0.5 * width, y + 0.5 * height - sy);
        cr.close_path();

        cr.new_sub_path();
        cr.line_to(x + width - sx, y + height - d2);
        cr.line_to(x + 0.5 * width, y + 0.5 * height + sy);
        cr.line_to(x + sx, y + height - d2);
        cr.close_path();

        alpha = core::f64::consts::FRAC_PI_2 - alpha;
        let sy = inner_triangle_offset(d2, (alpha * 0.5).tan(), height - d)
            .unwrap_or((height - d) * 0.5);
        let cos_alpha = alpha.cos();
        let sx = inner_triangle_offset(d2, cos_alpha, width - d).unwrap_or((width - d) * 0.5);

        cr.new_sub_path();
        cr.line_to(x + d2, y + sy);
        cr.line_to(x + d2, y + height - sy);
        cr.line_to(x + 0.5 * width - sx, y + 0.5 * height);
        cr.close_path();

        cr.new_sub_path();
        cr.line_to(x + width - d2, y + sy);
        cr.line_to(x + 0.5 * width + sx, y + 0.5 * height);
        cr.line_to(x + width - d2, y + height - sy);
        cr.close_path();
    }

    fn draw_box_glyph(&self, gi: &GlyphInfo, cx: f64, cy: f64, invalid: bool) {
        self.cr.save();
        let height = f64::from(UNKNOWN_GLYPH_HEIGHT);
        self.draw_frame(
            cx + 1.5,
            cy + 1.5 - height,
            units_to_user(gi.geometry.width) - 3.0,
            height - 3.0,
            1.0,
            invalid,
        );
        self.cr.restore();
    }

    fn draw_unknown_glyph(&self, font: Option<&Font>, gi: &GlyphInfo, cx: f64, cy: f64) {
        let cr = self.cr;
        cr.save();
        self.draw_unknown_glyph_saved(font, gi, cx, cy);
        cr.restore();
    }

    fn draw_unknown_glyph_saved(&self, font: Option<&Font>, gi: &GlyphInfo, cx: f64, cy: f64) {
        let cr = self.cr;
        let ch = gi.glyph & !GLYPH_UNKNOWN_FLAG;
        let invalid_input = gi.glyph == GLYPH_INVALID_INPUT || ch > 0x10FFFF;
        let glyph_width = units_to_user(gi.geometry.width);

        let hbi = match font.and_then(Font::hex_box_info) {
            Some(hbi) if hbi.font.install(cr) => hbi,
            _ => {
                self.draw_box_glyph(gi, cx, cy, invalid_input);
                return;
            }
        };

        let rows;
        let cols;
        let name: String;

        if invalid_input {
            rows = hbi.rows;
            cols = 1;
            name = String::new();
        } else if ch == 0x2423 || unichar_type(ch) == UnicodeType::SpaceSeparator {
            let x = cx + 0.5 * glyph_width;
            let y = cy + hbi.box_descent - 0.5 * hbi.box_height;
            cr.new_sub_path();
            cr.arc(x, y, 1.5 * hbi.line_width, 0., 2. * core::f64::consts::PI);
            cr.close_path();
            cr.fill();
            return;
        } else if ch == u32::from('\t') {
            let y = cy + hbi.box_descent - 0.5 * hbi.box_height;
            let offset = 0.2 * glyph_width;
            let x = cx + offset;
            let al = glyph_width - 2. * offset;
            let tl = if hbi.digit_width < 0.75 * al {
                hbi.digit_width
            } else {
                0.75 * al
            };
            let tw2 = 2.5 * hbi.line_width;
            let lw2 = 0.5 * hbi.line_width;
            cr.move_to(x - lw2, y - tw2);
            cr.line_to(x + lw2, y - tw2);
            cr.line_to(x + lw2, y - lw2);
            cr.line_to(x + al - tl, y - lw2);
            cr.line_to(x + al - tl, y - tw2);
            cr.line_to(x + al, y);
            cr.line_to(x + al - tl, y + tw2);
            cr.line_to(x + al - tl, y + lw2);
            cr.line_to(x + lw2, y + lw2);
            cr.line_to(x + lw2, y + tw2);
            cr.line_to(x - lw2, y + tw2);
            cr.close_path();
            cr.fill();
            return;
        } else if ch == u32::from('\n') || ch == 0x2028 || ch == 0x2029 {
            let offset = 0.2 * glyph_width;
            let al = glyph_width - 2. * offset;
            let tl = if hbi.digit_width < 0.75 * al {
                hbi.digit_width
            } else {
                0.75 * al
            };
            let ah = al - 0.5 * tl;
            let tw2 = 2.5 * hbi.line_width;
            let x = cx + offset;
            let y = cy - (hbi.box_height - al) / 2.;
            let lw2 = 0.5 * hbi.line_width;
            cr.move_to(x, y);
            cr.line_to(x + tl, y - tw2);
            cr.line_to(x + tl, y - lw2);
            cr.line_to(x + al - lw2, y - lw2);
            cr.line_to(x + al - lw2, y - ah);
            cr.line_to(x + al + lw2, y - ah);
            cr.line_to(x + al + lw2, y + lw2);
            cr.line_to(x + tl, y + lw2);
            cr.line_to(x + tl, y + tw2);
            cr.close_path();
            cr.fill();
            return;
        } else if let Some((nick, nick_rows, nick_cols)) = get_ignorable_size(ch) {
            rows = nick_rows;
            cols = nick_cols;
            name = nick.to_owned();
        } else {
            rows = hbi.rows;
            cols = (if ch > 0xffff { 6 } else { 4 }) / rows;
            name = if ch > 0xffff {
                format!("{ch:06X}")
            } else {
                format!("{ch:04X}")
            };
        }

        let width = 3. * hbi.pad_x + f64::from(cols) * (hbi.digit_width + hbi.pad_x);
        let mut lsb = (glyph_width - width) * 0.5;
        lsb = (lsb / hbi.pad_x).floor() * hbi.pad_x;

        self.draw_frame(
            cx + lsb + 1.5 * hbi.pad_x,
            cy + hbi.box_descent - hbi.box_height + hbi.pad_y * 0.5,
            width - hbi.pad_x,
            hbi.box_height - hbi.pad_y,
            hbi.line_width,
            invalid_input,
        );

        if invalid_input {
            return;
        }

        let x0 = cx + lsb + hbi.pad_x * 3.;
        let y0 = cy + hbi.box_descent
            - hbi.pad_y * 2.
            - (f64::from(hbi.rows - rows) * hbi.digit_height / 2.);

        let mut digits = name.bytes();
        for row in 0..rows {
            let y = y0 - f64::from(rows - 1 - row) * (hbi.digit_height + hbi.pad_y);
            for col in 0..cols {
                let x = x0 + f64::from(col) * (hbi.digit_width + hbi.pad_x);
                cr.move_to(x, y);
                let digit = digits.next().filter(|&byte| byte != 0);
                let text = CString::new(digit.map(|byte| vec![byte]).unwrap_or_default())
                    .unwrap_or_default();
                if self.do_path {
                    cr.text_path(&text);
                } else {
                    cr.show_text(&text);
                }
            }
        }
    }

    fn has_hex_box_scaled_font(&self, font: Option<&Font>, glyphs: &GlyphString) -> bool {
        !self.do_path
            && self.has_show_text_glyphs
            && glyph_string_has_unknown_glyphs(glyphs)
            && font.and_then(Font::hex_box_scaled_font).is_some()
    }

    #[allow(clippy::too_many_arguments)]
    fn show_text_glyphs_range(
        &self,
        text: &[u8],
        glyphs: &GlyphString,
        glyph_start: i32,
        glyph_end: i32,
        clusters: Option<&[TextCluster]>,
        backward: bool,
        font: Option<&Font>,
        x: i32,
        y: i32,
        use_hex_box_scaled_font: bool,
    ) {
        let cr = self.cr;
        let base_x = self.x_offset + units_to_user(x);
        let base_y = self.y_offset + units_to_user(y);
        let components = self.components;
        let start = usize::try_from(glyph_start)
            .unwrap_or(0)
            .min(glyphs.glyphs.len());
        let end = usize::try_from(glyph_end)
            .unwrap_or(0)
            .min(glyphs.glyphs.len());

        cr.save();
        if !self.do_path {
            self.apply_part_color(RenderPart::Foreground);
        }

        let mut x_position = glyphs.glyphs[..start].iter().fold(0i32, |position, info| {
            position.wrapping_add(info.geometry.width)
        });

        let glyph_origin = |x_position: i32, gi: &GlyphInfo| {
            let cx = base_x + units_to_user(x_position + gi.geometry.x_offset);
            let cy = if gi.geometry.y_offset == 0 {
                base_y
            } else {
                base_y + units_to_user(gi.geometry.y_offset)
            };
            (cx, cy)
        };

        let hex_box_scaled_font = if use_hex_box_scaled_font {
            font.and_then(Font::hex_box_scaled_font)
        } else {
            None
        };

        if let Some(scaled_font) = hex_box_scaled_font.as_ref() {
            cr.set_scaled_font(scaled_font);
        } else if !font.is_some_and(|font| font.install(cr)) {
            for gi in glyphs.glyphs.get(start..end).unwrap_or(&[]) {
                if gi.glyph != GLYPH_EMPTY {
                    let (cx, cy) = glyph_origin(x_position, gi);
                    self.draw_unknown_glyph(font, gi, cx, cy);
                }
                x_position = x_position.wrapping_add(gi.geometry.width);
            }
            cr.restore();
            return;
        }

        let mut cairo_glyphs: Vec<CairoGlyph> = Vec::with_capacity(end.saturating_sub(start));
        for gi in glyphs.glyphs.get(start..end).unwrap_or(&[]) {
            let wanted = if gi.attr.is_color {
                render_component::COLOR_GLYPH
            } else {
                render_component::PLAIN_GLYPH
            };
            if components & wanted != 0 && gi.glyph != GLYPH_EMPTY {
                let (cx, cy) = glyph_origin(x_position, gi);
                if gi.glyph & GLYPH_UNKNOWN_FLAG != 0 {
                    if use_hex_box_scaled_font {
                        let index = font.map_or(u64::from(gi.glyph), |font| {
                            font.encode_hex_box_glyph(gi.glyph)
                        });
                        cairo_glyphs.push(CairoGlyph {
                            index: index as _,
                            x: cx,
                            y: cy,
                        });
                    } else if gi.glyph != (0x20 | GLYPH_UNKNOWN_FLAG) {
                        self.draw_unknown_glyph(font, gi, cx, cy);
                    }
                } else {
                    cairo_glyphs.push(CairoGlyph {
                        index: gi.glyph as _,
                        x: cx,
                        y: cy,
                    });
                }
            }
            x_position = x_position.wrapping_add(gi.geometry.width);
        }

        if self.do_path {
            cr.glyphs_path(&cairo_glyphs);
        } else if let Some(clusters) = clusters {
            cr.show_text_glyphs(text, &cairo_glyphs, clusters, backward);
        } else {
            cr.show_glyphs(&cairo_glyphs);
        }

        cr.restore();
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn show_text_glyphs(
        &self,
        text: Option<&[u8]>,
        glyphs: &GlyphString,
        clusters: Option<&[TextCluster]>,
        backward: bool,
        font: Option<&Font>,
        x: i32,
        y: i32,
    ) {
        self.show_text_glyphs_range(
            text.unwrap_or(&[]),
            glyphs,
            0,
            glyphs.glyphs.len() as i32,
            clusters,
            backward,
            font,
            x,
            y,
            false,
        );
    }

    pub(super) fn show_glyph_item_clusters(
        &self,
        text: &str,
        glyph_item: &GlyphItem,
        x: i32,
        y: i32,
    ) {
        let font = glyph_item.item.analysis.font.as_ref();
        let glyphs = &glyph_item.glyphs;
        let backward = glyph_item.item.analysis.level & 1 != 0;

        if !self.has_show_text_glyphs || self.do_path {
            self.show_text_glyphs(None, glyphs, None, false, font, x, y);
            return;
        }

        let use_hex_box_scaled_font = self.has_hex_box_scaled_font(font, glyphs);
        let bytes = text.as_bytes();
        let mut clusters: Vec<TextCluster> = Vec::with_capacity(glyphs.glyphs.len());
        let mut segment: Option<(bool, i32, i32)> = None;
        let mut segment_end_index = 0;
        let mut segment_end_glyph = 0;

        let flush = |clusters: &mut Vec<TextCluster>,
                     (use_hex_box, start_index, start_glyph): (bool, i32, i32),
                     end_index: i32,
                     end_glyph: i32| {
            let (range_start, range_end) = glyph_range(start_glyph, end_glyph);
            self.show_text_glyphs_range(
                byte_range(bytes, start_index, end_index),
                glyphs,
                range_start,
                range_end,
                Some(clusters),
                backward,
                font,
                x,
                y,
                use_hex_box,
            );
            clusters.clear();
        };

        let mut iter = GlyphItemIter::default();
        if iter.init_start(glyph_item, text) {
            loop {
                let num_bytes = iter.end_index - iter.start_index;
                let cluster_use_hex_box = use_hex_box_scaled_font
                    && glyph_range_has_unknown_glyphs(glyphs, iter.start_glyph, iter.end_glyph);
                let num_glyphs = count_glyphs(
                    glyphs,
                    iter.start_glyph,
                    iter.end_glyph,
                    cluster_use_hex_box,
                );

                if let Some(open) = segment
                    && open.0 != cluster_use_hex_box
                {
                    flush(&mut clusters, open, segment_end_index, segment_end_glyph);
                    segment = None;
                }

                if segment.is_none() {
                    segment = Some((cluster_use_hex_box, iter.start_index, iter.start_glyph));
                }

                segment_end_index = iter.end_index;
                segment_end_glyph = iter.end_glyph;
                clusters.push(TextCluster {
                    num_bytes,
                    num_glyphs,
                });

                if !iter.next_cluster(glyph_item, text) {
                    break;
                }
            }
        }

        if let Some(open) = segment {
            flush(&mut clusters, open, segment_end_index, segment_end_glyph);
        }
    }

    pub(super) fn fill_rectangle(&self, part: RenderPart, x: i32, y: i32, width: i32, height: i32) {
        let cr = self.cr;
        if !self.do_path {
            cr.save();
            self.apply_part_color(part);
        }

        cr.rectangle(
            self.x_offset + units_to_user(x),
            self.y_offset + units_to_user(y),
            units_to_user(width),
            units_to_user(height),
        );

        if !self.do_path {
            cr.fill();
            cr.restore();
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn fill_trapezoid(
        &self,
        part: RenderPart,
        y1: f64,
        x11: f64,
        x21: f64,
        y2: f64,
        x12: f64,
        x22: f64,
    ) {
        let cr = self.cr;
        cr.save();

        if !self.do_path {
            self.apply_part_color(part);
        }

        let (x, y) = cr.user_to_device_distance(self.x_offset, self.y_offset);
        cr.identity_matrix();
        cr.translate(x, y);

        cr.move_to(x11, y1);
        cr.line_to(x21, y1);
        cr.line_to(x22, y2);
        cr.line_to(x12, y2);
        cr.close_path();

        if !self.do_path {
            cr.fill();
        }

        cr.restore();
    }

    pub(super) fn fill_error_underline(&self, x: i32, y: i32, width: i32, height: i32) {
        let cr = self.cr;
        if !self.do_path {
            cr.save();
            self.apply_part_color(RenderPart::Underline);
            cr.new_path();
        }

        draw_error_underline_path(
            cr,
            self.x_offset + units_to_user(x),
            self.y_offset + units_to_user(y),
            units_to_user(width),
            units_to_user(height),
        );

        if !self.do_path {
            cr.fill();
            cr.restore();
        }
    }
}
