//! Southstar — the C ABI of the 2D context style helpers the C drawing code calls, and the pattern builder they call back into.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_double, c_int, c_void};

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use super::cairo::{Cairo, Context};
use super::state::CanvasState;

unsafe extern "C" {
    fn ns_ctx_build_pattern(
        ctx: *mut JSContext,
        obj: JSValue,
        origin_clean: *mut c_int,
    ) -> *mut c_void;
}

pub(crate) fn build_pattern(scope: &mut Scope<'_>, obj: &Value) -> (*mut c_void, bool) {
    let mut clean: c_int = 1;
    let pattern =
        unsafe { ns_ctx_build_pattern(quickjs::raw_context(scope), quickjs::raw(obj), &mut clean) };
    (pattern, clean != 0)
}

unsafe fn with_this<R>(
    ctx: *mut JSContext,
    this_val: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value) -> R,
) -> R {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            f(scope, &this)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_global_alpha(ctx: *mut JSContext, this_val: JSValue) -> c_double {
    unsafe { with_this(ctx, this_val, crate::style::global_alpha) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_apply_composite(
    ctx: *mut JSContext,
    this_val: JSValue,
    cr: *mut Cairo,
) {
    let Some(cr) = (unsafe { Context::from_raw(cr) }) else {
        return;
    };
    unsafe {
        with_this(ctx, this_val, |scope, this| {
            crate::style::apply_composite(scope, this, cr)
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_image_smoothing(ctx: *mut JSContext, this_val: JSValue) -> c_int {
    c_int::from(unsafe { with_this(ctx, this_val, crate::style::image_smoothing) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_sync_styles(
    ctx: *mut JSContext,
    this_val: JSValue,
    st: *mut CanvasState,
) {
    let Some(st) = (unsafe { st.as_mut() }) else {
        return;
    };
    unsafe {
        with_this(ctx, this_val, |scope, this| {
            crate::style::sync(scope, this, st)
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_has_shadow(st: *const CanvasState) -> c_int {
    c_int::from(unsafe { st.as_ref() }.is_some_and(crate::style::has_shadow))
}
