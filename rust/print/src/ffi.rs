//! Southstar — the print setup and its C ABI default, drawing one sheet, and the style and paint calls pagination needs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::sync::OnceLock;

use southstar_cairo::ContextRef;
use southstar_css::PageRule;
use southstar_layout::BoxRef;

const A4_WIDTH: f64 = 210.0 * 96.0 / 25.4;
const A4_HEIGHT: f64 = 297.0 * 96.0 / 25.4;
const MARGIN: f64 = 0.5 * 96.0;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PrintSetup {
    pub width: f64,
    pub height: f64,
    pub margin_top: f64,
    pub margin_right: f64,
    pub margin_bottom: f64,
    pub margin_left: f64,
}

#[derive(Clone, Copy)]
pub enum BreakProp {
    Before,
    After,
    Inside,
}

fn break_props() -> &'static [c_int; 3] {
    static PROPS: OnceLock<[c_int; 3]> = OnceLock::new();
    PROPS.get_or_init(|| {
        [c"break-before", c"break-after", c"break-inside"].map(|name| {
            southstar_css::Prop::from_name(name.to_bytes()).map_or(-1, |p| p.id() as c_int)
        })
    })
}

pub fn break_keyword(b: BoxRef<'_>, prop: BreakProp) -> Option<&CStr> {
    let prop = usize::try_from(break_props()[prop as usize])
        .ok()
        .and_then(southstar_css::Prop::from_id)?;
    southstar_style::box_style(b)?.keyword_of(prop)
}

pub fn line_height_px(b: BoxRef<'_>) -> f64 {
    southstar_style::box_style(b).map_or(0.0, |s| southstar_paint::css_line_height_px(Some(s)))
}

impl PrintSetup {
    pub fn a4() -> PrintSetup {
        PrintSetup {
            width: A4_WIDTH,
            height: A4_HEIGHT,
            margin_top: MARGIN,
            margin_right: MARGIN,
            margin_bottom: MARGIN,
            margin_left: MARGIN,
        }
    }

    pub fn apply_page_rule(&mut self, rule: &PageRule) {
        if rule.has_size != 0 && rule.width > 1.0 && rule.height > 1.0 {
            self.width = rule.width;
            self.height = rule.height;
        }
        let margins = [
            &mut self.margin_top,
            &mut self.margin_right,
            &mut self.margin_bottom,
            &mut self.margin_left,
        ];
        for (i, side) in margins.into_iter().enumerate() {
            if rule.has_margin[i] != 0 {
                *side = rule.margin[i];
            }
        }
    }
}

pub fn draw_page(
    cr: ContextRef,
    root: BoxRef<'_>,
    setup: &PrintSetup,
    scale: f64,
    page_top: f64,
    page_bottom: f64,
) {
    let content_w = setup.width - setup.margin_left - setup.margin_right;
    let content_h = page_bottom - page_top;
    if !crate::exceeds(content_w, 0.0) || !crate::exceeds(content_h, 0.0) {
        return;
    }
    cr.save();
    cr.scale(scale, scale);
    cr.translate(setup.margin_left, setup.margin_top);
    cr.rectangle(0.0, 0.0, content_w, content_h);
    cr.clip();
    cr.translate(0.0, -page_top);
    southstar_paint::paint(cr, root, None, None);
    cr.restore();
}
