//! Southstar — the C-ABI QuickJS shims of the font loading natives, kept while raw callers remain.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use core::ptr;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_fontface_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe {
        quickjs::call_native(
            ctx,
            this_val,
            argc,
            argv,
            crate::ffi::nsr_window_fontface_ctor,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_get_fonts(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, this, _| {
            crate::ffi::nsr_document_get_fonts(scope, this)
        })
    }
}
