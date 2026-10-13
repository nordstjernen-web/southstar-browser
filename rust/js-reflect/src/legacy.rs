//! Southstar — the C-ABI accessors the raw QuickJS callers still bind, each forwarding to its nsr_ twin.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::JsResult;
use crate::ffi;

unsafe fn getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value) -> JsResult,
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
pub unsafe extern "C" fn ns_element_int_attr_getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        getter(ctx, this_val, |scope, this| {
            ffi::nsr_element_int_attr_getter(scope, this, magic)
        })
    }
}
