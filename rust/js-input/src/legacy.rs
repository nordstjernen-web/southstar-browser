//! Southstar — the C-ABI QuickJS shims of the pointer capture natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use southstar_js_engine::NativeFn;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
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
        )*
    };
}

natives! {
    ns_element_setPointerCapture => crate::ffi::nsr_element_setPointerCapture,
    ns_element_releasePointerCapture => crate::ffi::nsr_element_releasePointerCapture,
    ns_element_hasPointerCapture => crate::ffi::nsr_element_hasPointerCapture,
}
