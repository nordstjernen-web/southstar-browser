//! Southstar — MutationObserver, IntersectionObserver and ResizeObserver: each page's observers, the records they queue and the ticks that deliver them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod intersection;
mod mutation;
mod resize;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use southstar_js_engine::{NativeFn, Scope, Value};

use crate::ffi::Js;

#[derive(Default)]
pub(crate) struct Page {
    mutation: RefCell<Option<Vec<Weak<mutation::Observer>>>>,
    drain_scheduled: Cell<bool>,
    intersection: RefCell<Vec<Weak<intersection::Observer>>>,
    resize: RefCell<Vec<Value>>,
    ticking: Cell<bool>,
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Rc<Page>>> = RefCell::new(HashMap::new());
}

pub(crate) fn page(js: Js) -> Option<Rc<Page>> {
    PAGES
        .try_with(|pages| pages.try_borrow().ok()?.get(&js).cloned())
        .ok()
        .flatten()
}

pub(crate) fn page_or_new(js: Js) -> Rc<Page> {
    PAGES.with(|pages| pages.borrow_mut().entry(js).or_default().clone())
}

fn upgrade_at<T>(list: &RefCell<Vec<Weak<T>>>, index: usize) -> Option<Option<Rc<T>>> {
    let weak = list.borrow().get(index).cloned()?;
    Some(weak.upgrade())
}

fn forget<T>(list: &mut Vec<Weak<T>>, gone: *const T) {
    if let Some(index) = list.iter().position(|weak| weak.as_ptr() == gone) {
        list.swap_remove(index);
    }
}

pub(crate) fn reset(js: Js) {
    let Some(page) = page(js) else {
        return;
    };
    let mutation: Vec<_> = page.mutation.borrow().iter().flatten().cloned().collect();
    for observer in mutation.iter().filter_map(Weak::upgrade) {
        observer.disconnect();
    }
    let intersection = page.intersection.borrow().clone();
    for observer in intersection.iter().filter_map(Weak::upgrade) {
        observer.reset();
    }
}

pub(crate) fn teardown(js: Js) {
    let Some(page) = page(js) else {
        return;
    };
    let mutation = page.mutation.borrow_mut().take();
    for observer in mutation.iter().flatten().filter_map(Weak::upgrade) {
        observer.teardown();
    }
    let intersection = core::mem::take(&mut *page.intersection.borrow_mut());
    for observer in intersection.iter().filter_map(Weak::upgrade) {
        observer.teardown();
    }
    let resize = core::mem::take(&mut *page.resize.borrow_mut());
    page.drain_scheduled.set(false);
    let removed = PAGES.with(|pages| pages.borrow_mut().remove(&js));
    drop(removed);
    drop(resize);
}

pub(crate) fn c_text(mut bytes: Vec<u8>) -> Vec<u8> {
    if let Some(nul) = bytes.iter().position(|&c| c == 0) {
        bytes.truncate(nul);
    }
    bytes
}

pub(crate) fn text_of(scope: &mut Scope<'_>, value: &Value) -> Result<Vec<u8>, Value> {
    scope.to_bytes(value).map(c_text)
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn set_index(scope: &mut Scope<'_>, object: &Value, index: u32, value: Value) {
    let _ = scope.set_index(object, index, value);
}

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn array_length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let length = get(scope, array, "length");
    scope.to_int32(&length).map_or(0, |n| n as u32)
}

pub(crate) fn number_or(scope: &mut Scope<'_>, value: &Value) -> Option<f64> {
    scope.to_number(value).ok()
}

pub(crate) fn global_number(scope: &mut Scope<'_>, key: &str) -> Option<f64> {
    let global = scope.global();
    let value = get(scope, &global, key);
    number_or(scope, &value)
}

pub(crate) fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, arity: u32, f: NativeFn) {
    let function = scope.function(name, arity, f);
    set(scope, object, name, function);
}

pub(crate) fn bind_if_not_callable(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    arity: u32,
    f: NativeFn,
) {
    let current = scope.get(object, name).ok();
    if !current.is_some_and(|current| scope.is_function(&current)) {
        bind(scope, object, name, arity, f);
    }
}

pub(crate) fn new_instance<T: core::any::Any>(
    scope: &mut Scope<'_>,
    new_target: &Value,
    data: T,
    methods: &[(&str, u32, NativeFn)],
) -> (Value, bool) {
    let proto = scope
        .get(new_target, "prototype")
        .ok()
        .filter(Value::is_object);
    let object = scope.new_host_object(proto.as_ref(), data);
    let Some(proto) = proto else {
        return (object, false);
    };
    if !matches!(scope.has_property(&proto, "observe"), Ok(true)) {
        for &(name, arity, f) in methods {
            bind(scope, &proto, name, arity, f);
        }
    }
    (object, true)
}

pub(crate) fn report_error(js: Js, scope: &mut Scope<'_>, what: &str, exception: &Value) {
    if let Ok(message) = text_of(scope, exception) {
        let mut line = format!("JS error in {what}: ").into_bytes();
        line.extend_from_slice(&message);
        ffi::log_line(js, &line);
    }
}
