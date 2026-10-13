//! Southstar — the C-ABI QuickJS shims of the Web Storage installers, class and constructor, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_install_proto(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::borrow_value(scope, proto);
            crate::ffi::nsr_storage_install_proto(scope, &proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_register_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            crate::ffi::nsr_storage_register_class(scope, proto);
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_js_storage_class_id() -> u32 {
    quickjs::class_id(&crate::class::STORAGE_CLASS)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_area_of(obj: JSValue) -> c_int {
    unsafe { quickjs::with_raw(obj, crate::ffi::nsr_storage_area_of) }.unwrap_or(0)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_storage_new(ctx: *mut JSContext, area: c_int) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let storage = crate::ffi::nsr_storage_new(scope, area);
            quickjs::result_raw(scope, storage)
        })
    }
}
