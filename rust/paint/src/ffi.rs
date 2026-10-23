//! Southstar — the C ABI of painting, over the cairo, southstar-text and engine bindings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub mod engine;

use core::ffi::{c_double, c_int};

use southstar_glib::GBoolean;
use southstar_layout::{BoxRef, InlineAttr, NsBox};

use crate::{inline, text};

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_pango_font_size(size_px: c_double) -> c_int {
    text::pango_font_size(size_px)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_paint_register_font_oracle() {
    southstar_css::set_font_oracle(southstar_css::FontOracle {
        available: text::font_available,
        generation: text::font_generation,
        metrics: text::font_metrics,
    });
    southstar_css::set_font_alias(text::font_alias);
}

fn box_ref<'a>(b: *const NsBox) -> Option<BoxRef<'a>> {
    unsafe { BoxRef::from_ptr(b) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_drop_box_cache(b: *mut NsBox) {
    if let Some(b) = box_ref(b) {
        inline::drop_box_cache(b);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_inline_xy_to_byte(
    b: *const NsBox,
    rel_x: c_double,
    rel_y: c_double,
    out_byte: *mut usize,
) -> GBoolean {
    let Some(byte) = box_ref(b).and_then(|b| inline::inline_xy_to_byte(b, rel_x, rel_y)) else {
        return 0;
    };
    if let Some(out) = unsafe { out_byte.as_mut() } {
        *out = byte;
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_inline_word_range(
    b: *const NsBox,
    byte: usize,
    out_start: *mut usize,
    out_end: *mut usize,
) -> GBoolean {
    let Some((s, e)) = box_ref(b).and_then(|b| inline::inline_word_range(b, byte)) else {
        return 0;
    };
    if let Some(out) = unsafe { out_start.as_mut() } {
        *out = s;
    }
    if let Some(out) = unsafe { out_end.as_mut() } {
        *out = e;
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_paint_inline_range_extents(
    b: *const NsBox,
    start: usize,
    len: usize,
    element: *const InlineAttr,
    out_x: *mut c_double,
    out_y: *mut c_double,
    out_w: *mut c_double,
    out_h: *mut c_double,
) -> GBoolean {
    let element = unsafe { element.as_ref() };
    let Some((x, y, w, h)) =
        box_ref(b).and_then(|b| inline::inline_range_extents(b, start, len, element))
    else {
        return 0;
    };
    for (out, v) in [(out_x, x), (out_y, y), (out_w, w), (out_h, h)] {
        if let Some(out) = unsafe { out.as_mut() } {
            *out = v;
        }
    }
    1
}
