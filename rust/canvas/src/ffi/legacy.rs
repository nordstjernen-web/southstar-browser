//! Southstar — the C ABI shims of the canvas for crates still on raw QuickJS; each goes once its last caller calls the neutral API.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int};

use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_bitmap_is(v: JSValue) -> c_int {
    c_int::from(unsafe { quickjs::with_raw(v, super::is_image_bitmap) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hidden_is(v: JSValue, kind: c_int) -> c_int {
    c_int::from(unsafe { quickjs::with_raw(v, |v| crate::hidden::is(v, kind)) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hget(ctx: *mut JSContext, obj: JSValue, key: *const c_char) -> JSValue {
    let key = if key.is_null() {
        ""
    } else {
        unsafe { CStr::from_ptr(key) }.to_str().unwrap_or_default()
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let obj = quickjs::borrow_value(scope, obj);
            let result = crate::hidden::get(scope, &obj, key);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_canvas_clone_object(ctx: *mut JSContext, v: JSValue) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let v = quickjs::borrow_value(scope, v);
            let result = crate::api::clone_object(scope, &v);
            quickjs::result_raw(scope, result)
        })
    }
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
