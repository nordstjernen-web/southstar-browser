//! Southstar — the C ABI of the event core that raw QuickJS callers still use, each forwarding to its nsr_ twin.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use southstar_dom::NsNode;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::ffi;

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

unsafe fn with_value(ctx: *mut JSContext, value: JSValue, f: impl FnOnce(&mut Scope<'_>, &Value)) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            f(scope, &value);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_new(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let event = ffi::nsr_event_new(scope);
            quickjs::result_raw(scope, event)
        })
    }
}

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

natives! {
    ns_event_prevent_default => ffi::nsr_event_prevent_default,
    ns_event_stop_propagation => ffi::nsr_event_stop_propagation,
    ns_event_stop_immediate => ffi::nsr_event_stop_immediate,
    ns_event_composed_path => ffi::nsr_event_composed_path,
}

macro_rules! value_hooks {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, value: JSValue) {
                unsafe { with_value(ctx, value, $f) }
            }
        )*
    };
}

value_hooks! {
    ns_event_define_cancel_bubble => ffi::nsr_event_define_cancel_bubble,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_event(
    ctx: *mut JSContext,
    kind: *const c_char,
    target: *const NsNode,
) -> JSValue {
    let kind = text(kind);
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(ffi::nsr_make_event(scope, &kind, target))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_adopt_interface(
    ctx: *mut JSContext,
    ev: JSValue,
    iface: *const c_char,
) {
    let iface = text(iface);
    unsafe {
        with_value(ctx, ev, |scope, ev| {
            ffi::nsr_event_adopt_interface(scope, ev, &iface)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_throw_dom_exception(
    ctx: *mut JSContext,
    name: *const c_char,
    code: c_int,
    message: *const c_char,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let error = ffi::nsr_dom_exception(scope, c_bytes(name), code, c_bytes(message));
            quickjs::result_raw(scope, Err(error))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_abort_error(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(ffi::nsr_make_abort_error(scope))
        })
    }
}

fn text(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}

fn c_bytes<'a>(text: *const c_char) -> &'a [u8] {
    if text.is_null() {
        return &[];
    }
    unsafe { CStr::from_ptr(text) }.to_bytes()
}
