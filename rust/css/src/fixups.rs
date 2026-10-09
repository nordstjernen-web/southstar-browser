//! Southstar — the fixups an element's computed style gets after the cascade: whether an element can be unboxed by display: contents, which native checkbox and radio widgets draw their own box, and the viewport a frame's own width and height give its document.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, Node};

use crate::ffi::{Slot, StyleView};
use crate::prop::Prop;
use crate::units::PX;

const NOT_UNBOXABLE: &[&[u8]] = &[
    b"audio",
    b"br",
    b"canvas",
    b"embed",
    b"frame",
    b"frameset",
    b"iframe",
    b"img",
    b"input",
    b"meter",
    b"object",
    b"progress",
    b"select",
    b"textarea",
    b"video",
    b"wbr",
];

pub(crate) const WIDGET_DECORATIONS: [Prop; 23] = [
    Prop::BackgroundColor,
    Prop::BackgroundImage,
    Prop::BorderTopWidth,
    Prop::BorderRightWidth,
    Prop::BorderBottomWidth,
    Prop::BorderLeftWidth,
    Prop::BorderTopStyle,
    Prop::BorderRightStyle,
    Prop::BorderBottomStyle,
    Prop::BorderLeftStyle,
    Prop::BorderTopColor,
    Prop::BorderRightColor,
    Prop::BorderBottomColor,
    Prop::BorderLeftColor,
    Prop::BorderTopLeftRadius,
    Prop::BorderTopRightRadius,
    Prop::BorderBottomRightRadius,
    Prop::BorderBottomLeftRadius,
    Prop::PaddingTop,
    Prop::PaddingRight,
    Prop::PaddingBottom,
    Prop::PaddingLeft,
    Prop::BoxShadow,
];

pub(crate) fn cannot_be_unboxed(el: Node) -> bool {
    let Some(name) = el.element_name() else {
        return false;
    };
    if el.flags() & FLAG_SVG_NS != 0 {
        return name == b"svg" && el.parent().is_some_and(|p| p.flags() & FLAG_SVG_NS == 0);
    }
    if el.flags() & FLAG_FOREIGN_NS != 0 {
        return false;
    }
    NOT_UNBOXABLE
        .iter()
        .any(|tag| name.eq_ignore_ascii_case(tag))
}

pub(crate) fn is_native_toggle(el: Node) -> bool {
    el.element_name() == Some(b"input")
        && el.attr(c"type").is_some_and(|ty| {
            let ty = ty.to_bytes();
            ty.eq_ignore_ascii_case(b"checkbox") || ty.eq_ignore_ascii_case(b"radio")
        })
}

fn keyword<'a>(style: &'a StyleView<'a>, prop: Prop) -> Option<&'a [u8]> {
    match style.get(prop.id()) {
        Some(Slot::Keyword(kw)) => kw.map(|kw| kw.to_bytes()),
        _ => None,
    }
}

pub(crate) fn appearance_none(style: &StyleView) -> bool {
    keyword(style, Prop::Appearance) == Some(b"none")
}

fn px(style: &StyleView, prop: Prop) -> Option<f64> {
    match style.get(prop.id()) {
        Some(Slot::Length(l)) if l.unit == PX => Some(l.v),
        _ => None,
    }
}

fn border_px(style: &StyleView, width: Prop, line: Prop) -> f64 {
    match style.get(line.id()) {
        None => 0.0,
        Some(Slot::Keyword(Some(kw))) if matches!(kw.to_bytes(), b"none" | b"hidden") => 0.0,
        Some(_) => px(style, width).unwrap_or(0.0),
    }
}

pub(crate) fn frame_viewport(style: &StyleView) -> Option<(f64, f64)> {
    let mut w = px(style, Prop::Width)?;
    let mut h = px(style, Prop::Height)?;
    if keyword(style, Prop::BoxSizing) == Some(b"border-box") {
        let edge = |prop| px(style, prop).unwrap_or(0.0);
        w -= edge(Prop::PaddingLeft)
            + edge(Prop::PaddingRight)
            + border_px(style, Prop::BorderLeftWidth, Prop::BorderLeftStyle)
            + border_px(style, Prop::BorderRightWidth, Prop::BorderRightStyle);
        h -= edge(Prop::PaddingTop)
            + edge(Prop::PaddingBottom)
            + border_px(style, Prop::BorderTopWidth, Prop::BorderTopStyle)
            + border_px(style, Prop::BorderBottomWidth, Prop::BorderBottomStyle);
    }
    (w > 0.0 && h > 0.0).then_some((w, h))
}
