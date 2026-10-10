//! Southstar — the window's bindings: History, the Navigation API, Location, window.open, postMessage, session history, the browsing-context members and named access.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod context;
mod ffi;
mod history;
mod location;
mod message;
mod named;
mod navigation;

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use southstar_js_engine::{NativeFn, Scope, Value};

use crate::ffi::Js;

pub(crate) struct Entry {
    pub url: Vec<u8>,
    pub state: Value,
    pub key: Option<String>,
    pub id: Option<String>,
}

pub(crate) struct Session {
    pub entries: Option<Vec<Entry>>,
    pub pos: usize,
    pub state: Value,
    pub length: i32,
    pub key_seq: u64,
    pub navigation: Value,
    pub popstates: VecDeque<Value>,
}

impl Default for Session {
    fn default() -> Session {
        Session {
            entries: None,
            pos: 0,
            state: Value::null(),
            length: 1,
            key_seq: 0,
            navigation: Value::undefined(),
            popstates: VecDeque::new(),
        }
    }
}

#[derive(Default)]
pub(crate) struct Links {
    pub forwards: Vec<(Value, Value)>,
    pub outwards: Vec<(Value, Value)>,
}

#[derive(Default)]
pub(crate) struct Page {
    pub session: RefCell<Session>,
    pub links: RefCell<Links>,
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Rc<Page>>> = RefCell::new(HashMap::new());
}

pub(crate) fn page(js: Js) -> Rc<Page> {
    PAGES.with(|pages| pages.borrow_mut().entry(js).or_default().clone())
}

pub(crate) fn existing_page(js: Js) -> Option<Rc<Page>> {
    PAGES
        .try_with(|pages| pages.try_borrow().ok()?.get(&js).cloned())
        .ok()
        .flatten()
}

pub(crate) fn teardown(js: Js) {
    let removed = PAGES.with(|pages| pages.borrow_mut().remove(&js));
    drop(removed);
}

pub(crate) fn c_text(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(nul) = bytes.iter().position(|&c| c == 0) {
        bytes.truncate(nul);
    }
    bytes
}

pub(crate) fn text_of(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    scope.to_bytes(value).ok().map(c_text)
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn truthy(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    let value = get(scope, object, key);
    scope.to_bool(&value)
}

pub(crate) fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub(crate) fn bind_getter(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    function_name: &str,
    f: NativeFn,
) {
    let getter = scope.function(function_name, 0, f);
    let _ = scope.define_accessor(
        object,
        name,
        Some(&getter),
        None,
        southstar_js_engine::Attributes::CONFIGURABLE,
    );
}

pub(crate) fn is_nullish(value: &Value) -> bool {
    value.is_undefined() || value.is_null()
}

pub(crate) fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let length = get(scope, array, "length");
    scope.to_int32(&length).map_or(0, |n| n as u32)
}
