//! Southstar — the C-ABI QuickJS shims of the focus, click, fullscreen and pointer lock natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{GetterFn, NativeFn};

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[allow(non_snake_case)]
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
    )*};
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[allow(non_snake_case)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
            let f: GetterFn = $f;
            unsafe {
                quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, this, _| {
                    f(scope, this)
                })
            }
        }
    )*};
}

methods! {
    ns_element_focus => crate::ffi::nsr_element_focus,
    ns_element_blur => crate::ffi::nsr_element_blur,
    ns_document_has_focus => crate::ffi::nsr_document_has_focus,
    ns_element_click => crate::ffi::nsr_element_click,
    ns_element_request_fullscreen => crate::ffi::nsr_element_request_fullscreen,
    ns_document_exit_fullscreen => crate::ffi::nsr_document_exit_fullscreen,
    ns_element_requestPointerLock => crate::ffi::nsr_element_requestPointerLock,
    ns_document_exitPointerLock => crate::ffi::nsr_document_exitPointerLock,
}

getters! {
    ns_document_get_fullscreen_element => crate::ffi::nsr_document_get_fullscreen_element,
    ns_document_get_fullscreen_enabled => crate::ffi::nsr_document_get_fullscreen_enabled,
    ns_document_get_is_fullscreen => crate::ffi::nsr_document_get_is_fullscreen,
    ns_document_get_pointerLockElement => crate::ffi::nsr_document_get_pointerLockElement,
}
