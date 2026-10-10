//! Southstar — input events the shell delivers to the page: mouse and pointer events with iframe coordinates, wheel and touch, clipboard and drag events with their DataTransfer, and the WPT test-driver hooks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod mouse;
mod scroll_touch;
mod transfer;
mod wpt;

use std::cell::RefCell;
use std::collections::HashMap;

use southstar_dom::Node;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::Js;

pub(crate) type Element = Node<'static>;

pub(crate) const PROPERTY: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

#[derive(Clone, Copy, Default)]
pub(crate) struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct Pointer {
    pub client: (f64, f64),
    pub page: (f64, f64),
    pub button: i32,
    pub buttons: i32,
    pub modifiers: Modifiers,
}

#[derive(Default)]
struct Page {
    last_mouse: [Option<(f64, f64)>; 2],
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Page>> = RefCell::new(HashMap::new());
}

pub(crate) fn movement(js: Js, slot: usize, client: (f64, f64)) -> (i32, i32) {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let page = pages.entry(js).or_default();
        let delta = page.last_mouse[slot].map_or((0, 0), |(x, y)| {
            ((client.0 - x) as i32, (client.1 - y) as i32)
        });
        page.last_mouse[slot] = Some(client);
        delta
    })
}

pub(crate) fn teardown(js: Js) {
    PAGES.with(|pages| pages.borrow_mut().remove(&js));
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn put(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.define(object, key, value, PROPERTY);
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn number_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> f64 {
    scope.to_number(&arg(args, index)).unwrap_or(0.0)
}

pub(crate) fn int_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> i32 {
    scope.to_int32(&arg(args, index)).unwrap_or(0)
}
