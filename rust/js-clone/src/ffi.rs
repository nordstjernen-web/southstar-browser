//! Southstar — the C ABI of structured cloning and the worker wire graph, as declared in src/js_internal.h, and the js.c and canvas calls behind them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

const DATA_CLONE_ERR: c_int = 25;
const HIDDEN_IMAGEDATA: c_int = 4;

unsafe extern "C" {
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_js_is_host_object(v: JSValue) -> c_int;
    fn ns_unwrap_element(v: JSValue) -> *const c_void;
    fn ns_worker_transfer_is_port(ctx: *mut JSContext, v: JSValue) -> c_int;
    fn ns_image_bitmap_is(v: JSValue) -> c_int;
    fn ns_canvas_clone_object(ctx: *mut JSContext, v: JSValue) -> JSValue;
    fn ns_hidden_is(v: JSValue, kind: c_int) -> c_int;
    fn ns_hget(ctx: *mut JSContext, obj: JSValue, key: *const c_char) -> JSValue;
}

pub(crate) fn data_clone_error(scope: &mut Scope<'_>) -> Value {
    let ctx = quickjs::raw_context(scope);
    unsafe {
        ns_throw_dom_exception(
            ctx,
            c"DataCloneError".as_ptr(),
            DATA_CLONE_ERR,
            c"value could not be cloned.".as_ptr(),
        )
    };
    quickjs::take_exception(scope)
}

pub(crate) fn is_host_object(value: &Value) -> bool {
    unsafe { ns_js_is_host_object(quickjs::raw(value)) != 0 }
}

pub(crate) fn is_element(value: &Value) -> bool {
    !unsafe { ns_unwrap_element(quickjs::raw(value)) }.is_null()
}

pub(crate) fn is_port(scope: &Scope<'_>, value: &Value) -> bool {
    let ctx = quickjs::raw_context(scope);
    unsafe { ns_worker_transfer_is_port(ctx, quickjs::raw(value)) != 0 }
}

pub(crate) fn is_image_bitmap(value: &Value) -> bool {
    unsafe { ns_image_bitmap_is(quickjs::raw(value)) != 0 }
}

pub(crate) fn clone_canvas_object(
    scope: &mut Scope<'_>,
    value: &Value,
) -> Option<Result<Value, Value>> {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_canvas_clone_object(ctx, quickjs::raw(value)) };
    let result = quickjs::checked(scope, unsafe { quickjs::take_value(scope, raw) });
    match result {
        Ok(v) if v.is_undefined() => None,
        other => Some(other),
    }
}

pub(crate) fn is_image_data(value: &Value) -> bool {
    unsafe { ns_hidden_is(quickjs::raw(value), HIDDEN_IMAGEDATA) != 0 }
}

pub(crate) fn hidden_get(scope: &mut Scope<'_>, value: &Value, key: &core::ffi::CStr) -> Value {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_hget(ctx, quickjs::raw(value), key.as_ptr()) };
    quickjs::checked(scope, unsafe { quickjs::take_value(scope, raw) })
        .unwrap_or_else(|_| Value::undefined())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_sc_fail(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let error = data_clone_error(scope);
            quickjs::result_raw(scope, Err(error))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_sc_buffer_detached(ctx: *mut JSContext, buffer: JSValue) -> c_int {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let buffer = quickjs::borrow_value(scope, buffer);
            c_int::from(crate::buffer_detached(scope, &buffer))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_structured_clone_transfer(
    ctx: *mut JSContext,
    value: JSValue,
    transfer: JSValue,
    seed_from: JSValue,
    seed_to: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let transfer = quickjs::take_value(scope, transfer);
            let value = quickjs::borrow_value(scope, value);
            let seed_from = quickjs::borrow_value(scope, seed_from);
            let seed_to = quickjs::borrow_value(scope, seed_to);
            let result = crate::clone_transfer(scope, &value, &transfer, (&seed_from, &seed_to));
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_wire_encode_value(
    ctx: *mut JSContext,
    value: JSValue,
    ports: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            let ports = quickjs::borrow_value(scope, ports);
            let result = crate::wire::encode_value(scope, &value, &ports);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_wire_decode_value(
    ctx: *mut JSContext,
    wire: JSValue,
    ports: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let wire = quickjs::borrow_value(scope, wire);
            let ports = quickjs::borrow_value(scope, ports);
            let result = crate::wire::decode_value(scope, &wire, &ports);
            quickjs::result_raw(scope, result)
        })
    }
}
