//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{ContextHandle, Scope, Value};

pub type PortTransferPrepareFn =
    fn(&mut Scope<'_>, &Value, &Value, ContextHandle) -> Result<(Value, Value), Value>;

pub struct JsEventsLinks {
    pub dom_exception: fn(&mut Scope<'_>, &[u8], i32, &[u8]) -> Value,
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
    pub port_transfer_commit: fn(&mut Scope<'_>, &Value, &Value),
    pub port_transfer_prepare: PortTransferPrepareFn,
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

pub(crate) fn nsr_port_transfer_commit(
    scope: &mut Scope<'_>,
    old_ports: &Value,
    new_ports: &Value,
) {
    (js_workers_links().port_transfer_commit)(scope, old_ports, new_ports)
}

pub(crate) fn nsr_port_transfer_prepare(
    scope: &mut Scope<'_>,
    transfer: &Value,
    source_port: &Value,
    realm: ContextHandle,
) -> Result<(Value, Value), Value> {
    (js_workers_links().port_transfer_prepare)(scope, transfer, source_port, realm)
}
