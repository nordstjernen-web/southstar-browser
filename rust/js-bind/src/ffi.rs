//! Southstar — the host-object class reached through js.c, and the C ABI of the binding helpers as declared in src/js_internal.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::borrow::Cow;
use std::ffi::CStr;

use core::ffi::{c_char, c_int};

use southstar_js_engine::quickjs::{self, JSCFunction, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::class;
use crate::host::HostKind;
use crate::natives;

#[repr(C)]
pub struct FnDef {
    name: *const c_char,
    argc: c_int,
}

#[repr(C)]
pub struct IntConstant {
    name: *const c_char,
    value: c_int,
}

pub(crate) fn host_kind(value: &Value) -> Option<HostKind> {
    HostKind::from_raw(class::kind_of(value).unwrap_or(0))
}

pub(crate) fn host_state(_scope: &mut Scope<'_>, value: &Value, kind: HostKind) -> Option<Value> {
    class::kind_state(value, kind.raw()).filter(Value::is_object)
}

pub(crate) fn new_host(
    scope: &mut Scope<'_>,
    kind: HostKind,
    prototype: &Value,
) -> Result<Value, Value> {
    class::new_host_object(scope, kind.raw(), prototype)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ho_new(ctx: *mut JSContext, kind: c_int, proto: JSValue) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::borrow_value(scope, proto);
            let object = class::new_host_object(scope, kind, &proto);
            quickjs::result_raw(scope, object)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ho_kind_of(value: JSValue) -> c_int {
    unsafe { quickjs::with_raw(value, class::kind_of) }.unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_is_host_object(value: JSValue) -> c_int {
    c_int::from(unsafe { quickjs::with_raw(value, class::kind_of) }.is_some())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_value_is_form_data(_ctx: *mut JSContext, value: JSValue) -> c_int {
    let kind = unsafe { quickjs::with_raw(value, class::kind_of) };
    c_int::from(kind == Some(HostKind::FormData.raw()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_net_host_state(
    _ctx: *mut JSContext,
    value: JSValue,
    kind: c_int,
) -> JSValue {
    let state = unsafe { quickjs::with_raw(value, |value| class::kind_state(value, kind)) };
    state.map_or(quickjs::UNDEFINED, quickjs::into_raw)
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
pub unsafe extern "C" fn ns_bind_fn_if_missing(
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
            let _ = crate::bind_function_if_missing(scope, &object, &name, |scope| {
                quickjs::c_function(scope, &name, arity(argc), f)
            });
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bind_fns(
    ctx: *mut JSContext,
    obj: JSValue,
    f: JSCFunction,
    defs: *const FnDef,
    n: usize,
) {
    if defs.is_null() {
        return;
    }
    for def in unsafe { core::slice::from_raw_parts(defs, n) } {
        unsafe { ns_bind_fn(ctx, obj, def.name, f, def.argc) };
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bind_ctors(
    ctx: *mut JSContext,
    obj: JSValue,
    f: JSCFunction,
    defs: *const FnDef,
    n: usize,
) {
    if defs.is_null() {
        return;
    }
    for def in unsafe { core::slice::from_raw_parts(defs, n) } {
        unsafe { ns_bind_ctor(ctx, obj, def.name, f, def.argc) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bind_ctor_proto_fn(
    ctx: *mut JSContext,
    global: JSValue,
    ctor_name: *const c_char,
    fn_name: *const c_char,
    f: JSCFunction,
    argc: c_int,
) {
    let ctor_name = unsafe { text(ctor_name) };
    let fn_name = unsafe { text(fn_name) };
    unsafe {
        with_scope(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            let _ = crate::bind_prototype_function_if_missing(
                scope,
                &global,
                &ctor_name,
                &fn_name,
                |scope| quickjs::c_function(scope, &fn_name, arity(argc), f),
            );
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bind_ctor_int_constants(
    ctx: *mut JSContext,
    global: JSValue,
    ctor_name: *const c_char,
    constants: *const IntConstant,
    count: usize,
) {
    if constants.is_null() {
        return;
    }
    let ctor_name = unsafe { text(ctor_name) };
    let constants: Vec<(Cow<'_, str>, i32)> =
        unsafe { core::slice::from_raw_parts(constants, count) }
            .iter()
            .map(|constant| (unsafe { text(constant.name) }, constant.value))
            .collect();
    unsafe {
        with_scope(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            let _ = crate::bind_int_constants(
                scope,
                &global,
                &ctor_name,
                constants
                    .iter()
                    .map(|(name, value)| (name.as_ref(), *value)),
            );
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_promise_resolve_take(ctx: *mut JSContext, value: JSValue) -> JSValue {
    unsafe {
        with_scope(ctx, |scope| {
            let value = quickjs::take_value(scope, value);
            let result = natives::resolved(scope, value);
            quickjs::result_raw(scope, result)
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
export_native!(ns_returns_null, natives::null);
export_native!(ns_returns_resolved_undefined, natives::resolved_undefined);
export_native!(ns_returns_resolved_false, natives::resolved_false);
export_native!(
    ns_returns_resolved_empty_array,
    natives::resolved_empty_array
);
