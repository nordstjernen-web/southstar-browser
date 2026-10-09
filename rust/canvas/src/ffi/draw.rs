//! Southstar — the C ABI of the 2D context's drawing helpers that the text and image drawing in js_canvas.c still call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_void};

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use super::cairo::{Cairo, CairoPathData, Context, Path};
use super::state::{CanvasState, NsJs};

type DrawFn = unsafe extern "C" fn(cr: *mut Cairo, ud: *mut c_void);

unsafe extern "C" {
    fn ns_js_mark_mutated(js: *mut NsJs);
}

pub(crate) fn ctx_state(scope: &Scope<'_>, this: &Value) -> Option<&'static mut CanvasState> {
    let js = super::state::js_of(scope);
    if js.is_null() || !crate::hidden::is_ctx2d(this) {
        return None;
    }
    let el = super::state::Node::from_addr(crate::hidden::ptr(this));
    let st = crate::state::state_for(js, el)?;
    Some(unsafe { &mut *st })
}

pub(crate) fn mark_mutated(scope: &Scope<'_>) {
    let js = super::state::js_of(scope);
    if !js.is_null() {
        unsafe { ns_js_mark_mutated(js.ptr()) };
    }
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
pub unsafe extern "C" fn ns_ctx_with_shadow(
    _ctx: *mut JSContext,
    _this_val: JSValue,
    st: *mut CanvasState,
    draw: DrawFn,
    ud: *mut c_void,
) {
    let Some(st) = (unsafe { st.as_ref() }) else {
        return;
    };
    crate::draw::with_shadow(st, |cr| unsafe { draw(cr.raw(), ud) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_set_fill_source(
    ctx: *mut JSContext,
    this_val: JSValue,
    st: *mut CanvasState,
) {
    let Some(st) = (unsafe { st.as_ref() }) else {
        return;
    };
    unsafe {
        with_this(ctx, this_val, |scope, this| {
            crate::draw::set_fill_source(scope, this, st)
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_set_stroke_source(
    ctx: *mut JSContext,
    this_val: JSValue,
    st: *mut CanvasState,
) {
    let Some(st) = (unsafe { st.as_ref() }) else {
        return;
    };
    unsafe {
        with_this(ctx, this_val, |scope, this| {
            crate::draw::set_stroke_source(scope, this, st)
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_prepare_path_and_rule(
    ctx: *mut JSContext,
    cr: *mut Cairo,
    argc: c_int,
    argv: *mut JSValue,
) -> *mut CairoPathData {
    let Some(cr) = (unsafe { Context::from_raw(cr) }) else {
        return core::ptr::null_mut();
    };
    let raw_args = if argv.is_null() || argc <= 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(argv, argc as usize) }
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let args: Vec<Value> = raw_args
                .iter()
                .map(|&raw| quickjs::borrow_value(scope, raw))
                .collect();
            crate::draw::prepare_path_and_rule(scope, cr, &args)
                .map_or(core::ptr::null_mut(), Path::into_raw)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ctx_restore_path(cr: *mut Cairo, saved: *mut CairoPathData) {
    let saved = unsafe { Path::from_raw(saved) };
    match unsafe { Context::from_raw(cr) } {
        Some(cr) => crate::draw::restore_path(cr, saved),
        None => drop(saved),
    }
}
