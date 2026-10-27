//! Southstar — value serialization, interpolation, comparison and the specified canonical text, reading a stored value into the Rust value.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use core::ptr;

use super::property::to_c;
use super::value::{NsCssValue, OwnedValue, Payload};
use crate::grid::{AreaRect, Areas};
use crate::prop::Prop;
use crate::property::{Body, Rect, Size, Value};
use crate::values;

fn owned_bytes(s: Option<&CStr>) -> Option<Vec<u8>> {
    s.map(|s| s.to_bytes().to_vec())
}

fn body_of(raw: &NsCssValue) -> Body {
    match &raw.u {
        Payload::Length(length) => Body::Length(length.v, length.unit),
        Payload::Size(size) => Body::Size(Size {
            w: size.w,
            h: size.h,
            w_unit: size.w_unit,
            h_unit: size.h_unit,
            w_auto: size.w_auto != 0,
            h_auto: size.h_auto != 0,
        }),
        Payload::Color(color) => Body::Color(color.to_color()),
        Payload::Calc(calc) => Body::Calc(calc.to_calc()),
        Payload::Shadow(shadow) => Body::Shadow(shadow.clone()),
        Payload::Gradient(gradient) => Body::Gradient(gradient.clone()),
        Payload::Tracks(tracks) => Body::Tracks(tracks.clone()),
        Payload::Url(url) => Body::Url(owned_bytes(url.as_deref()).unwrap_or_default()),
        Payload::Transform(transform) => Body::Transform(transform.clone()),
        Payload::Areas(areas) => {
            let n = usize::try_from(areas.n_rects)
                .unwrap_or(0)
                .min(areas.rects.len());
            Body::Areas(Areas {
                rows: areas.n_rows,
                cols: areas.n_cols,
                rects: areas.rects[..n]
                    .iter()
                    .map(|rect| AreaRect {
                        name: owned_bytes(rect.name()).unwrap_or_default(),
                        r0: rect.r0,
                        r1: rect.r1,
                        c0: rect.c0,
                        c1: rect.c1,
                    })
                    .collect(),
            })
        }
        Payload::Anim(list) => Body::Anim(list.entries()),
        Payload::Rect(rect) => Body::Rect(Rect {
            v: rect.v,
            unit: rect.unit,
            is_auto: rect.is_auto.map(|flag| flag != 0),
        }),
        Payload::Keyword(keyword) => {
            Body::Keyword(owned_bytes(keyword.as_deref()).unwrap_or_default())
        }
    }
}

fn rust_value(raw: &NsCssValue) -> Value {
    Value {
        body: body_of(raw),
        image_set_text: owned_bytes(raw.image_set_text.as_deref()),
        specified: owned_bytes(raw.specified.as_deref()),
        next_layer: raw.next_layer().map(|next| Box::new(rust_value(next))),
    }
}

pub(super) fn text_of(v: Option<&NsCssValue>, specified: bool) -> Vec<u8> {
    v.map(|v| values::serialize(&rust_value(v), specified))
        .unwrap_or_default()
}

pub fn value_serialize(v: &NsCssValue) -> Vec<u8> {
    text_of(Some(v), false)
}

pub fn value_resolve_font_units(v: &NsCssValue, font_px: f64, root_px: f64) -> Option<OwnedValue> {
    let mut value = rust_value(v);
    values::resolve_font_units(&mut value, font_px, root_px).then(|| to_c(value))
}

pub fn value_interpolate(
    prop: Option<Prop>,
    a: &NsCssValue,
    b: &NsCssValue,
    t: f64,
    keywords: bool,
) -> Option<OwnedValue> {
    let (a, b) = (rust_value(a), rust_value(b));
    values::interpolate_prop(prop, &a, &b, t, keywords).map(to_c)
}

pub fn values_equal(a: Option<&NsCssValue>, b: Option<&NsCssValue>) -> bool {
    let (a, b) = match (a, b) {
        (None, None) => return true,
        (Some(a), Some(b)) if ptr::eq(a, b) => return true,
        (Some(a), Some(b)) => (a, b),
        _ => return false,
    };
    let (a, b) = (rust_value(a), rust_value(b));
    values::serialize(&a, false) == values::serialize(&b, false)
}
