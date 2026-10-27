//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use core::ffi::c_void;
use southstar_js_engine::{Scope, Value};

pub struct JsEventsLinks {
    pub dom_exception: fn(&mut Scope<'_>, &[u8], i32, &[u8]) -> Value,
    pub event_define_source: fn(&mut Scope<'_>, &Value, Value),
    pub event_adopt_interface: fn(&mut Scope<'_>, &Value, &str),
    pub event_mark_default_prevented: fn(&mut Scope<'_>, &Value),
    pub event_define_cancel_bubble: fn(&mut Scope<'_>, &Value),
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

pub struct JsWorkersLinks {
    pub worker_report_exception: fn(*mut c_void, &Value) -> bool,
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

pub(crate) fn nsr_dom_exception(
    scope: &mut Scope<'_>,
    name: &[u8],
    code: i32,
    message: &[u8],
) -> Value {
    (js_events_links().dom_exception)(scope, name, code, message)
}

pub(crate) fn nsr_event_define_source(scope: &mut Scope<'_>, event: &Value, source: Value) {
    (js_events_links().event_define_source)(scope, event, source)
}

pub(crate) fn nsr_event_adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    (js_events_links().event_adopt_interface)(scope, event, iface)
}

pub(crate) fn nsr_event_mark_default_prevented(scope: &mut Scope<'_>, event: &Value) {
    (js_events_links().event_mark_default_prevented)(scope, event)
}

pub(crate) fn nsr_event_define_cancel_bubble(scope: &mut Scope<'_>, event: &Value) {
    (js_events_links().event_define_cancel_bubble)(scope, event)
}

pub(crate) fn nsr_event_new(scope: &mut Scope<'_>) -> Result<Value, Value> {
    (js_events_links().event_new)(scope)
}

pub(crate) fn nsr_worker_report_exception(js: *mut c_void, exception: &Value) -> bool {
    (js_workers_links().worker_report_exception)(js, exception)
}
