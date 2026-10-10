//! Southstar — the window-level services: timers, navigator and its sub-objects, the window console, screen, matchMedia, Notification, queueMicrotask, ClipboardItem and the WebRTC stubs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod clipboard_item;
mod console;
mod ffi;
mod media;
mod navigator;
mod rtc;
mod screen;
mod timers;
mod window;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::Js;

const PROTOTYPE_ONLY_WRITABLE: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: false,
};

pub(crate) fn make_ctor(scope: &mut Scope<'_>, name: &str, arity: u32, f: NativeFn) -> Value {
    let function = scope.constructor_or_function(name, arity, f);
    let proto = scope.new_object();
    let _ = scope.define(&proto, "constructor", function.clone(), Attributes::METHOD);
    let _ = scope.define_to_string_tag(&proto, name);
    let _ = scope.define(&function, "prototype", proto, PROTOTYPE_ONLY_WRITABLE);
    function
}

pub(crate) fn resolved(scope: &mut Scope<'_>, value: Value) -> Result<Value, Value> {
    let (promise, resolve, _reject) = scope.new_promise()?;
    scope.call(&resolve, &Value::undefined(), &[value])?;
    Ok(promise)
}

#[derive(Default)]
pub(crate) struct Page {
    counts: RefCell<HashMap<String, u32>>,
    timers: RefCell<timers::Timers>,
    console_timers: RefCell<Option<HashMap<String, Instant>>>,
    media_lists: RefCell<Vec<Value>>,
}

thread_local! {
    static PAGES: RefCell<HashMap<usize, Rc<Page>>> = RefCell::new(HashMap::new());
}

pub(crate) fn page(js: Js) -> Option<Rc<Page>> {
    if js.is_null() {
        return None;
    }
    PAGES
        .try_with(|pages| pages.try_borrow().ok()?.get(&js.key()).cloned())
        .ok()
        .flatten()
}

pub(crate) fn page_or_new(js: Js) -> Rc<Page> {
    PAGES.with(|pages| pages.borrow_mut().entry(js.key()).or_default().clone())
}

pub(crate) fn with_page<R>(js: Js, f: impl FnOnce(&Page) -> R) -> Option<R> {
    page(js).map(|page| f(&page))
}

pub(crate) fn reset(js: Js) {
    if let Some(page) = page(js) {
        timers::clear_all(js, &page);
        let lists = core::mem::take(&mut *page.media_lists.borrow_mut());
        drop(lists);
    }
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES
        .try_with(|pages| pages.borrow_mut().remove(&js.key()))
        .ok()
        .flatten();
    if let Some(page) = page {
        timers::clear_all(js, &page);
        let lists = core::mem::take(&mut *page.media_lists.borrow_mut());
        drop(lists);
    }
}
