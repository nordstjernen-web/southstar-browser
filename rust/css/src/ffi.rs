//! Southstar — the C ABI of the ported css.c sections, and the GLib and css.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use southstar_glib::{self as glib, GBoolean};

use crate::color;

const COLOR_SCHEME_DARK: c_int = 1;

unsafe extern "C" {
    fn ns_css_get_color_scheme() -> c_int;
    fn g_strdup_printf(format: *const c_char, ...) -> *mut c_char;
}

pub(crate) fn prefers_dark() -> bool {
    unsafe { ns_css_get_color_scheme() == COLOR_SCHEME_DARK }
}

pub(crate) fn strtod(text: &CStr, pos: usize) -> (f64, usize) {
    glib::ascii_strtod_at(text, pos)
}

pub(crate) fn format_g6(value: f64) -> Vec<u8> {
    let text = unsafe { glib::GStr::take(g_strdup_printf(c"%.6g".as_ptr(), value)) };
    text.map(|text| text.to_bytes().to_vec())
        .unwrap_or_default()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_color(
    s: *const c_char,
    r: *mut u8,
    g: *mut u8,
    b: *mut u8,
    a: *mut u8,
) -> GBoolean {
    let mut channels: color::Channels = [None; 4];
    let ok = if s.is_null() {
        channels[3] = Some(255);
        false
    } else {
        color::parse_into(unsafe { CStr::from_ptr(s) }, &mut channels)
    };
    for (slot, value) in [r, g, b, a].into_iter().zip(channels) {
        if let (false, Some(value)) = (slot.is_null(), value) {
            unsafe { *slot = value };
        }
    }
    glib::boolean(ok)
}
