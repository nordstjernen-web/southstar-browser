//! Southstar — the C ABI of the text bindings that raw QuickJS callers still bind, each forwarding to its nsr_ twin.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{GetterFn, SetterFn, Value};

use crate::ffi;

unsafe fn getter(ctx: *mut JSContext, this_val: JSValue, f: GetterFn) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let result = f(scope, &this);
            quickjs::result_raw(scope, result)
        })
    }
}

unsafe fn setter(ctx: *mut JSContext, this_val: JSValue, val: JSValue, f: SetterFn) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let val = quickjs::borrow_value(scope, val);
            let result = f(scope, &this, &val).map(|()| Value::undefined());
            quickjs::result_raw(scope, result)
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
                unsafe { quickjs::call_native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

natives! {
    ns_element_normalize => ffi::nsr_element_normalize,
    ns_element_substring_data => ffi::nsr_element_substring_data,
    ns_element_append_data => ffi::nsr_element_append_data,
    ns_element_delete_data => ffi::nsr_element_delete_data,
    ns_element_insert_data => ffi::nsr_element_insert_data,
    ns_element_replace_data => ffi::nsr_element_replace_data,
    ns_element_split_text => ffi::nsr_element_split_text,
    ns_text_replaceWholeText => ffi::nsr_text_replaceWholeText,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_innerText(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, ffi::nsr_element_get_innerText) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_set_innerText(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
) -> JSValue {
    unsafe { setter(ctx, this_val, val, ffi::nsr_element_set_innerText) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_set_outerText(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
) -> JSValue {
    unsafe { setter(ctx, this_val, val, ffi::nsr_element_set_outerText) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_wholeText(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, ffi::nsr_element_get_wholeText) }
}
