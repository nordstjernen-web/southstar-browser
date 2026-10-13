//! Southstar — the C ABI shims of the binding helpers for crates still on raw QuickJS; each goes once its last caller calls the neutral API.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::borrow::Cow;
use std::ffi::CStr;

use core::ffi::{c_char, c_int};

use southstar_js_engine::quickjs::{self, JSCFunction, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::host::{self, HostKind};
use crate::natives;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_is_host_object(value: JSValue) -> c_int {
    c_int::from(unsafe { quickjs::with_raw(value, host::kind_of) }.is_some())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_value_is_form_data(_ctx: *mut JSContext, value: JSValue) -> c_int {
    let form_data = unsafe { quickjs::with_raw(value, |value| host::is(value, HostKind::FormData)) };
    c_int::from(form_data)
}

unsafe fn text<'a>(raw: *const c_char) -> Cow<'a, str> {
    if raw.is_null() {
        Cow::Borrowed("")
    } else {
        unsafe { CStr::from_ptr(raw) }.to_string_lossy()
    }
}

fn arity(argc: c_int) -> u32 {
    u32::try_from(argc).unwrap_or(0)
}

unsafe fn with_scope<R>(ctx: *mut JSContext, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
    unsafe { quickjs::with_context(ctx, f) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_obj_has_defined_own_prop(
    ctx: *mut JSContext,
    obj: JSValue,
    name: *const c_char,
) -> c_int {
    let name = unsafe { text(name) };
    unsafe {
        with_scope(ctx, |scope| {
            let object = quickjs::borrow_value(scope, obj);
            c_int::from(crate::has_defined_own(scope, &object, &name))
        })
    }
}

fn make_constructor(scope: &mut Scope<'_>, f: JSCFunction, name: &str, argc: c_int) -> Value {
    let constructor = quickjs::c_constructor_or_function(scope, name, arity(argc), f);
    let _ = crate::attach_prototype(scope, &constructor, name);
    constructor
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bind_fn(
    ctx: *mut JSContext,
    obj: JSValue,
    name: *const c_char,
    f: JSCFunction,
    argc: c_int,
) {
    let name = unsafe { text(name) };
    unsafe {
        with_scope(ctx, |scope| {
            let object = quickjs::borrow_value(scope, obj);
            let function = quickjs::c_function(scope, &name, arity(argc), f);
            let _ = crate::bind_function(scope, &object, &name, function);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_ctor(
    ctx: *mut JSContext,
    f: JSCFunction,
    name: *const c_char,
    argc: c_int,
) -> JSValue {
    let name = unsafe { text(name) };
    unsafe {
        with_scope(ctx, |scope| {
            quickjs::into_raw(make_constructor(scope, f, &name, argc))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bind_ctor(
    ctx: *mut JSContext,
    obj: JSValue,
    name: *const c_char,
    f: JSCFunction,
    argc: c_int,
) {
    let name = unsafe { text(name) };
    unsafe {
        with_scope(ctx, |scope| {
            let object = quickjs::borrow_value(scope, obj);
            let constructor = make_constructor(scope, f, &name, argc);
            let _ = scope.set(&object, &name, constructor);
        })
    }
}

macro_rules! export_native {
    ($name:ident, $f:path) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            ctx: *mut JSContext,
            this_val: JSValue,
            argc: c_int,
            argv: *mut JSValue,
        ) -> JSValue {
            let f: NativeFn = $f;
            unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
        }
    };
}

export_native!(ns_event_noop, natives::noop);
export_native!(ns_event_empty_array, natives::empty_array);
export_native!(ns_returns_resolved_undefined, natives::resolved_undefined);
export_native!(ns_returns_resolved_false, natives::resolved_false);
export_native!(
    ns_returns_resolved_empty_array,
    natives::resolved_empty_array
);
