//! Southstar — the C ABI of the element bindings that raw QuickJS callers still use, each forwarding to its nsr_ twin.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{GetterFn, Value};

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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_img_natural_width(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, ffi::nsr_element_img_natural_width) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_img_natural_height(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, ffi::nsr_element_img_natural_height) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_set_nodeValue(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let val = quickjs::borrow_value(scope, val);
            let result =
                ffi::nsr_element_set_nodeValue(scope, &this, &val).map(|()| Value::undefined());
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_element_is_rendered(ctx: *mut JSContext, v: JSValue) -> GBoolean {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, v);
            glib::boolean(ffi::nsr_js_element_is_rendered(scope, &value))
        })
    }
}
