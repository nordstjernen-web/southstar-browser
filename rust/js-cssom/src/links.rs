//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

pub struct JsElementsLinks {
    pub js_element_is_rendered: fn(&mut Scope<'_>, &Value) -> bool,
}

static JS_ELEMENTS_LINKS: OnceLock<JsElementsLinks> = OnceLock::new();

pub fn set_js_elements_links(links: JsElementsLinks) {
    let _ = JS_ELEMENTS_LINKS.set(links);
}

fn js_elements_links() -> &'static JsElementsLinks {
    JS_ELEMENTS_LINKS
        .get()
        .expect("southstar-js-elements installs its links before the first page")
}

pub struct JsEventsLinks {
    pub event_new: fn(&mut Scope<'_>) -> Result<Value, Value>,
}

static JS_EVENTS_LINKS: OnceLock<JsEventsLinks> = OnceLock::new();

pub fn set_js_events_links(links: JsEventsLinks) {
    let _ = JS_EVENTS_LINKS.set(links);
}

fn js_events_links() -> &'static JsEventsLinks {
    JS_EVENTS_LINKS
        .get()
        .expect("southstar-js-events installs its links before the first page")
}

pub(crate) fn nsr_js_element_is_rendered(scope: &mut Scope<'_>, value: &Value) -> bool {
    (js_elements_links().js_element_is_rendered)(scope, value)
}

pub(crate) fn nsr_event_new(scope: &mut Scope<'_>) -> Result<Value, Value> {
    (js_events_links().event_new)(scope)
}

pub struct JsWorkersLinks {
    pub port_remove_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub port_add_event_listener: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_WORKERS_LINKS: OnceLock<JsWorkersLinks> = OnceLock::new();

pub fn set_js_workers_links(links: JsWorkersLinks) {
    let _ = JS_WORKERS_LINKS.set(links);
}

fn js_workers_links() -> &'static JsWorkersLinks {
    JS_WORKERS_LINKS
        .get()
        .expect("southstar-js-workers installs its links before the first page")
}

pub(crate) fn nsr_port_remove_event_listener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().port_remove_event_listener)(scope, this, args)
}

pub(crate) fn nsr_port_add_event_listener(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_workers_links().port_add_event_listener)(scope, this, args)
}
