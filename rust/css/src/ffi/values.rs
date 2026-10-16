//! Southstar — value serialization, interpolation, comparison and the specified canonical text, reading css.c's ns_css_value into the Rust value.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use core::ptr;

use super::property::to_c;
use super::value::{
    KIND_ANIM, KIND_AREAS, KIND_CALC, KIND_COLOR, KIND_GRADIENT, KIND_KEYWORD, KIND_LENGTH,
    KIND_RECT, KIND_SHADOW, KIND_SIZE, KIND_TRACKS, KIND_TRANSFORM, KIND_URL, NsCssValue,
    OwnedValue,
};
use crate::grid::{AreaRect, Areas};
use crate::property::{Body, Rect, Size, Value};
use crate::values;

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

unsafe fn owned_bytes(s: *const c_char) -> Option<Vec<u8>> {
    unsafe { bytes(s) }.map(<[u8]>::to_vec)
}

unsafe fn body_of(raw: &NsCssValue) -> Body {
    let u = &raw.u;
    unsafe {
        match raw.kind {
            KIND_LENGTH => Body::Length(u.length.v, u.length.unit),
            KIND_SIZE => Body::Size(Size {
                w: u.size.w,
                h: u.size.h,
                w_unit: u.size.w_unit,
                h_unit: u.size.h_unit,
                w_auto: u.size.w_auto != 0,
                h_auto: u.size.h_auto != 0,
            }),
            KIND_COLOR => Body::Color(u.color.to_color()),
            KIND_CALC => Body::Calc(u.calc.to_calc()),
            KIND_SHADOW => Body::Shadow(Box::new(u.shadow)),
            KIND_GRADIENT => Body::Gradient(Box::new(u.gradient)),
            KIND_TRACKS => Body::Tracks(Box::new(u.tracks)),
            KIND_URL => Body::Url(owned_bytes(u.url).unwrap_or_default()),
            KIND_TRANSFORM => Body::Transform(Box::new(u.transform)),
            KIND_AREAS => {
                let n = usize::try_from(u.areas.n_rects)
                    .unwrap_or(0)
                    .min(u.areas.rects.len());
                Body::Areas(Areas {
                    rows: u.areas.n_rows,
                    cols: u.areas.n_cols,
                    rects: u.areas.rects[..n]
                        .iter()
                        .map(|rect| AreaRect {
                            name: owned_bytes(rect.name).unwrap_or_default(),
                            r0: rect.r0,
                            r1: rect.r1,
                            c0: rect.c0,
                            c1: rect.c1,
                        })
                        .collect(),
                })
            }
            KIND_ANIM => Body::Anim(u.anim.entries()),
            KIND_RECT => Body::Rect(Rect {
                v: u.rect.v,
                unit: u.rect.unit,
                is_auto: u.rect.is_auto.map(|flag| flag != 0),
            }),
            KIND_KEYWORD => Body::Keyword(owned_bytes(u.keyword).unwrap_or_default()),
            _ => Body::Keyword(Vec::new()),
        }
    }
}

unsafe fn from_c(v: *const NsCssValue, layers: bool) -> Option<Value> {
    let raw = unsafe { v.as_ref() }?;
    Some(Value {
        body: unsafe { body_of(raw) },
        image_set_text: unsafe { owned_bytes(raw.image_set_text) },
        specified: unsafe { owned_bytes(raw.specified) },
        next_layer: if layers {
            unsafe { from_c(raw.next_layer, true) }.map(Box::new)
        } else {
            None
        },
    })
}

pub(super) unsafe fn text_of(v: *const NsCssValue, specified: bool) -> Vec<u8> {
    unsafe { from_c(v, true) }
        .map(|value| values::serialize(&value, specified))
        .unwrap_or_default()
}

pub fn value_serialize(v: &NsCssValue) -> Vec<u8> {
    // SAFETY: a value's strings and next layer are null or owned by it for as long as the reference lives.
    unsafe { text_of(v, false) }
}

fn rust_value(v: &NsCssValue, layers: bool) -> Option<Value> {
    // SAFETY: a reference is a live value, and the strings and layers it points to are its own.
    unsafe { from_c(v, layers) }
}

pub fn value_interpolate(a: &NsCssValue, b: &NsCssValue, t: f64) -> Option<OwnedValue> {
    let (Some(a), Some(b)) = (rust_value(a, false), rust_value(b, false)) else {
        return None;
    };
    values::interpolate(&a, &b, t).map(|value| OwnedValue(to_c(value)))
}

pub fn values_equal(a: Option<&NsCssValue>, b: Option<&NsCssValue>) -> bool {
    let (a, b) = match (a, b) {
        (None, None) => return true,
        (Some(a), Some(b)) if ptr::eq(a, b) => return true,
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    let (Some(a), Some(b)) = (rust_value(a, true), rust_value(b, true)) else {
        return false;
    };
    values::serialize(&a, false) == values::serialize(&b, false)
}
