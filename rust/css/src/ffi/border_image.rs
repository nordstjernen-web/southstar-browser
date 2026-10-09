//! Southstar — the C ABI of the border-image parameters painting reads from a computed style.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

use super::value;
use crate::border_image::{self, Params};

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
}

fn prop_ids() -> &'static [c_int; 4] {
    static IDS: OnceLock<[c_int; 4]> = OnceLock::new();
    IDS.get_or_init(|| {
        [
            c"border-image-slice",
            c"border-image-width",
            c"border-image-outset",
            c"border-image-repeat",
        ]
        .map(|name| unsafe { ns_css_prop_id(name.as_ptr()) })
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_border_image_params(style: *const c_void, out: *mut Params) {
    if out.is_null() {
        return;
    }
    let ids = prop_ids();
    let text = |i: usize, fallback: &'static [u8]| -> &[u8] {
        if style.is_null() {
            return fallback;
        }
        unsafe { value::keyword_of(value::style_value(style, ids[i])) }
            .filter(|t| !t.is_empty())
            .unwrap_or(fallback)
    };
    let params = border_image::params(
        text(0, b"100%"),
        text(1, b"1"),
        text(2, b"0"),
        text(3, b"stretch"),
    );
    unsafe { *out = params };
}
