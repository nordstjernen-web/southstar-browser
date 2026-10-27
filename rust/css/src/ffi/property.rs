//! Southstar — the property table and the per-property value parser: property ids, names, aliases, inheritance and paint-only properties, and parsed values built as the values styles store.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_glib::{self as glib};

use super::animation::RawList;
use super::value::{self, NsCssValue, OwnedValue, Payload, RawColor, RawRect, RawSize};
use crate::calc::Parsed;
use crate::prop::Prop;
use crate::property::{Body, Value};

pub(crate) fn id_of(prop: Prop) -> c_int {
    prop.id() as c_int
}

pub(crate) fn prop_of(id: c_int) -> Option<Prop> {
    usize::try_from(id).ok().and_then(Prop::from_id)
}

fn payload(body: Body) -> Payload {
    match body {
        Body::Keyword(text) => Payload::keyword(&text),
        Body::Length(v, unit) => Payload::of_parsed(&Parsed::Length(v, unit)),
        Body::Calc(c) => Payload::of_parsed(&Parsed::Calc(c)),
        Body::Color(color) => Payload::Color(RawColor::from_color(&color)),
        Body::Size(s) => Payload::Size(RawSize {
            w: s.w,
            h: s.h,
            w_unit: s.w_unit,
            h_unit: s.h_unit,
            w_auto: glib::boolean(s.w_auto),
            h_auto: glib::boolean(s.h_auto),
        }),
        Body::Rect(r) => Payload::Rect(RawRect {
            v: r.v,
            unit: r.unit,
            is_auto: r.is_auto.map(glib::boolean),
        }),
        Body::Url(url) => Payload::Url(Some(value::c_dup(&url))),
        Body::Shadow(list) => Payload::Shadow(list),
        Body::Gradient(gr) => Payload::Gradient(gr),
        Body::Tracks(tk) => Payload::Tracks(tk),
        Body::Areas(areas) => Payload::Areas(Box::new(value::areas_of(&areas))),
        Body::Transform(tf) => Payload::Transform(tf),
        Body::Anim(list) => Payload::Anim(Box::new(RawList::from_entries(&list))),
    }
}

pub(crate) fn to_c(v: Value) -> OwnedValue {
    OwnedValue::new(NsCssValue {
        u: payload(v.body),
        image_set_text: v.image_set_text.map(|t| value::c_dup(&t)),
        specified: v.specified.map(|t| value::c_dup(&t)),
        next_layer: v.next_layer.map(|next| to_c(*next)),
    })
}
