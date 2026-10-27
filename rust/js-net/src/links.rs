//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

pub type JsFormDataSerializeFn = fn(&mut Scope<'_>, &Value) -> (Vec<u8>, Vec<u8>);

pub struct JsDispatchLinks {
    pub xhr_fire_progress_event: fn(&mut Scope<'_>, &Value, &str, f64, f64, bool),
    pub target_fire_event: fn(&mut Scope<'_>, &Value, &str),
    pub target_dispatch_event: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
}

static JS_DISPATCH_LINKS: OnceLock<JsDispatchLinks> = OnceLock::new();

pub fn set_js_dispatch_links(links: JsDispatchLinks) {
    let _ = JS_DISPATCH_LINKS.set(links);
}

fn js_dispatch_links() -> &'static JsDispatchLinks {
    JS_DISPATCH_LINKS
        .get()
        .expect("southstar-js-dispatch installs its links before the first page")
}

pub struct JsEventsLinks {
    pub event_adopt_interface: fn(&mut Scope<'_>, &Value, &str),
    pub event_define_cancel_bubble: fn(&mut Scope<'_>, &Value),
    pub event_new: fn(&mut Scope<'_>) -> Result<Value, Value>,
    pub make_abort_error: fn(&mut Scope<'_>) -> Value,
    pub dom_exception: fn(&mut Scope<'_>, &[u8], i32, &[u8]) -> Value,
    pub event_composed_path: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub event_stop_immediate: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub event_stop_propagation: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    pub event_prevent_default: fn(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
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

pub struct JsFormsLinks {
    pub js_form_data_serialize: JsFormDataSerializeFn,
    pub net_install_form_data: fn(&mut Scope<'_>, &Value),
}

static JS_FORMS_LINKS: OnceLock<JsFormsLinks> = OnceLock::new();

pub fn set_js_forms_links(links: JsFormsLinks) {
    let _ = JS_FORMS_LINKS.set(links);
}

fn js_forms_links() -> &'static JsFormsLinks {
    JS_FORMS_LINKS
        .get()
        .expect("southstar-js-forms installs its links before the first page")
}

pub struct JsWorkersLinks {
    pub net_install_ports: fn(&mut Scope<'_>, &Value),
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

pub(crate) fn nsr_xhr_fire_progress_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    loaded: f64,
    total: f64,
    length_computable: bool,
) {
    (js_dispatch_links().xhr_fire_progress_event)(
        scope,
        target,
        kind,
        loaded,
        total,
        length_computable,
    )
}

pub(crate) fn nsr_target_fire_event(scope: &mut Scope<'_>, obj: &Value, kind: &str) {
    (js_dispatch_links().target_fire_event)(scope, obj, kind)
}

pub(crate) fn nsr_event_adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    (js_events_links().event_adopt_interface)(scope, event, iface)
}

pub(crate) fn nsr_event_define_cancel_bubble(scope: &mut Scope<'_>, event: &Value) {
    (js_events_links().event_define_cancel_bubble)(scope, event)
}

pub(crate) fn nsr_event_new(scope: &mut Scope<'_>) -> Result<Value, Value> {
    (js_events_links().event_new)(scope)
}

pub(crate) fn nsr_make_abort_error(scope: &mut Scope<'_>) -> Value {
    (js_events_links().make_abort_error)(scope)
}

pub(crate) fn nsr_dom_exception(
    scope: &mut Scope<'_>,
    name: &[u8],
    code: i32,
    message: &[u8],
) -> Value {
    (js_events_links().dom_exception)(scope, name, code, message)
}

pub(crate) fn nsr_js_form_data_serialize(scope: &mut Scope<'_>, fd: &Value) -> (Vec<u8>, Vec<u8>) {
    (js_forms_links().js_form_data_serialize)(scope, fd)
}

pub(crate) fn nsr_net_install_form_data(scope: &mut Scope<'_>, global: &Value) {
    (js_forms_links().net_install_form_data)(scope, global)
}

pub(crate) fn nsr_net_install_ports(scope: &mut Scope<'_>, global: &Value) {
    (js_workers_links().net_install_ports)(scope, global)
}

#[allow(non_snake_case)]
pub(crate) fn nsr_target_dispatchEvent(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_dispatch_links().target_dispatch_event)(scope, this, args)
}

pub(crate) fn nsr_event_composed_path(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_events_links().event_composed_path)(scope, this, args)
}

pub(crate) fn nsr_event_stop_immediate(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_events_links().event_stop_immediate)(scope, this, args)
}

pub(crate) fn nsr_event_stop_propagation(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_events_links().event_stop_propagation)(scope, this, args)
}

pub(crate) fn nsr_event_prevent_default(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    (js_events_links().event_prevent_default)(scope, this, args)
}
