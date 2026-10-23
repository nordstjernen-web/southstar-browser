//! Southstar text — the cairo text renderer: part colors from run attributes, backgrounds, underlines, overlines and strikethroughs gathered across a line.
//! Copyright 2004 Red Hat, Inc.; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

mod glyphs;
mod layout_entry;

use crate::attrs::{AttrValue, ShapeAttr};
use crate::font::{CairoContext, Font, metrics_of};
use crate::glyph::GlyphString;
use crate::glyph_item::GlyphItem;
use crate::item::{Item, analysis_flags};
use crate::metrics::FontMetrics;
use crate::types::{Color, Overline, Rectangle, RenderPart, SCALE, Underline, units_round};

pub use glyphs::draw_error_underline_path;
pub use layout_entry::{layout_line_path, layout_path, show_layout, show_layout_line};

#[cfg(test)]
#[path = "tests/render.rs"]
mod tests;

pub mod render_component {
    pub const NONE: u32 = 0;
    pub const PLAIN_GLYPH: u32 = 1 << 1;
    pub const COLOR_GLYPH: u32 = 1 << 2;
    pub const BACKGROUND: u32 = 1 << 3;
    pub const UNDERLINE: u32 = 1 << 3;
    pub const STRIKETHROUGH: u32 = 1 << 4;
    pub const OVERLINE: u32 = 1 << 5;
    pub const ALL: u32 =
        PLAIN_GLYPH | COLOR_GLYPH | BACKGROUND | UNDERLINE | STRIKETHROUGH | OVERLINE;
}

const N_RENDER_PARTS: usize = 5;
const UNDERLINE_NONE: i32 = Underline::None as i32;
const OVERLINE_NONE: i32 = Overline::None as i32;

#[derive(Clone, Copy, Debug, Default)]
struct LineState {
    underline: i32,
    underline_rect: Rectangle,
    strikethrough: bool,
    strikethrough_rect: Rectangle,
    strikethrough_glyphs: i32,
    overline: i32,
    overline_rect: Rectangle,
    logical_rect_end: i32,
}

pub struct CairoRenderer<'a> {
    cr: &'a CairoContext,
    do_path: bool,
    has_show_text_glyphs: bool,
    x_offset: f64,
    y_offset: f64,
    cr_had_current_point: bool,
    underline: i32,
    strikethrough: bool,
    overline: i32,
    active_count: i32,
    color: [Option<Color>; N_RENDER_PARTS],
    alpha: [u16; N_RENDER_PARTS],
    components: u32,
    line_state: Option<LineState>,
}

pub fn shape_get_extents(
    n_chars: i32,
    shape_ink: &Rectangle,
    shape_logical: &Rectangle,
) -> (Rectangle, Rectangle) {
    if n_chars <= 0 {
        return (Rectangle::default(), Rectangle::default());
    }
    let repeat = shape_logical.width * (n_chars - 1);
    let ink = Rectangle {
        x: shape_ink.x.min(shape_ink.x + repeat),
        y: shape_ink.y,
        width: shape_ink.width.max(shape_ink.width + repeat),
        height: shape_ink.height,
    };
    let logical = Rectangle {
        x: shape_logical.x.min(shape_logical.x + repeat),
        y: shape_logical.y,
        width: shape_logical.width.max(shape_logical.width + repeat),
        height: shape_logical.height,
    };
    (ink, logical)
}

fn part_to_component(part: RenderPart) -> u32 {
    match part {
        RenderPart::Foreground => render_component::PLAIN_GLYPH | render_component::COLOR_GLYPH,
        RenderPart::Background => render_component::BACKGROUND,
        RenderPart::Underline => render_component::UNDERLINE,
        RenderPart::Strikethrough => render_component::STRIKETHROUGH,
        RenderPart::Overline => render_component::OVERLINE,
    }
}

fn shape_attr_of(item: &Item) -> Option<ShapeAttr> {
    item.analysis
        .extra_attrs
        .iter()
        .rev()
        .find_map(|attr| match &attr.value {
            AttrValue::Shape(shape) => Some(*shape),
            _ => None,
        })
}

impl<'a> CairoRenderer<'a> {
    pub fn new(cr: &'a CairoContext, do_path: bool) -> CairoRenderer<'a> {
        CairoRenderer {
            cr,
            do_path,
            has_show_text_glyphs: false,
            x_offset: 0.,
            y_offset: 0.,
            cr_had_current_point: false,
            underline: UNDERLINE_NONE,
            strikethrough: false,
            overline: OVERLINE_NONE,
            active_count: 0,
            color: [None; N_RENDER_PARTS],
            alpha: [0; N_RENDER_PARTS],
            components: render_component::ALL,
            line_state: None,
        }
    }

    pub fn at_current_point(cr: &'a CairoContext, do_path: bool) -> CairoRenderer<'a> {
        let mut renderer = CairoRenderer::new(cr, do_path);
        renderer.cr_had_current_point = cr.has_current_point();
        (renderer.x_offset, renderer.y_offset) = cr.current_point();
        renderer.has_show_text_glyphs = cr.target_has_show_text_glyphs();
        renderer
    }

    pub fn restore_current_point(&self) {
        if self.cr_had_current_point {
            self.cr.move_to(self.x_offset, self.y_offset);
        } else {
            self.cr.new_sub_path();
        }
    }

    pub fn set_offset(&mut self, x_offset: f64, y_offset: f64) {
        self.x_offset = x_offset;
        self.y_offset = y_offset;
    }

    pub fn set_has_show_text_glyphs(&mut self, has_show_text_glyphs: bool) {
        self.has_show_text_glyphs = has_show_text_glyphs;
    }

    pub fn activate(&mut self) {
        self.active_count += 1;
    }

    pub fn deactivate(&mut self) {
        if self.active_count > 0 {
            self.active_count -= 1;
        }
    }

    pub fn set_components(&mut self, components: u32) {
        self.components = components;
    }

    pub fn components(&self) -> u32 {
        self.components
    }

    fn should_draw_component(&self, components: u32) -> bool {
        self.components & components != 0
    }

    fn should_draw_part(&self, part: RenderPart) -> bool {
        self.should_draw_component(part_to_component(part))
    }

    pub fn color(&self, part: RenderPart) -> Option<Color> {
        self.color[part as usize]
    }

    pub fn alpha(&self, part: RenderPart) -> u16 {
        self.alpha[part as usize]
    }

    pub fn set_color(&mut self, part: RenderPart, color: Option<&Color>) {
        let current = self.color[part as usize];
        let unchanged = match (color, current) {
            (None, None) => true,
            (Some(new), Some(old)) => {
                new.red == old.red && new.green == old.green && new.blue == old.blue
            }
            _ => false,
        };
        if unchanged {
            return;
        }
        self.part_changed(part);
        self.color[part as usize] = color.copied();
    }

    pub fn set_alpha(&mut self, part: RenderPart, alpha: u16) {
        if self.alpha[part as usize] == alpha {
            return;
        }
        self.part_changed(part);
        self.alpha[part as usize] = alpha;
    }

    pub fn part_changed(&mut self, part: RenderPart) {
        if self.active_count <= 0 {
            return;
        }
        self.handle_line_state_change(part);
    }

    fn handle_line_state_change(&mut self, part: RenderPart) {
        let Some(mut state) = self.line_state.take() else {
            return;
        };

        if part == RenderPart::Underline && state.underline != UNDERLINE_NONE {
            state.underline_rect.width = state.logical_rect_end - state.underline_rect.x;
            self.draw_underline(&mut state);
            state.underline = self.underline;
            state.underline_rect.x = state.logical_rect_end;
            state.underline_rect.width = 0;
        }

        if part == RenderPart::Overline && state.overline != OVERLINE_NONE {
            state.overline_rect.width = state.logical_rect_end - state.overline_rect.x;
            self.draw_overline(&mut state);
            state.overline = self.overline;
            state.overline_rect.x = state.logical_rect_end;
            state.overline_rect.width = 0;
        }

        if part == RenderPart::Strikethrough && state.strikethrough {
            state.strikethrough_rect.width = state.logical_rect_end - state.strikethrough_rect.x;
            self.draw_strikethrough(&mut state);
            state.strikethrough = self.strikethrough;
        }

        self.line_state = Some(state);
    }

    fn draw_underline(&mut self, state: &mut LineState) {
        let rect = state.underline_rect;
        let underline = Underline::from_i32(state.underline);
        state.underline = UNDERLINE_NONE;

        match underline {
            Some(Underline::Double | Underline::DoubleLine) => {
                self.draw_rectangle(
                    RenderPart::Underline,
                    rect.x,
                    rect.y + 2 * rect.height,
                    rect.width,
                    rect.height,
                );
                self.draw_rectangle(
                    RenderPart::Underline,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                );
            }
            Some(Underline::Single | Underline::Low | Underline::SingleLine) => {
                self.draw_rectangle(
                    RenderPart::Underline,
                    rect.x,
                    rect.y,
                    rect.width,
                    rect.height,
                );
            }
            Some(Underline::Error | Underline::ErrorLine) => {
                self.draw_error_underline(rect.x, rect.y, rect.width, 3 * rect.height);
            }
            Some(Underline::None) | None => {}
        }
    }

    fn draw_overline(&mut self, state: &mut LineState) {
        let rect = state.overline_rect;
        let overline = state.overline;
        state.overline = OVERLINE_NONE;

        if overline == Overline::Single as i32 {
            self.draw_rectangle(
                RenderPart::Overline,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
            );
        }
    }

    fn draw_strikethrough(&mut self, state: &mut LineState) {
        let rect = state.strikethrough_rect;
        let num_glyphs = state.strikethrough_glyphs;

        if state.strikethrough && num_glyphs > 0 {
            self.draw_rectangle(
                RenderPart::Strikethrough,
                rect.x,
                rect.y / num_glyphs,
                rect.width,
                rect.height / num_glyphs,
            );
        }

        state.strikethrough = false;
        state.strikethrough_glyphs = 0;
        let rect = &mut state.strikethrough_rect;
        rect.x += rect.width;
        rect.width = 0;
        rect.y = 0;
        rect.height = 0;
    }

    fn add_underline(
        &mut self,
        state: &mut LineState,
        metrics: &FontMetrics,
        base_x: i32,
        base_y: i32,
        ink_rect: &Rectangle,
        logical_rect: &Rectangle,
    ) {
        let underline_thickness = metrics.underline_thickness();
        let underline_position = metrics.underline_position();

        let mut new_rect = Rectangle {
            x: base_x + ink_rect.x.min(logical_rect.x),
            y: base_y,
            width: ink_rect.width.max(logical_rect.width),
            height: underline_thickness,
        };

        match Underline::from_i32(self.underline) {
            Some(Underline::Single | Underline::Double | Underline::Error) => {
                new_rect.y -= underline_position;
            }
            Some(Underline::Low) => {
                new_rect.y += ink_rect.y + ink_rect.height + underline_thickness;
            }
            Some(Underline::SingleLine | Underline::DoubleLine | Underline::ErrorLine) => {
                new_rect.y -= underline_position;
                if state.underline == self.underline {
                    let current = &mut state.underline_rect;
                    new_rect.y = current.y.max(new_rect.y);
                    new_rect.height = current.height.max(new_rect.height);
                    current.y = new_rect.y;
                    current.height = new_rect.height;
                }
            }
            Some(Underline::None) | None => {}
        }

        let current = &mut state.underline_rect;
        if self.underline == state.underline
            && new_rect.y == current.y
            && new_rect.height == current.height
        {
            current.width = new_rect.x + new_rect.width - current.x;
        } else {
            self.draw_underline(state);
            state.underline_rect = new_rect;
            state.underline = self.underline;
        }
    }

    fn add_overline(
        &mut self,
        state: &mut LineState,
        metrics: &FontMetrics,
        base_x: i32,
        base_y: i32,
        ink_rect: &Rectangle,
        logical_rect: &Rectangle,
    ) {
        let underline_thickness = metrics.underline_thickness();
        let ascent = metrics.ascent();

        let mut new_rect = Rectangle {
            x: base_x + ink_rect.x.min(logical_rect.x),
            y: base_y,
            width: ink_rect.width.max(logical_rect.width),
            height: underline_thickness,
        };

        if self.overline == Overline::Single as i32 {
            new_rect.y -= ascent;
            if state.overline == self.overline {
                let current = &mut state.overline_rect;
                new_rect.y = current.y.min(new_rect.y);
                new_rect.height = current.height.max(new_rect.height);
                current.y = new_rect.y;
                current.height = new_rect.height;
            }
        }

        let current = &mut state.overline_rect;
        if self.overline == state.overline
            && new_rect.y == current.y
            && new_rect.height == current.height
        {
            current.width = new_rect.x + new_rect.width - current.x;
        } else {
            self.draw_overline(state);
            state.overline_rect = new_rect;
            state.overline = self.overline;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_strikethrough(
        &mut self,
        state: &mut LineState,
        metrics: &FontMetrics,
        base_x: i32,
        base_y: i32,
        ink_rect: &Rectangle,
        logical_rect: &Rectangle,
        num_glyphs: i32,
    ) {
        let strikethrough_thickness = metrics.strikethrough_thickness();
        let strikethrough_position = metrics.strikethrough_position();

        let new_rect = Rectangle {
            x: base_x + ink_rect.x.min(logical_rect.x),
            y: (base_y - strikethrough_position) * num_glyphs,
            width: ink_rect.width.max(logical_rect.width),
            height: strikethrough_thickness * num_glyphs,
        };

        if state.strikethrough {
            let current = &mut state.strikethrough_rect;
            current.width = new_rect.x + new_rect.width - current.x;
            current.y += new_rect.y;
            current.height += new_rect.height;
            state.strikethrough_glyphs += num_glyphs;
        } else {
            state.strikethrough_rect = new_rect;
            state.strikethrough = true;
            state.strikethrough_glyphs = num_glyphs;
        }
    }

    fn has_line_decorations(&self) -> bool {
        self.underline != UNDERLINE_NONE || self.overline != OVERLINE_NONE || self.strikethrough
    }

    pub fn prepare_run(&mut self, run: &GlyphItem) {
        let mut fg_color = None;
        let mut bg_color = None;
        let mut underline_color = None;
        let mut overline_color = None;
        let mut strikethrough_color = None;
        let mut fg_alpha = 0u16;
        let mut bg_alpha = 0u16;

        self.underline = UNDERLINE_NONE;
        self.overline = OVERLINE_NONE;
        self.strikethrough = false;

        for attr in &run.item.analysis.extra_attrs {
            match &attr.value {
                AttrValue::Underline(value) => self.underline = *value,
                AttrValue::Overline(value) => self.overline = *value,
                AttrValue::Strikethrough(value) => self.strikethrough = *value != 0,
                AttrValue::Foreground(color) => fg_color = Some(*color),
                AttrValue::Background(color) => bg_color = Some(*color),
                AttrValue::UnderlineColor(color) => underline_color = Some(*color),
                AttrValue::OverlineColor(color) => overline_color = Some(*color),
                AttrValue::StrikethroughColor(color) => strikethrough_color = Some(*color),
                AttrValue::ForegroundAlpha(value) => fg_alpha = *value as u16,
                AttrValue::BackgroundAlpha(value) => bg_alpha = *value as u16,
                _ => {}
            }
        }

        let underline_color = underline_color.or(fg_color);
        let overline_color = overline_color.or(fg_color);
        let strikethrough_color = strikethrough_color.or(fg_color);

        self.set_color(RenderPart::Foreground, fg_color.as_ref());
        self.set_color(RenderPart::Background, bg_color.as_ref());
        self.set_color(RenderPart::Underline, underline_color.as_ref());
        self.set_color(RenderPart::Strikethrough, strikethrough_color.as_ref());
        self.set_color(RenderPart::Overline, overline_color.as_ref());

        self.set_alpha(RenderPart::Foreground, fg_alpha);
        self.set_alpha(RenderPart::Background, bg_alpha);
        self.set_alpha(RenderPart::Underline, fg_alpha);
        self.set_alpha(RenderPart::Strikethrough, fg_alpha);
        self.set_alpha(RenderPart::Overline, fg_alpha);
    }

    pub fn draw_glyphs(&mut self, font: Option<&Font>, glyphs: &GlyphString, x: i32, y: i32) {
        self.activate();
        if self.should_draw_part(RenderPart::Foreground) {
            self.show_text_glyphs(None, glyphs, None, false, font, x, y);
        }
        self.deactivate();
    }

    pub fn draw_glyph_item(&mut self, text: Option<&str>, glyph_item: &GlyphItem, x: i32, y: i32) {
        let Some(text) = text else {
            self.draw_glyphs(
                glyph_item.item.analysis.font.as_ref(),
                &glyph_item.glyphs,
                x,
                y,
            );
            return;
        };

        if self.should_draw_part(RenderPart::Foreground) {
            self.activate();
            self.show_glyph_item_clusters(text, glyph_item, x, y);
            self.deactivate();
        }
    }

    pub fn draw_rectangle(&mut self, part: RenderPart, x: i32, y: i32, width: i32, height: i32) {
        if self.active_count <= 0 {
            return;
        }
        if self.should_draw_part(part) {
            self.fill_rectangle(part, x, y, width, height);
        }
    }

    pub fn draw_error_underline(&mut self, x: i32, y: i32, width: i32, height: i32) {
        if self.active_count <= 0 {
            return;
        }
        if self.should_draw_part(RenderPart::Underline) {
            self.fill_error_underline(x, y, width, height);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_trapezoid(
        &mut self,
        part: RenderPart,
        y1: f64,
        x11: f64,
        x21: f64,
        y2: f64,
        x12: f64,
        x22: f64,
    ) {
        if self.active_count <= 0 {
            return;
        }
        if self.should_draw_part(part) {
            self.fill_trapezoid(part, y1, x11, x21, y2, x12, x22);
        }
    }

    pub fn draw_line(
        &mut self,
        runs: &[GlyphItem],
        text: Option<&str>,
        x: i32,
        y: i32,
        mut line_logical_extents: impl FnMut() -> Rectangle,
    ) {
        let mut x_off = 0;
        let mut overall_rect: Option<Rectangle> = None;
        let mut state = LineState::default();

        self.activate();

        for run in runs {
            let font = run.item.analysis.font.as_ref();
            let centered_baseline =
                run.item.analysis.flags & analysis_flags::CENTERED_BASELINE != 0;

            self.line_state = Some(state);
            self.prepare_run(run);
            state = self.line_state.take().unwrap_or_default();

            let shape_attr = shape_attr_of(&run.item);
            let decorated = self.has_line_decorations();
            let ink: Option<Rectangle>;
            let logical: Option<Rectangle>;
            let glyph_string_width;

            if let Some(shape) = shape_attr {
                let (shape_ink, shape_logical) = shape_get_extents(
                    run.glyphs.glyphs.len() as i32,
                    &shape.ink_rect,
                    &shape.logical_rect,
                );
                ink = Some(shape_ink);
                logical = Some(shape_logical);
                glyph_string_width = shape_logical.width;
            } else if decorated || centered_baseline {
                let (glyphs_ink, glyphs_logical) = run.glyphs.extents(font);
                ink = decorated.then_some(glyphs_ink);
                logical = Some(glyphs_logical);
                glyph_string_width = glyphs_logical.width;
            } else {
                ink = None;
                logical = None;
                glyph_string_width = run.glyphs.get_width();
            }

            state.logical_rect_end = x + x_off + glyph_string_width;

            x_off += run.start_x_offset;
            let mut y_off = run.y_offset;

            if centered_baseline {
                let logical_rect = logical.unwrap_or_default();
                let is_hinted = (logical_rect.y | logical_rect.height) & (SCALE - 1) == 0;
                let mut adjustment = logical_rect.y + logical_rect.height / 2;
                if is_hinted {
                    adjustment = units_round(adjustment);
                }
                y_off += adjustment;
            }

            if self.color[RenderPart::Background as usize].is_some() {
                let overall = *overall_rect.get_or_insert_with(&mut line_logical_extents);
                self.draw_rectangle(
                    RenderPart::Background,
                    x + x_off,
                    y + overall.y,
                    glyph_string_width,
                    overall.height,
                );
            }

            if shape_attr.is_none() {
                self.draw_glyph_item(text, run, x + x_off, y - y_off);
            }

            if decorated {
                let metrics = metrics_of(font, run.item.analysis.language);
                let ink = ink.unwrap_or_default();
                let logical = logical.unwrap_or_default();
                let base_x = x + x_off;
                let base_y = y - y_off;

                if self.underline != UNDERLINE_NONE {
                    self.add_underline(&mut state, &metrics, base_x, base_y, &ink, &logical);
                }
                if self.overline != OVERLINE_NONE {
                    self.add_overline(&mut state, &metrics, base_x, base_y, &ink, &logical);
                }
                if self.strikethrough {
                    self.add_strikethrough(
                        &mut state,
                        &metrics,
                        base_x,
                        base_y,
                        &ink,
                        &logical,
                        run.glyphs.glyphs.len() as i32,
                    );
                }
            }

            if self.underline == UNDERLINE_NONE && state.underline != UNDERLINE_NONE {
                self.draw_underline(&mut state);
            }
            if self.overline == OVERLINE_NONE && state.overline != OVERLINE_NONE {
                self.draw_overline(&mut state);
            }
            if !self.strikethrough && state.strikethrough {
                self.draw_strikethrough(&mut state);
            }

            x_off += glyph_string_width;
            x_off += run.end_x_offset;
        }

        self.draw_underline(&mut state);
        self.draw_overline(&mut state);
        self.draw_strikethrough(&mut state);

        self.line_state = None;
        self.deactivate();
    }

    fn unset_part_colors(&mut self) {
        for part in [
            RenderPart::Foreground,
            RenderPart::Background,
            RenderPart::Underline,
            RenderPart::Strikethrough,
            RenderPart::Overline,
        ] {
            self.set_color(part, None);
        }
    }
}

fn do_glyph_string(cr: &CairoContext, font: Option<&Font>, glyphs: &GlyphString, do_path: bool) {
    let mut renderer = CairoRenderer::at_current_point(cr, do_path);
    if !do_path {
        renderer.activate();
        renderer.unset_part_colors();
    }
    renderer.draw_glyphs(font, glyphs, 0, 0);
    if !do_path {
        renderer.deactivate();
    }
    renderer.restore_current_point();
}

fn do_glyph_item(cr: &CairoContext, text: &str, glyph_item: &GlyphItem, do_path: bool) {
    let mut renderer = CairoRenderer::at_current_point(cr, do_path);
    if !do_path {
        renderer.activate();
        renderer.unset_part_colors();
    }
    renderer.draw_glyph_item(Some(text), glyph_item, 0, 0);
    if !do_path {
        renderer.deactivate();
    }
    renderer.restore_current_point();
}

fn do_line_runs(
    cr: &CairoContext,
    runs: &[GlyphItem],
    text: Option<&str>,
    line_logical_extents: impl FnMut() -> Rectangle,
    do_path: bool,
) {
    let mut renderer = CairoRenderer::at_current_point(cr, do_path);
    renderer.draw_line(runs, text, 0, 0, line_logical_extents);
    renderer.restore_current_point();
}

fn do_error_underline(cr: &CairoContext, x: f64, y: f64, width: f64, height: f64, do_path: bool) {
    if !do_path {
        cr.new_path();
    }
    draw_error_underline_path(cr, x, y, width, height);
    if !do_path {
        cr.fill();
    }
}

pub fn show_glyph_string(cr: &CairoContext, font: Option<&Font>, glyphs: &GlyphString) {
    do_glyph_string(cr, font, glyphs, false);
}

pub fn glyph_string_path(cr: &CairoContext, font: Option<&Font>, glyphs: &GlyphString) {
    do_glyph_string(cr, font, glyphs, true);
}

pub fn show_glyph_item(cr: &CairoContext, text: &str, glyph_item: &GlyphItem) {
    do_glyph_item(cr, text, glyph_item, false);
}

pub fn glyph_item_path(cr: &CairoContext, text: &str, glyph_item: &GlyphItem) {
    do_glyph_item(cr, text, glyph_item, true);
}

pub fn show_line_runs(
    cr: &CairoContext,
    runs: &[GlyphItem],
    text: Option<&str>,
    line_logical_extents: impl FnMut() -> Rectangle,
) {
    do_line_runs(cr, runs, text, line_logical_extents, false);
}

pub fn line_runs_path(
    cr: &CairoContext,
    runs: &[GlyphItem],
    text: Option<&str>,
    line_logical_extents: impl FnMut() -> Rectangle,
) {
    do_line_runs(cr, runs, text, line_logical_extents, true);
}

pub fn show_error_underline(cr: &CairoContext, x: f64, y: f64, width: f64, height: f64) {
    do_error_underline(cr, x, y, width, height, false);
}

pub fn error_underline_path(cr: &CairoContext, x: f64, y: f64, width: f64, height: f64) {
    do_error_underline(cr, x, y, width, height, true);
}
