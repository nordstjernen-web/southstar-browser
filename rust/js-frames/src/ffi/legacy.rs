//! Southstar — the frame bindings' C-ABI shims over raw QuickJS, kept for the crates that still call them that way.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use southstar_dom::NsNode;
use southstar_glib::GBoolean;
use southstar_js_engine::Value;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

use super::NsJs;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_cross_origin_window(
    ctx: *mut JSContext,
    target: JSValue,
) -> JSValue {
    if ctx.is_null() {
        return quickjs::into_raw(Value::null());
    }
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let target = quickjs::take_value(scope, target);
            quickjs::into_raw(super::nsr_iframe_cross_origin_window(scope, &target))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_realm_window(js: *mut NsJs, frame: *const NsNode) -> JSValue {
    quickjs::into_raw(unsafe { super::nsr_js_frame_realm_window(js, frame) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_realm_window(
    ctx: *mut JSContext,
    this_val: JSValue,
    node: *mut NsNode,
) -> JSValue {
    if ctx.is_null() || node.is_null() {
        return quickjs::into_raw(Value::null());
    }
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let element = quickjs::borrow_value(scope, this_val);
            quickjs::into_raw(super::nsr_iframe_realm_window(scope, &element, node))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_frame_node(js: *mut NsJs, window: JSValue) -> *mut NsNode {
    if !quickjs::raw_is_object(window) {
        return core::ptr::null_mut();
    }
    unsafe { quickjs::with_raw(window, |window| super::nsr_window_frame_node(js, window)) }
}

unsafe fn getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    f: fn(&mut southstar_js_engine::Scope<'_>, &Value) -> Result<Value, Value>,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let result = f(scope, &this);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn ns_element_get_contentDocument(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, super::nsr_element_get_contentDocument) }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn ns_element_get_contentWindow(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, super::nsr_element_get_contentWindow) }
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub unsafe extern "C" fn ns_element_getSVGDocument(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, super::nsr_element_getSVGDocument) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_child_frame_window(
    ctx: *mut JSContext,
    doc: *mut NsNode,
    index: u32,
    name: *const c_char,
    raw: GBoolean,
) -> JSValue {
    let name = (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) });
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let result = super::nsr_window_child_frame_window(scope, doc, index, name, raw != 0);
            quickjs::result_raw(scope, result)
        })
    }
}
