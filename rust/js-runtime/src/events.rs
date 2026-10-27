//! Southstar — event creation, dispatch and error reporting for the binding crates, as a typed table the dispatch and event crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_js_engine::{Scope, Value};

use crate::api::Page;

pub struct EventHooks {
    pub dispatch_event: fn(Option<Page>, Node<'_>, &str) -> (bool, bool),
    pub dispatch_built_event: fn(Option<Page>, Node<'_>, &str, Value) -> (bool, bool),
    pub dispatch_window_only_event: fn(Option<Page>, Option<Node<'_>>, &str, Value) -> bool,
    pub dispatch_resource_event: fn(Option<Page>, Node<'_>, &str) -> bool,
    pub window_for_document: fn(Option<Page>, &mut Scope<'_>, Option<Node<'_>>) -> Value,
    pub fire_element_handlers: fn(Option<Page>, Node<'_>, &str, &Value),
    pub report_uncaught: fn(Option<Page>, &Value, Option<&str>),
    pub report_handler_exception: fn(Option<Page>, &mut Scope<'_>, Option<&str>, &Value),
    pub init_flags: fn(&str, bool) -> (bool, bool),
}

impl EventHooks {
    const NONE: EventHooks = EventHooks {
        dispatch_event: |_, _, _| (false, false),
        dispatch_built_event: |_, _, _, _| (false, false),
        dispatch_window_only_event: |_, _, _, _| false,
        dispatch_resource_event: |_, _, _| false,
        window_for_document: |_, _, _| Value::null(),
        fire_element_handlers: |_, _, _, _| {},
        report_uncaught: |_, _, _| {},
        report_handler_exception: |_, _, _, _| {},
        init_flags: |_, _| (false, false),
    };
}

crate::api::hook_table!(EventHooks, EVENTS, set_event_hooks, hooks);

pub struct EventObjectHooks {
    pub make_event: fn(&mut Scope<'_>, &str, Option<Node<'_>>) -> Value,
}

impl EventObjectHooks {
    const NONE: EventObjectHooks = EventObjectHooks {
        make_event: |_, _, _| Value::null(),
    };
}

crate::api::hook_table!(
    EventObjectHooks,
    EVENT_OBJECTS,
    set_event_object_hooks,
    object_hooks
);

pub fn dispatch_event(js: Option<Page>, target: Node<'_>, kind: &str) -> (bool, bool) {
    (hooks().dispatch_event)(js, target, kind)
}

pub fn dispatch_built_event(
    js: Option<Page>,
    target: Node<'_>,
    kind: &str,
    event: Value,
) -> (bool, bool) {
    (hooks().dispatch_built_event)(js, target, kind, event)
}

pub fn dispatch_window_only_event(
    js: Option<Page>,
    document: Option<Node<'_>>,
    kind: &str,
    event: Value,
) -> bool {
    (hooks().dispatch_window_only_event)(js, document, kind, event)
}

pub fn dispatch_resource_event(js: Option<Page>, target: Node<'_>, kind: &str) -> bool {
    (hooks().dispatch_resource_event)(js, target, kind)
}

pub fn window_for_document(
    js: Option<Page>,
    scope: &mut Scope<'_>,
    document: Option<Node<'_>>,
) -> Value {
    (hooks().window_for_document)(js, scope, document)
}

pub fn fire_element_handlers(js: Option<Page>, element: Node<'_>, kind: &str, event: &Value) {
    (hooks().fire_element_handlers)(js, element, kind, event)
}

pub fn report_uncaught(js: Option<Page>, exception: &Value, origin: Option<&str>) {
    (hooks().report_uncaught)(js, exception, origin)
}

pub fn report_handler_exception(
    js: Option<Page>,
    scope: &mut Scope<'_>,
    kind: Option<&str>,
    exception: &Value,
) {
    (hooks().report_handler_exception)(js, scope, kind, exception)
}

pub fn init_flags(kind: &str, at_document: bool) -> (bool, bool) {
    (hooks().init_flags)(kind, at_document)
}

pub fn make_event(scope: &mut Scope<'_>, kind: &str, target: Option<Node<'_>>) -> Value {
    (object_hooks().make_event)(scope, kind, target)
}
