//! Southstar — getContext()'s view of the drawing state, PNG export helpers, and the WebGL, WebGPU and DOM calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use super::imports;
use super::state::{CanvasState, Node, js_of};

pub(crate) fn ctx2d_of(st: &CanvasState) -> Option<Value> {
    st.ctx2d()
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: i32,
    message: &CStr,
) -> Value {
    imports::nsr_dom_exception(scope, name.to_bytes(), code, message.to_bytes())
}

pub(crate) fn is_worker(scope: &Scope<'_>) -> bool {
    let js = js_of(scope);
    api::is_worker(js.page())
}

pub(crate) fn unwrap_element(value: &Value) -> Node {
    Node::of(southstar_js_runtime::api::unwrap_element(value))
}

pub(crate) fn webgl_context(
    scope: &mut Scope<'_>,
    canvas: &Value,
    el: Node,
    version: i32,
    options: &Value,
) -> Result<Value, Value> {
    let js = js_of(scope);
    imports::nsr_webgl_get_context(scope, js.page(), canvas, el.key(), version, options)
}

#[cfg(feature = "webgpu")]
pub(crate) fn webgpu_context(
    scope: &mut Scope<'_>,
    canvas: &Value,
    el: Node,
) -> Result<Option<Value>, Value> {
    Ok(Some(crate::links::webgpu_get_context(
        scope,
        canvas,
        el.key(),
    )))
}

#[cfg(not(feature = "webgpu"))]
pub(crate) fn webgpu_context(
    _scope: &mut Scope<'_>,
    _canvas: &Value,
    _el: Node,
) -> Result<Option<Value>, Value> {
    Ok(None)
}
