//! Southstar — the <img> state getters: complete, naturalWidth, naturalHeight and currentSrc.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_image::ImageRef;
use southstar_js_engine::{Scope, Value};

use crate::{Element, JsResult, ffi, named};

fn image_for(scope: &Scope<'_>, img: Option<Element>) -> Option<ImageRef<'static>> {
    let img = img.filter(|&n| named(Some(n), b"img"))?;
    ffi::image_for_node(ffi::js_of(scope), img)
}

pub(crate) fn complete(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let node = ffi::unwrap_node(this);
    let Some(img) = node.filter(|&n| named(Some(n), b"img")) else {
        return Ok(Value::boolean(true));
    };
    if img.attr(c"src").is_none_or(|src| src.is_empty()) {
        return Ok(Value::boolean(true));
    }
    let done = image_for(scope, Some(img)).is_some_and(|im| im.loaded() || im.failed());
    Ok(Value::boolean(done))
}

fn natural_dimension(scope: &Scope<'_>, this: &Value, width: bool) -> Value {
    let node = ffi::unwrap_node(this);
    let natural = image_for(scope, node).map_or(0, |im| {
        let (w, h) = im.natural_size();
        if width { w } else { h }
    });
    if natural <= 0 {
        return Value::int(0);
    }
    Value::int((f64::from(natural) / ffi::chosen_density(node)) as i32)
}

pub(crate) fn natural_width(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(natural_dimension(scope, this, true))
}

pub(crate) fn natural_height(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(natural_dimension(scope, this, false))
}

pub(crate) fn current_src(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(scope.string(""));
    };
    let Some(chosen) = ffi::chosen_url(node).filter(|url| !url.is_empty()) else {
        return Ok(scope.string(""));
    };
    let url = ffi::resolve_against_doc_base(ffi::js_of(scope), node, &chosen).unwrap_or(chosen);
    Ok(scope.string_from_bytes(&url))
}
