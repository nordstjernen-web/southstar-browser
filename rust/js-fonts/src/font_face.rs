//! Southstar — the FontFace constructor, its descriptor defaults, load() and the loaded promise.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, ffi, ready};

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

fn load(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    crate::set_str(scope, this, "status", "loaded");
    let js = ffi::js_of(scope);
    ffi::flush_layout(js);
    let (promise, resolve, _) = scope.new_promise()?;
    ready::resolve_when_fonts_loaded(scope, js, resolve, this.clone());
    Ok(promise)
}
