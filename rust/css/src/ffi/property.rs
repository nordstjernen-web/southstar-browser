//! Southstar — the C ABI of the property table and the per-property value parser: property ids, names, aliases, inheritance and paint-only properties, and parsed values built as the ns_css_value css.c owns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::animation::RawList;
use super::value::{
    self, KIND_COLOR, KIND_RECT, KIND_SIZE, KIND_URL, NsCssValue, RawColor, RawRect, RawSize,
};
use crate::calc::Parsed;
use crate::prop::Prop;
use crate::property::{self, Body, Value};

pub(crate) fn id_of(prop: Prop) -> c_int {
    prop.id() as c_int
}

pub(crate) fn prop_of(id: c_int) -> Option<Prop> {
    usize::try_from(id).ok().and_then(Prop::from_id)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_prop_id(name: *const c_char) -> c_int {
    unsafe { bytes(name) }
        .and_then(Prop::from_name)
        .map_or(-1, id_of)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_prop_name(prop: c_int) -> *const c_char {
    prop_of(prop).map_or(ptr::null(), |p| p.c_name().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_prop_inherits(prop: c_int) -> GBoolean {
    glib::boolean(prop_of(prop).is_some_and(Prop::inherits))
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_prop_affects_layout(prop: c_int) -> GBoolean {
    glib::boolean(prop_of(prop).is_none_or(Prop::affects_layout))
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
pub unsafe extern "C" fn ns_css_initial_value_text(name: *const c_char) -> *const c_char {
    unsafe { bytes(name) }
        .and_then(crate::initial::initial_value)
        .map_or(ptr::null(), CStr::as_ptr)
}
