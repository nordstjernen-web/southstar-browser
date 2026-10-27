//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{Scope, Value};

pub struct JsDispatchLinks {
    pub target_dispatch_with_event: fn(&mut Scope<'_>, &Value, &str, &Value),
    pub target_make_event: fn(&mut Scope<'_>, &Value, &str) -> Value,
    pub target_fire_event: fn(&mut Scope<'_>, &Value, &str),
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

pub(crate) fn nsr_target_dispatch_with_event(
    scope: &mut Scope<'_>,
    obj: &Value,
    kind: &str,
    ev: &Value,
) {
    (js_dispatch_links().target_dispatch_with_event)(scope, obj, kind, ev)
}

pub(crate) fn nsr_target_make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    (js_dispatch_links().target_make_event)(scope, target, kind)
}

pub(crate) fn nsr_target_fire_event(scope: &mut Scope<'_>, obj: &Value, kind: &str) {
    (js_dispatch_links().target_fire_event)(scope, obj, kind)
}

pub(crate) fn nsr_event_adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    (js_events_links().event_adopt_interface)(scope, event, iface)
}
