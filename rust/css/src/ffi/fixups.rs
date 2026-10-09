//! Southstar — the C ABI of the fixups an element's computed style gets after the cascade, applied to css.c's ns_style.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use super::computed_units::{RawStyle, StyleView};
use super::value::{new_keyword, ns_css_value_free};
use crate::display;
use crate::fixups::{self, WIDGET_DECORATIONS};
use crate::prop::Prop;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_display_contents_to_none(
    el: *const NsNode,
    s: *mut RawStyle,
) -> GBoolean {
    let (Some(el), Some(style)) = (unsafe { Node::from_ptr(el) }, unsafe { s.as_mut() }) else {
        return glib::FALSE;
    };
    if style.display.box_ != display::BOX_CONTENTS || !fixups::cannot_be_unboxed(el) {
        return glib::FALSE;
    }
    let slot = &mut style.values[Prop::Display.id()];
    unsafe { ns_css_value_free(*slot) };
    *slot = new_keyword(b"none");
    style.display = display::from_keyword(Some(b"none"));
    glib::TRUE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_strip_native_widget_decorations(
    el: *const NsNode,
    s: *mut RawStyle,
) {
    let (Some(el), Some(style)) = (unsafe { Node::from_ptr(el) }, unsafe { s.as_mut() }) else {
        return;
    };
    if !fixups::is_native_toggle(el) || fixups::appearance_none(&StyleView(style)) {
        return;
    }
    for prop in WIDGET_DECORATIONS {
        let slot = &mut style.values[prop.id()];
        if !slot.is_null() {
            unsafe { ns_css_value_free(*slot) };
            *slot = ptr::null_mut();
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_frame_viewport_from_style(
    s: *const RawStyle,
    w: *mut f64,
    h: *mut f64,
) -> GBoolean {
    let Some((fw, fh)) = unsafe { s.as_ref() }.and_then(|s| fixups::frame_viewport(&StyleView(s)))
    else {
        return glib::FALSE;
    };
    unsafe {
        *w = fw;
        *h = fh;
    }
    glib::TRUE
}
