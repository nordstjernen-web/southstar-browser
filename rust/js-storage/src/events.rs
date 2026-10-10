//! Southstar — the storage event: built on every change, queued, and delivered to the other frames of the page and then to the window.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::rc::Rc;

use southstar_dom::{Kind, Node, children, index};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Page, PendingEvent};

const DELIVERY_DEPTH_MAX: usize = 512;
const DRAIN_MAX: usize = 1000;

fn string_or_null(scope: &mut Scope<'_>, text: Option<&str>) -> Value {
    text.map_or_else(Value::null, |text| scope.string(text))
}

fn source_url(scope: &mut Scope<'_>, global: &Value, js: Js) -> String {
    let location = scope
        .get(global, "location")
        .unwrap_or_else(|_| Value::undefined());
    if location.is_object() {
        let href = scope
            .get(&location, "href")
            .unwrap_or_else(|_| Value::undefined());
        if href.is_string()
            && let Ok(href) = scope.to_string(&href)
        {
            return href;
        }
    }
    ffi::current_url(js).unwrap_or_default()
}

pub(crate) fn fire(
    scope: &mut Scope<'_>,
    storage: &Value,
    key: Option<&str>,
    old_value: Option<&str>,
    new_value: Option<&str>,
) {
    let js = ffi::js_of(scope);
    if js.is_null() || ffi::halted(js) {
        return;
    }
    let Some(page) = crate::page(js) else {
        return;
    };
    let source = scope.global();
    let url = source_url(scope, &source, js);
    let event = ffi::in_page_context(js, |scope| {
        let event = ffi::make_event(scope, "storage");
        let fields = [
            ("bubbles", Value::boolean(false)),
            ("cancelable", Value::boolean(false)),
            ("key", string_or_null(scope, key)),
            ("oldValue", string_or_null(scope, old_value)),
            ("newValue", string_or_null(scope, new_value)),
            ("url", scope.string(&url)),
            ("storageArea", storage.clone()),
        ];
        for (name, value) in fields {
            let _ = scope.set(&event, name, value);
        }
        event
    });
    page.events
        .borrow_mut()
        .push_back(PendingEvent { event, source });
}

fn call_handler(scope: &mut Scope<'_>, js: Js, handler: &Value, window: &Value, event: &Value) {
    if !scope.is_function(handler) {
        return;
    }
    if let Err(error) = scope.call(handler, window, std::slice::from_ref(event))
        && let Ok(message) = scope.to_string(&error)
    {
        ffi::log_line(js, &format!("JS error in onstorage: {message}"));
    }
}

fn fire_window_property(scope: &mut Scope<'_>, js: Js, window: &Value, event: &Value) {
    let key = scope.string("onstorage");
    if let Ok(Some(slot)) = scope.own_property(window, &key)
        && !slot.accessor
    {
        call_handler(scope, js, &slot.value, window, event);
    }
}

fn fire_body_attribute(
    scope: &mut Scope<'_>,
    js: Js,
    window: &Value,
    body: Node<'_>,
    event: &Value,
) {
    let Some(source) = body.attr(c"onstorage") else {
        return;
    };
    if source.is_empty() || !ffi::inline_handlers_allowed(js) {
        return;
    }
    let code = format!(
        "(function(__nsStorageEvt){{with(this){{(function(event){{\n{}\n}}).call(this,__nsStorageEvt);}}}})",
        source.to_string_lossy()
    );
    if let Ok(handler) = scope.eval_script(&code, "<inline>") {
        call_handler(scope, js, &handler, window, event);
    }
}

fn deliver_to_frames(
    scope: &mut Scope<'_>,
    js: Js,
    node: Node<'_>,
    pending: &PendingEvent,
    depth: usize,
) {
    if depth > DELIVERY_DEPTH_MAX {
        return;
    }
    if node.kind() == Kind::Document
        && let Some(frame) = node.parent()
    {
        let window = ffi::frame_realm_window(scope, js, frame);
        let is_source = window.is_object() && window.same_object(&pending.source);
        if !is_source {
            let body = index::find_first_element(node, c"body");
            if window.is_object() {
                let _ = scope.set(&pending.event, "target", window.clone());
                fire_window_property(scope, js, &window, &pending.event);
                if let Some(body) = body {
                    fire_body_attribute(scope, js, &window, body, &pending.event);
                }
            } else if let Some(body) = body {
                ffi::fire_element_handlers(scope, js, body, "storage", &pending.event);
            }
        }
    }
    for child in children(node) {
        deliver_to_frames(scope, js, child, pending, depth + 1);
    }
}

fn deliver(js: Js, pending: &PendingEvent) {
    ffi::in_page_context(js, |scope| {
        if let Some(doc) = ffi::current_document(js) {
            deliver_to_frames(scope, js, doc, pending, 0);
        }
        let global = scope.global();
        let _ = scope.set(&pending.event, "target", global);
        ffi::dispatch_window_event(scope, js, "storage", &pending.event);
    });
}

pub(crate) fn drain_in_main_realm(js: Js, page: &Rc<Page>) {
    for _ in 0..DRAIN_MAX {
        let next = page.events.borrow_mut().pop_front();
        let Some(pending) = next else {
            break;
        };
        deliver(js, &pending);
        drop(pending);
        ffi::drain_microtasks(js);
    }
}

pub(crate) fn drain(js: Js) {
    let Some(page) = crate::page(js) else {
        return;
    };
    if page.draining.get() || page.events.borrow().is_empty() {
        return;
    }
    if ffi::halted(js) || ffi::in_frame_load(js) {
        return;
    }
    page.draining.set(true);
    ffi::in_main_realm(js, || drain_in_main_realm(js, &page));
    page.draining.set(false);
}

pub(crate) fn discard(js: Js) {
    if let Some(page) = crate::page(js) {
        let events = core::mem::take(&mut *page.events.borrow_mut());
        drop(events);
    }
}
