//! Southstar — the layout, form-control and MathML entry points painting calls back into, installed once by the positioning crate before the first layout.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_cairo::ContextRef;
use southstar_dom::Node;
use southstar_layout::{BoxRef, InlineAttr};
use southstar_style::StyleRef;

use crate::text_layout::{AttrList, Layout};

pub struct LegendGap {
    pub inset: f64,
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

pub struct LayoutLinks {
    pub fieldset_legend_gap: fn(BoxRef<'_>) -> Option<LegendGap>,
    pub inline_apply_atomic_shapes: fn(&AttrList, BoxRef<'_>),
    pub inline_layout_set_attrs: fn(&Layout, &AttrList, BoxRef<'_>),
    pub control_css_extra_w: fn(&InlineAttr, StyleRef<'_>) -> f64,
    pub math_paint: fn(ContextRef<'_>, Node<'_>, f64, f64, f64, [f64; 4]),
    pub sticky_offset: fn(BoxRef<'_>, f64, f64, f64, f64) -> (f64, f64),
    pub in_scroller: fn(BoxRef<'_>) -> bool,
}

static LINKS: OnceLock<LayoutLinks> = OnceLock::new();

pub fn install_layout_links(links: LayoutLinks) {
    let _ = LINKS.set(links);
}

pub(crate) fn get() -> Option<&'static LayoutLinks> {
    LINKS.get()
}
