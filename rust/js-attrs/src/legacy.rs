//! Southstar — the C ABI shims of the attribute bindings for crates still on raw QuickJS; each goes once its last caller calls the nsr_ entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};

use southstar_js_engine::Value;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_token_list(
    ctx: *mut JSContext,
    element: JSValue,
    attr: *const c_char,
) -> JSValue {
    if attr.is_null() {
        return quickjs::into_raw(Value::null());
    }
    let attr = unsafe { CStr::from_ptr(attr) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let element = quickjs::borrow_value(scope, element);
            let result = crate::element::token_list(scope, &element, attr);
            quickjs::result_raw(scope, result)
        })
    }
}
