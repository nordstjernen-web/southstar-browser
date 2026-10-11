//! Southstar — events fired at the window itself: building one, hashchange at the top-level window and dispatching at the page's top document.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashSet;

use southstar_dom::{Kind, Node};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};

thread_local! {
    static IN_HASHCHANGE: RefCell<HashSet<Js>> = RefCell::new(HashSet::new());
}

pub(crate) fn make_window_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    let event = ffi::make_event(scope, kind);
    let global = scope.global();
    let _ = scope.set(&event, "target", global);
    let _ = scope.set(&event, "bubbles", Value::boolean(false));
    let _ = scope.set(&event, "cancelable", Value::boolean(false));
    event
}

pub(crate) fn top_document(doc: Option<Node<'static>>) -> Option<Node<'static>> {
    let mut doc = doc;
    while let Some(parent) = doc.and_then(Node::parent) {
        let mut cursor = Some(parent);
        while let Some(current) = cursor.filter(|c| c.kind() != Kind::Document) {
            cursor = current.parent();
        }
        doc = cursor;
    }
    doc
}

fn fire_hashchange(scope: &mut Scope<'_>, js: Js, old_url: &[u8], new_url: &[u8]) {
    let global = scope.global();
    let event = make_window_event(scope, "hashchange");
    let old_url = scope.string_from_bytes(old_url);
    let _ = scope.set(&event, "oldURL", old_url);
    let new_url = scope.string_from_bytes(new_url);
    let _ = scope.set(&event, "newURL", new_url);
    if let Some(doc) = ffi::current_document(js) {
        ffi::dispatch_window_only_event(js, Some(doc), c"hashchange", event);
        return;
    }
    let handler = scope
        .get(&global, "onhashchange")
        .unwrap_or_else(|_| Value::undefined());
    if scope.is_function(&handler) {
        let _ = scope.call(&handler, &global, &[event]);
    }
}

pub(crate) fn dispatch_hashchange(js: Js, old_url: &[u8], new_url: &[u8]) {
    if js.is_null() || !ffi::has_main_context(js) {
        return;
    }
    if ffi::events_suspended(js) || IN_HASHCHANGE.with_borrow(|active| active.contains(&js)) {
        return;
    }
    IN_HASHCHANGE.with_borrow_mut(|active| active.insert(js));
    ffi::in_main_realm_scope(js, || {
        ffi::with_main_context(js, |scope| fire_hashchange(scope, js, old_url, new_url));
    });
    IN_HASHCHANGE.with_borrow_mut(|active| active.remove(&js));
}
