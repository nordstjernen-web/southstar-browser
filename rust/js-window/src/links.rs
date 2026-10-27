//! Southstar — the entry points of the crates above this one that it calls back into, as typed tables those crates install once before the first page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use southstar_js_engine::{ContextHandle, Scope, Value};

pub type PortTransferPrepareFn =
    fn(&mut Scope<'_>, &Value, &Value, ContextHandle) -> Result<(Value, Value), Value>;

pub struct JsDispatchLinks {
    pub target_dispatch_with_event: fn(&mut Scope<'_>, &Value, &str, &Value),
    pub target_make_event: fn(&mut Scope<'_>, &Value, &str) -> Value,
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

pub(crate) fn nsr_target_dispatch_with_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    event: &Value,
) {
    (js_dispatch_links().target_dispatch_with_event)(scope, target, kind, event)
}

pub(crate) fn nsr_target_make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    (js_dispatch_links().target_make_event)(scope, target, kind)
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
