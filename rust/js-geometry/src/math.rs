//! Southstar — MathML geometry: the client rectangle of an element inside a formula, read back from the formula's own layout.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::Scope;
use southstar_style::PropId;

use crate::boxes::border_box;
use crate::{Element, Rect, element_named, ffi};

const DEFAULT_FONT_PX: f64 = 16.0;

pub(crate) fn client_rect(scope: &mut Scope<'_>, node: Option<Element>) -> Option<Rect> {
    let node = node?;
    let math = southstar_dom::ancestors(node).find(|n| element_named(*n, "math"))?;
    let js = ffi::js_of(scope);
    if js.is_null() {
        return None;
    }
    ffi::flush_layout(js);
    let math_box = ffi::find_by_dom(ffi::layout_root(js)?, math)?;
    let font_px = ffi::box_style(math_box)
        .and_then(|style| style.get(PropId::FontSize))
        .map_or(DEFAULT_FONT_PX, |size| size.length_or(DEFAULT_FONT_PX));
    let [x, y, w, h] = southstar_mathml::element_rect(math, node, font_px)?;
    let outer = border_box(math_box);
    let (border, padding) = (math_box.border(), math_box.padding());
    Some(Rect {
        x: outer.x + border.left + padding.left + x,
        y: outer.y + border.top + padding.top + y,
        w,
        h,
    })
}
