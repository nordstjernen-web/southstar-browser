//! Southstar — the C-ABI QuickJS shims of the CSSOM natives and installers, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};

use crate::ffi;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cssom_register_style_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            ffi::nsr_cssom_register_style_class(scope, proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_style_class_proto(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(ffi::nsr_style_class_proto(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_cssom_install_style_proto(ctx: *mut JSContext, proto: JSValue) {
    if ctx.is_null() {
        return;
    }
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::borrow_value(scope, proto);
            ffi::nsr_cssom_install_style_proto(scope, &proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_animate(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, ffi::nsr_element_animate) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_style(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, this, _| {
            ffi::nsr_element_get_style(scope, this)
        })
    }
}
