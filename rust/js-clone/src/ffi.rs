//! Southstar — structured cloning's and the worker wire graph's nsr_ entry points, and the binding and canvas calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_bind::HostKind;
use southstar_js_bind::host;
use southstar_js_engine::{ContextHandle, Scope, Value};

const DATA_CLONE_ERR: i32 = 25;
const HIDDEN_IMAGEDATA: i32 = 4;

unsafe extern "Rust" {
    safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    safe fn nsr_image_bitmap_is(value: &Value) -> bool;
    safe fn nsr_canvas_clone_object(scope: &mut Scope<'_>, value: &Value) -> Result<Value, Value>;
    safe fn nsr_canvas_hidden_is(value: &Value, kind: i32) -> bool;
    safe fn nsr_canvas_hidden_get(
        scope: &mut Scope<'_>,
        object: &Value,
        key: &str,
    ) -> Result<Value, Value>;
    safe fn nsr_port_transfer_prepare(
        scope: &mut Scope<'_>,
        transfer: &Value,
        source_port: &Value,
        realm: ContextHandle,
    ) -> Result<(Value, Value), Value>;
    safe fn nsr_port_transfer_commit(scope: &mut Scope<'_>, old_ports: &Value, new_ports: &Value);
}

pub(crate) fn port_transfer_prepare(
    scope: &mut Scope<'_>,
    transfer: &Value,
) -> Result<(Value, Value), Value> {
    nsr_port_transfer_prepare(
        scope,
        transfer,
        &Value::undefined(),
        ContextHandle::default(),
    )
}

pub(crate) fn port_transfer_commit(scope: &mut Scope<'_>, old_ports: &Value, new_ports: &Value) {
    nsr_port_transfer_commit(scope, old_ports, new_ports);
}

pub(crate) fn data_clone_error(scope: &mut Scope<'_>) -> Value {
    nsr_dom_exception(
        scope,
        b"DataCloneError",
        DATA_CLONE_ERR,
        b"value could not be cloned.",
    )
}

pub(crate) fn is_host_object(value: &Value) -> bool {
    host::kind_of(value).is_some()
}

pub(crate) fn is_element(value: &Value) -> bool {
    southstar_js_runtime::api::unwrap_element(value).is_some()
}

pub(crate) fn is_port(_scope: &Scope<'_>, value: &Value) -> bool {
    host::is(value, HostKind::MessagePort)
}

pub(crate) fn is_image_bitmap(value: &Value) -> bool {
    nsr_image_bitmap_is(value)
}

pub(crate) fn clone_canvas_object(
    scope: &mut Scope<'_>,
    value: &Value,
) -> Option<Result<Value, Value>> {
    match nsr_canvas_clone_object(scope, value) {
        Ok(v) if v.is_undefined() => None,
        other => Some(other),
    }
}

pub(crate) fn is_image_data(value: &Value) -> bool {
    nsr_canvas_hidden_is(value, HIDDEN_IMAGEDATA)
}

pub(crate) fn hidden_get(scope: &mut Scope<'_>, value: &Value, key: &core::ffi::CStr) -> Value {
    nsr_canvas_hidden_get(scope, value, key.to_str().unwrap_or_default())
        .unwrap_or_else(|_| Value::undefined())
}

#[unsafe(no_mangle)]
pub fn nsr_structured_clone_transfer(
    scope: &mut Scope<'_>,
    value: &Value,
    transfer: &Value,
    seed_from: &Value,
    seed_to: &Value,
) -> Result<Value, Value> {
    crate::clone_transfer(scope, value, transfer, (seed_from, seed_to))
}

#[unsafe(no_mangle)]
pub fn nsr_window_structured_clone(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::structured_clone(scope, this, args)
}
