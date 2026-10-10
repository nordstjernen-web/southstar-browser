//! Southstar — URL, URLSearchParams, atob/btoa, TextEncoder/TextDecoder, object URLs and FileReader.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod base64;
mod blob;
mod ffi;
mod file_reader;
mod search_params;
mod text_codec;
mod url;

use std::cell::RefCell;
use std::collections::HashMap;

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::Js;

pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) const ALL: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

#[derive(Default)]
pub(crate) struct Page {
    url_helper: Option<Value>,
    search_params_helper: Option<Value>,
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Page>> = RefCell::new(HashMap::new());
}

pub(crate) fn with_page<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> R {
    PAGES.with(|pages| f(pages.borrow_mut().entry(js).or_default()))
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES
        .try_with(|pages| pages.borrow_mut().remove(&js))
        .ok()
        .flatten();
    drop(page);
}

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &[u8]) {
    let value = scope.string_from_bytes(text);
    set(scope, object, key, value);
}

pub(crate) fn truthy(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = get(scope, object, key);
    scope.to_bool(&value)
}

pub(crate) fn int64_prop(scope: &mut Scope<'_>, object: &Value, key: &str, fallback: i64) -> i64 {
    let value = get(scope, object, key);
    scope.to_int64(&value).unwrap_or(fallback)
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn is_nullish(value: &Value) -> bool {
    value.is_undefined() || value.is_null()
}

pub(crate) fn proto_of(scope: &mut Scope<'_>, global: &Value, name: &str) -> Value {
    let ctor = get(scope, global, name);
    if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    }
}

pub(crate) fn bind_fn(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    arity: u32,
    f: southstar_js_engine::NativeFn,
) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub(crate) fn latin1_string(scope: &mut Scope<'_>, bytes: &[u8]) -> Value {
    let text: String = bytes.iter().map(|&b| char::from(b)).collect();
    scope.string(&text)
}

pub(crate) fn illegal_invocation(scope: &mut Scope<'_>) -> Value {
    scope.type_error("Illegal invocation")
}
