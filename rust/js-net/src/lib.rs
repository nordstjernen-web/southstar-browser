//! Southstar — the networking bindings: fetch, Request, Response, AbortController, XMLHttpRequest, WebSocket and EventSource.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod abort;
mod body;
mod fetch;
mod ffi;
mod headers;

use core::cell::RefCell;
use std::collections::HashMap;

use southstar_js_engine::{Scope, Value};

use crate::ffi::Js;

pub(crate) type JsResult<T = Value> = Result<T, Value>;

#[derive(Default)]
pub(crate) struct Page {
    pub fetches: HashMap<u32, fetch::FetchState>,
    pub aborts: HashMap<u32, abort::AbortTimeout>,
    pub body_helper: Option<Value>,
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Page>> = RefCell::new(HashMap::new());
}

pub(crate) fn with_page<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    PAGES
        .try_with(|pages| {
            let mut pages = pages.try_borrow_mut().ok()?;
            Some(f(pages.entry(js).or_default()))
        })
        .ok()
        .flatten()
}

pub(crate) fn existing_page<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> Option<R> {
    PAGES
        .try_with(|pages| {
            let mut pages = pages.try_borrow_mut().ok()?;
            pages.get_mut(&js).map(f)
        })
        .ok()
        .flatten()
}

pub(crate) fn page_init(js: Js) {
    with_page(js, |_| ());
}

pub(crate) fn page_reset(js: Js) {
    let dropped = existing_page(js, |page| core::mem::take(&mut page.fetches));
    drop(dropped);
}

pub(crate) fn page_teardown(js: Js) {
    let page = PAGES
        .try_with(|pages| pages.try_borrow_mut().ok()?.remove(&js))
        .ok()
        .flatten();
    drop(page);
}

pub(crate) fn pending_fetches(js: Js) -> usize {
    existing_page(js, |page| page.fetches.len()).unwrap_or(0)
}

static NEXT_ID: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

pub(crate) fn next_id() -> u32 {
    loop {
        let id = NEXT_ID
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed)
            .wrapping_add(1);
        if id != 0 {
            return id;
        }
    }
}

pub(crate) fn until_nul(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(end) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(end);
    }
    bytes
}

pub(crate) fn c_bytes(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    scope.to_bytes(value).ok().map(until_nul)
}

pub(crate) fn prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn string_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> Option<Vec<u8>> {
    let value = prop(scope, object, key);
    if value.is_string() {
        c_bytes(scope, &value)
    } else {
        None
    }
}

pub(crate) fn bool_prop(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = prop(scope, object, key);
    scope.to_bool(&value)
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn set_str(scope: &mut Scope<'_>, object: &Value, key: &str, text: &[u8]) {
    let value = scope.string_from_bytes(text);
    set(scope, object, key, value);
}

pub(crate) fn is_nullish(value: &Value) -> bool {
    value.is_undefined() || value.is_null()
}

pub(crate) fn call_ignoring(scope: &mut Scope<'_>, function: &Value, this: &Value, args: &[Value]) {
    let _ = scope.call(function, this, args);
}

pub(crate) fn call_method(scope: &mut Scope<'_>, object: &Value, name: &str, args: &[Value]) {
    let function = prop(scope, object, name);
    if scope.is_function(&function) {
        call_ignoring(scope, &function, object, args);
    }
}

pub(crate) fn global_ctor(scope: &mut Scope<'_>, name: &str) -> Value {
    let global = scope.global();
    prop(scope, &global, name)
}

pub(crate) fn proto_of(scope: &mut Scope<'_>, name: &str) -> Value {
    let ctor = global_ctor(scope, name);
    if ctor.is_object() {
        prop(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    }
}

pub(crate) fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let len = prop(scope, array, "length");
    let number = scope.to_number(&len).unwrap_or(0.0);
    if !number.is_finite() {
        return 0;
    }
    number.trunc().rem_euclid(4_294_967_296.0) as u32
}

pub(crate) fn ascii_starts_with(text: &[u8], prefix: &[u8]) -> bool {
    text.len() >= prefix.len() && text[..prefix.len()].eq_ignore_ascii_case(prefix)
}
