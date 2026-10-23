//! Southstar text — drawing whole layouts and layout lines with the cairo text renderer, shown or added to the current path.
//! Copyright 2004 Red Hat, Inc.; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use super::{CairoRenderer, render_component};
use crate::font::CairoContext;
use crate::layout::{Layout, LayoutLine};

impl CairoRenderer<'_> {
    pub fn draw_layout(&mut self, layout: &Layout, x: i32, y: i32) {
        self.activate();

        let mut iter = layout.iter();
        loop {
            let (_, logical_rect) = iter.line_extents();
            let baseline = iter.baseline();
            self.draw_layout_line(layout, iter.line(), x + logical_rect.x, y + baseline);
            if !iter.next_line() {
                break;
            }
        }

        self.deactivate();
    }

    pub fn draw_layout_line(&mut self, layout: &Layout, line: &LayoutLine, x: i32, y: i32) {
        self.draw_line(&line.runs, Some(layout.text()), x, y, || {
            line.extents(layout).1
        });
    }
}

fn do_layout_line(cr: &CairoContext, layout: &Layout, line: &LayoutLine, do_path: bool) {
    let mut renderer = CairoRenderer::at_current_point(cr, do_path);
    renderer.draw_layout_line(layout, line, 0, 0);
    renderer.restore_current_point();
}

fn do_layout(cr: &CairoContext, layout: &Layout, do_path: bool, components: u32) {
    let mut renderer = CairoRenderer::at_current_point(cr, do_path);
    renderer.set_components(components);
    renderer.draw_layout(layout, 0, 0);
    renderer.restore_current_point();
}

pub fn show_layout_line(cr: &CairoContext, layout: &Layout, line: &LayoutLine) {
    do_layout_line(cr, layout, line, false);
}

pub fn layout_line_path(cr: &CairoContext, layout: &Layout, line: &LayoutLine) {
    do_layout_line(cr, layout, line, true);
}

pub fn show_layout(cr: &CairoContext, layout: &Layout) {
    do_layout(cr, layout, false, render_component::ALL);
}

pub fn layout_path(cr: &CairoContext, layout: &Layout) {
    do_layout(cr, layout, true, render_component::ALL);
}
