//! Southstar — the FontFace constructor, its descriptor defaults, load() and the loaded promise.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Attributes, ObjectKind, Scope, Value};

use crate::{JsResult, ffi, ready};

const SLANT_ROMAN: i32 = 1;
const SLANT_ITALIC: i32 = 2;
const SLANT_OBLIQUE: i32 = 3;

const DESCRIPTORS: [(&str, &str); 11] = [
    ("style", "normal"),
    ("weight", "normal"),
    ("stretch", "normal"),
    ("unicodeRange", "U+0-10FFFF"),
    ("variant", "normal"),
    ("featureSettings", "normal"),
    ("variationSettings", "normal"),
    ("display", "auto"),
    ("ascentOverride", "normal"),
    ("descentOverride", "normal"),
    ("lineGapOverride", "normal"),
];

pub(crate) fn construct(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let face = scope.new_object();
    let family = match args.first() {
        Some(family) => crate::c_string(scope, family),
        None => scope.string(""),
    };
    crate::set(scope, &face, "family", family);
    let descriptors = args.get(2).filter(|d| d.is_object());
    for (key, default) in DESCRIPTORS {
        let given = descriptors
            .map(|d| scope.get(d, key).unwrap_or_else(|_| Value::undefined()))
            .filter(|v| !v.is_undefined());
        let value = given.unwrap_or_else(|| scope.string(default));
        crate::set(scope, &face, key, value);
    }
    crate::set_str(scope, &face, "status", "unloaded");
    let (loaded, resolve, _) = scope.new_promise()?;
    let _ = scope.call(&resolve, &Value::undefined(), std::slice::from_ref(&face));
    crate::set(scope, &face, "loaded", loaded);
    crate::bind(scope, &face, "load", 0, load);
    Ok(face)
}

fn css_weight(value: &[u8]) -> i32 {
    let value = value.trim_ascii();
    if value.eq_ignore_ascii_case(b"normal") {
        return 400;
    }
    if value.eq_ignore_ascii_case(b"bold") {
        return 700;
    }
    core::str::from_utf8(value)
        .ok()
        .and_then(|text| text.parse::<f64>().ok())
        .filter(|weight| (1.0..=1000.0).contains(weight))
        .map_or(0, |weight| weight.round() as i32)
}

fn css_slant(value: &[u8]) -> i32 {
    let value = value.trim_ascii();
    if value.eq_ignore_ascii_case(b"italic") {
        SLANT_ITALIC
    } else if value.len() >= 7 && value[..7].eq_ignore_ascii_case(b"oblique") {
        SLANT_OBLIQUE
    } else {
        SLANT_ROMAN
    }
}

fn font_bytes(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    match scope.object_kind(value) {
        Some(ObjectKind::TypedArray(_) | ObjectKind::DataView) => scope.view_data(value),
        Some(ObjectKind::ArrayBuffer) => scope.array_buffer_bytes(value),
        _ => None,
    }
}

fn install_data(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let [family, data, weight, style, ..] = args else {
        return Ok(Value::boolean(false));
    };
    let family = scope.to_bytes(family).unwrap_or_default();
    let Some(bytes) = font_bytes(scope, data) else {
        return Ok(Value::boolean(false));
    };
    let descriptors = southstar_font::Descriptors {
        weight: css_weight(&scope.to_bytes(weight).unwrap_or_default()),
        slant: css_slant(&scope.to_bytes(style).unwrap_or_default()),
    };
    let installed = southstar_font::install_face_data(&family, &bytes, descriptors);
    if installed {
        ffi::mark_mutated(ffi::js_of(scope));
    }
    Ok(Value::boolean(installed))
}

pub fn install_data_hook(scope: &mut Scope<'_>, global: &Value) {
    let function = scope.function("__ns_font_face_install", 4, install_data);
    let hidden = Attributes {
        writable: true,
        enumerable: false,
        configurable: true,
    };
    let _ = scope.define(global, "__ns_font_face_install", function, hidden);
}

fn load(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    crate::set_str(scope, this, "status", "loaded");
    let js = ffi::js_of(scope);
    ffi::flush_layout(js);
    let (promise, resolve, _) = scope.new_promise()?;
    ready::resolve_when_fonts_loaded(scope, js, resolve, this.clone());
    Ok(promise)
}
