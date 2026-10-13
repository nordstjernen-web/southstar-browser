//! Southstar — the C-ABI QuickJS shims of the custom element natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::NativeFn;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

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
                let f: NativeFn = $f;
                unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
            }
        )*
    };
}

natives! {
    ns_ce_define => crate::ffi::nsr_ce_define,
    ns_ce_get => crate::ffi::nsr_ce_get,
    ns_ce_when_defined => crate::ffi::nsr_ce_when_defined,
    ns_ce_upgrade => crate::ffi::nsr_ce_upgrade,
    ns_ce_get_name => crate::ffi::nsr_ce_get_name,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_html_element_construct(
    ctx: *mut JSContext,
    new_target: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let new_target = quickjs::borrow_value(scope, new_target);
            quickjs::into_raw(crate::ffi::nsr_ce_html_element_construct(
                scope,
                &new_target,
            ))
        })
    }
}
