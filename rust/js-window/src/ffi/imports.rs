//! Southstar — the Rust-ABI entry points of the binding crates the window bindings call, resolved at link time because those crates sit beside this one in a cyclic graph.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::NsNode;
use southstar_js_engine::{ContextHandle, Scope, Value};

use super::NsJs;

unsafe extern "Rust" {
    pub(crate) safe fn nsr_structured_clone_transfer(
        scope: &mut Scope<'_>,
        value: &Value,
        transfer: &Value,
        seed_from: &Value,
        seed_to: &Value,
    ) -> Result<Value, Value>;
    pub(crate) safe fn nsr_window_structured_clone(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    pub(crate) unsafe fn nsr_make_event(
        scope: &mut Scope<'_>,
        kind: &str,
        target: *const NsNode,
    ) -> Value;
    pub(crate) safe fn nsr_port_transfer_prepare(
        scope: &mut Scope<'_>,
        transfer: &Value,
        source_port: &Value,
        realm: ContextHandle,
    ) -> Result<(Value, Value), Value>;
    pub(crate) safe fn nsr_port_transfer_commit(
        scope: &mut Scope<'_>,
        old_ports: &Value,
        new_ports: &Value,
    );
    pub(crate) safe fn nsr_target_make_event(
        scope: &mut Scope<'_>,
        target: &Value,
        kind: &str,
    ) -> Value;
    pub(crate) safe fn nsr_target_dispatch_with_event(
        scope: &mut Scope<'_>,
        target: &Value,
        kind: &str,
        event: &Value,
    );
    pub(crate) unsafe fn nsr_js_dispatch_window_only_event(
        js: *mut NsJs,
        target_doc: *const NsNode,
        kind: &str,
        event: Value,
    ) -> bool;
}
