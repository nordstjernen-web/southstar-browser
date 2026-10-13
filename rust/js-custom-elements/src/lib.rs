//! Southstar — custom elements: the customElements registry, element upgrades and the lifecycle callbacks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod reactions;
mod registry;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use southstar_dom::Node;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::Js;

pub(crate) type JsResult<T = Value> = Result<T, Value>;
pub(crate) type Element = Node<'static>;

pub(crate) const WRITABLE_CONFIGURABLE: Attributes = Attributes::METHOD;
pub(crate) const MAX_DEPTH: i32 = 512;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct Key {
    pub name: String,
    pub document: usize,
}

#[derive(Default)]
pub(crate) struct Page {
    pub registry: RefCell<Vec<(Key, Value)>>,
    pub pending: RefCell<HashMap<Key, Vec<Value>>>,
    pub under_construction: RefCell<HashSet<usize>>,
    pub upgrading: RefCell<Option<Value>>,
    pub in_attr_callback: Cell<i32>,
}

impl Page {
    pub fn lookup(&self, key: &Key) -> Option<Value> {
        self.registry
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    pub fn name_of(&self, ctor: &Value) -> Option<String> {
        self.registry
            .borrow()
            .iter()
            .find(|(_, v)| v.same_object(ctor))
            .map(|(k, _)| k.name.clone())
    }

    pub fn is_empty(&self) -> bool {
        self.registry.borrow().is_empty()
    }

    pub fn insert(&self, key: Key, ctor: Value) {
        let mut registry = self.registry.borrow_mut();
        let old = match registry.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => Some(std::mem::replace(&mut slot.1, ctor)),
            None => {
                registry.push((key, ctor));
                None
            }
        };
        drop(registry);
        drop(old);
    }

    pub fn reset(&self) {
        let registry = std::mem::take(&mut *self.registry.borrow_mut());
        let pending = std::mem::take(&mut *self.pending.borrow_mut());
        self.under_construction.borrow_mut().clear();
        self.in_attr_callback.set(0);
        drop(registry);
        drop(pending);
    }
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
    if let Some(page) = &removed {
        page.reset();
        let upgrading = page.upgrading.borrow_mut().take();
        drop(upgrading);
    }
    drop(removed);
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

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn name_valid(bytes: &[u8]) -> bool {
    let Some(&first) = bytes.first() else {
        return false;
    };
    first.is_ascii_lowercase()
        && bytes
            .iter()
            .all(|&c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-._".contains(&c))
        && bytes.contains(&b'-')
}
