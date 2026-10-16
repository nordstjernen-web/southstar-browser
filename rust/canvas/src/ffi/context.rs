//! Southstar — getContext()'s view of the drawing state, PNG export helpers, and the WebGL, WebGPU and DOM calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use super::cairo::Surface;
use super::imports;
use super::state::{CanvasState, Node, js_of};

fn state(st: *mut CanvasState) -> &'static mut CanvasState {
    unsafe { &mut *st }
}

pub(crate) fn ctx2d_of(scope: &mut Scope<'_>, st: *mut CanvasState) -> Option<Value> {
    let identity = state(st).ctx2d;
    (!identity.is_null()).then(|| scope.object_from_identity(identity))
}

pub(crate) fn context_kind(st: *mut CanvasState) -> i32 {
    state(st).context_kind
}

pub(crate) fn set_context_kind(st: *mut CanvasState, kind: i32) {
    state(st).context_kind = kind;
}

pub(crate) fn origin_clean(st: *mut CanvasState) -> bool {
    state(st).origin_clean != 0
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: i32,
    message: &CStr,
) -> Value {
    imports::nsr_dom_exception(scope, name.to_bytes(), code, message.to_bytes())
}

pub(crate) fn surface_of(st: *mut CanvasState) -> Option<Surface> {
    unsafe { Surface::from_borrowed(state(st).surf) }
}

pub(crate) fn attach_ctx2d(scope: &mut Scope<'_>, st: *mut CanvasState, obj: &Value, kind: i32) {
    let st = state(st);
    let js = js_of(scope);
    if !st.ctx2d.is_null() {
        unsafe { scope.release_identity(st.ctx2d) };
    }
    st.ctx2d = scope.retain_identity(obj);
    st.jsctx = api::main_context(js.ptr());
    st.context_kind = kind;
}

pub(crate) fn is_worker(scope: &Scope<'_>) -> bool {
    let js = js_of(scope);
    api::is_worker(js.ptr())
}

pub(crate) fn unwrap_element(value: &Value) -> Node {
    let node = southstar_js_runtime::api::unwrap_element(value);
    Node::from_addr(node.map_or(0, |node| node.as_ptr() as usize))
}

pub(crate) fn webgl_context(
    scope: &mut Scope<'_>,
    canvas: &Value,
    el: Node,
    version: i32,
    options: &Value,
) -> Result<Value, Value> {
    let js = js_of(scope);
    imports::nsr_webgl_get_context(scope, js.addr(), canvas, el.addr(), version, options)
}

#[cfg(feature = "webgpu")]
pub(crate) fn webgpu_context(
    scope: &mut Scope<'_>,
    canvas: &Value,
    el: Node,
) -> Result<Option<Value>, Value> {
    Ok(Some(imports::nsr_webgpu_get_context(
        scope,
        canvas,
        el.addr(),
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
