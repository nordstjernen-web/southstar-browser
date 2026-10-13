//! Southstar — the C-ABI QuickJS shims of the shadow DOM and slot natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{GetterFn, NativeFn};

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                let f: GetterFn = $f;
                unsafe {
                    quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, this, _| {
                        f(scope, this)
                    })
                }
            }
        )*
    };
}

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
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
        )*
    };
}

getters! {
    ns_element_get_shadowRoot => crate::ffi::nsr_element_get_shadowRoot,
    ns_element_get_assignedSlot => crate::ffi::nsr_element_get_assignedSlot,
}

methods! {
    ns_element_attachShadow => crate::ffi::nsr_element_attachShadow,
    ns_element_getRootNode => crate::ffi::nsr_element_getRootNode,
    ns_element_assignedNodes => crate::ffi::nsr_element_assignedNodes,
    ns_element_assignedElements => crate::ffi::nsr_element_assignedElements,
}
