//! Southstar — the C ABI of the per-property value parser: css.c's property ids resolved to Rust properties, and parsed values built as the ns_css_value css.c owns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::collections::HashMap;
use std::sync::OnceLock;

use southstar_glib::{self as glib, GBoolean};

use super::animation::RawList;
use super::value::{
    self, KIND_COLOR, KIND_RECT, KIND_SIZE, KIND_URL, NsCssValue, RawColor, RawRect, RawSize,
};
use crate::calc::Parsed;
use crate::prop::Prop;
use crate::property::{self, Body, Value};

unsafe extern "C" {
    fn ns_css_prop_name(prop: c_int) -> *const c_char;
}

struct Tables {
    by_id: Vec<Option<Prop>>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let names: HashMap<&[u8], Prop> = Prop::ALL.iter().map(|&p| (p.name(), p)).collect();
        let mut by_id = Vec::new();
        loop {
            let name = unsafe { ns_css_prop_name(by_id.len() as c_int) };
            if name.is_null() {
                break;
            }
            by_id.push(
                names
                    .get(unsafe { CStr::from_ptr(name) }.to_bytes())
                    .copied(),
            );
        }
        Tables { by_id }
    })
}

pub(crate) fn prop_of(id: c_int) -> Option<Prop> {
    usize::try_from(id)
        .ok()
        .and_then(|i| tables().by_id.get(i).copied().flatten())
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn strdup_opt(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |t| glib::strdup(&t))
}

pub(crate) fn to_c(v: Value) -> *mut NsCssValue {
    let raw = match v.body {
        Body::Keyword(text) => value::new_keyword(&text),
        Body::Length(v, unit) => value::new_value(&Parsed::Length(v, unit)),
        Body::Calc(c) => value::new_value(&Parsed::Calc(c)),
        Body::Color([r, g, b, a]) => {
            let raw = value::alloc(KIND_COLOR);
            unsafe { (*raw).u.color = RawColor { r, g, b, a } };
            raw
        }
        Body::Size(s) => {
            let raw = value::alloc(KIND_SIZE);
            unsafe {
                (*raw).u.size = RawSize {
                    w: s.w,
                    h: s.h,
                    w_unit: s.w_unit,
                    h_unit: s.h_unit,
                    w_auto: glib::boolean(s.w_auto),
                    h_auto: glib::boolean(s.h_auto),
                }
            };
            raw
        }
        Body::Rect(r) => {
            let raw = value::alloc(KIND_RECT);
            unsafe {
                (*raw).u.rect = RawRect {
                    v: r.v,
                    unit: r.unit,
                    is_auto: r.is_auto.map(glib::boolean),
                }
            };
            raw
        }
        Body::Url(url) => {
            let raw = value::alloc(KIND_URL);
            unsafe { (*raw).u.url = glib::strdup(&url) };
            raw
        }
        Body::Shadow(list) => value::new_shadow(&list),
        Body::Gradient(gr) => value::new_gradient(&gr),
        Body::Tracks(tk) => value::new_tracks(&tk),
        Body::Areas(areas) => value::new_areas(&areas),
        Body::Transform(tf) => value::new_transform(&tf),
        Body::Anim(list) => value::new_anim(&RawList::from_entries(&list)),
    };
    unsafe {
        (*raw).image_set_text = strdup_opt(v.image_set_text);
        (*raw).specified = strdup_opt(v.specified);
        (*raw).next_layer = v.next_layer.map_or(ptr::null_mut(), |next| to_c(*next));
    }
    raw
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_value_for(
    prop: c_int,
    text: *const c_char,
) -> *mut NsCssValue {
    let Some(text) = (unsafe { bytes(text) }) else {
        return ptr::null_mut();
    };
    property::parse_for(prop_of(prop), text).map_or(ptr::null_mut(), to_c)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_mask_box_keyword(t: *const c_char) -> *const c_char {
    let Some(found) = unsafe { bytes(t) }.and_then(property::mask_box_keyword) else {
        return ptr::null();
    };
    static_text(found)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_mask_composite_keyword(t: *const c_char) -> *const c_char {
    let Some(found) = unsafe { bytes(t) }.and_then(property::mask_composite_keyword) else {
        return ptr::null();
    };
    static_text(found)
}

fn static_text(word: &[u8]) -> *const c_char {
    static WORDS: OnceLock<HashMap<&'static [u8], std::ffi::CString>> = OnceLock::new();
    let words = WORDS.get_or_init(|| {
        [
            &b"border-box"[..],
            b"padding-box",
            b"content-box",
            b"fill-box",
            b"stroke-box",
            b"view-box",
            b"no-clip",
            b"add",
            b"subtract",
            b"intersect",
            b"exclude",
        ]
        .into_iter()
        .map(|w| (w, std::ffi::CString::new(w).unwrap_or_default()))
        .collect()
    });
    words.get(word).map_or(ptr::null(), |c| c.as_ptr())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_bg_repeat_token(
    tok: *const c_char,
    allow_axis: GBoolean,
) -> GBoolean {
    glib::boolean(
        unsafe { bytes(tok) }.is_some_and(|t| property::bg_repeat_token(t, allow_axis != 0)),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_bg_repeat_canonical(
    a: *const c_char,
    b: *const c_char,
) -> *mut c_char {
    let Some(a) = (unsafe { bytes(a) }) else {
        return ptr::null_mut();
    };
    glib::strdup(&property::bg_repeat_canonical(a, unsafe { bytes(b) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_bg_clip_canonical(text: *const c_char) -> *mut c_char {
    strdup_opt(unsafe { bytes(text) }.and_then(property::bg_clip_canonical))
}
