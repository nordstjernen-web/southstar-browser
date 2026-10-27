//! Southstar — the fixups an element's computed style gets after the cascade, applied to css.c's ns_style.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;

use super::computed_units::{RawStyle, StyleView};
use super::value::new_keyword;
use crate::display;
use crate::fixups::{self, WIDGET_DECORATIONS};
use crate::prop::Prop;

pub(super) fn display_contents_to_none(el: Node<'_>, style: &mut RawStyle) -> bool {
    if style.display.box_ != display::BOX_CONTENTS || !fixups::cannot_be_unboxed(el) {
        return false;
    }
    style.values[Prop::Display.id()] = Some(new_keyword(b"none"));
    style.display = display::from_keyword(Some(b"none"));
    true
}

pub(super) fn strip_native_widget_decorations(el: Node<'_>, style: &mut RawStyle) {
    if !fixups::is_native_toggle(el) || fixups::appearance_none(&StyleView(style)) {
        return;
    }
    for prop in WIDGET_DECORATIONS {
        style.values[prop.id()] = None;
    }
}

pub(super) fn frame_viewport_from_style(style: Option<&RawStyle>) -> Option<(f64, f64)> {
    style.and_then(|style| fixups::frame_viewport(&StyleView(style)))
}
