//! Southstar — the C ABI of getContext(), the context attributes, PNG export and DOMMatrix results, and the WebGL, WebGPU and DOM calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_double, c_int, c_uint, c_void};

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use super::cairo::Surface;
use super::state::{CanvasState, Node, NsJs, NsNode, js_of};

#[repr(C)]
struct GByteArray {
    data: *mut u8,
    len: c_uint,
}

unsafe extern "C" {
    fn ns_unwrap_element(val: JSValue) -> *const NsNode;
    fn ns_js_is_worker(js: *const NsJs) -> c_int;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_webgl_get_context(
        ctx: *mut JSContext,
        js: *mut NsJs,
        canvas_obj: JSValue,
        el: *const NsNode,
        version: c_int,
        options: JSValue,
    ) -> JSValue;
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;
}

#[cfg(feature = "webgpu")]
unsafe extern "C" {
    fn ns_webgpu_get_context(
        ctx: *mut JSContext,
        js: *mut NsJs,
        canvas_obj: JSValue,
        el: *const NsNode,
    ) -> JSValue;
}

fn state(st: *mut CanvasState) -> &'static mut CanvasState {
    unsafe { &mut *st }
}

pub(crate) fn ctx2d_of(scope: &Scope<'_>, st: *mut CanvasState) -> Option<Value> {
    let raw = state(st).ctx2d;
    quickjs::raw_is_object(raw).then(|| unsafe { quickjs::borrow_value(scope, raw) })
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

pub(crate) fn surface_of(st: *mut CanvasState) -> Option<Surface> {
    unsafe { Surface::from_borrowed(state(st).surf) }
}

pub(crate) fn attach_ctx2d(scope: &Scope<'_>, st: *mut CanvasState, obj: &Value, kind: i32) {
    let st = state(st);
    let js = js_of(scope);
    st.ctx2d = quickjs::into_raw(obj.clone());
    st.jsctx = unsafe { ns_js_main_context(js.ptr()) };
    st.rt = quickjs::runtime(scope);
    st.context_kind = kind;
}

pub(crate) fn is_worker(scope: &Scope<'_>) -> bool {
    let js = js_of(scope);
    !js.is_null() && unsafe { ns_js_is_worker(js.ptr()) } != 0
}

pub(crate) fn unwrap_element(value: &Value) -> Node {
    Node::from_addr(unsafe { ns_unwrap_element(quickjs::raw(value)) } as usize)
}

pub(crate) fn webgl_context(
    scope: &mut Scope<'_>,
    canvas: &Value,
    el: Node,
    version: i32,
    options: &Value,
) -> Result<Value, Value> {
    let js = js_of(scope);
    let raw = unsafe {
        ns_webgl_get_context(
            quickjs::raw_context(scope),
            js.ptr(),
            quickjs::raw(canvas),
            el.ptr(),
            version,
            quickjs::raw(options),
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

#[cfg(feature = "webgpu")]
pub(crate) fn webgpu_context(
    scope: &mut Scope<'_>,
    canvas: &Value,
    el: Node,
) -> Result<Option<Value>, Value> {
    let js = js_of(scope);
    let raw = unsafe {
        ns_webgpu_get_context(
            quickjs::raw_context(scope),
            js.ptr(),
            quickjs::raw(canvas),
            el.ptr(),
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value).map(Some)
}

#[cfg(not(feature = "webgpu"))]
pub(crate) fn webgpu_context(
    _scope: &mut Scope<'_>,
    _canvas: &Value,
    _el: Node,
) -> Result<Option<Value>, Value> {
    Ok(None)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_getContext(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe {
        quickjs::call_native(
            ctx,
            this_val,
            argc,
            argv,
            crate::context::element_get_context,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_png_write(
    closure: *mut c_void,
    data: *const u8,
    length: c_uint,
) -> c_int {
    unsafe { g_byte_array_append(closure.cast(), data, length) };
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dommatrix_make(
    ctx: *mut JSContext,
    a: c_double,
    b: c_double,
    c: c_double,
    d: c_double,
    e: c_double,
    f: c_double,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(crate::context::dommatrix(scope, [a, b, c, d, e, f]))
        })
    }
}
