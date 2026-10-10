//! Southstar — the C ABI of the font loading bindings as declared in src/js_internal.h, and the js.c and font loader calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint, c_void};
use core::ptr;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope};

use crate::{font_face, font_set, ready};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(usize);

impl Js {
    fn from_ptr(js: *mut NsJs) -> Js {
        Js(js as usize)
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }
}

type IdleCallback = unsafe extern "C" fn(user_data: *mut c_void);

unsafe extern "C" {
    fn ns_js_flush_layout(js: *mut NsJs);
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_drain_microtasks(js: *mut NsJs);
    fn ns_font_pending_count() -> c_uint;
    fn ns_font_add_idle_cb(cb: Option<IdleCallback>, user_data: *mut c_void);
    fn ns_font_remove_idle_cb(cb: Option<IdleCallback>, user_data: *mut c_void);
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::from_ptr(quickjs::context_opaque(scope).cast())
}

pub(crate) fn flush_layout(js: Js) {
    if !js.is_null() {
        unsafe { ns_js_flush_layout(js.ptr()) };
    }
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.ptr()) };
}

pub(crate) fn drain_microtasks(js: Js) {
    unsafe { ns_drain_microtasks(js.ptr()) };
}

pub(crate) fn pending_font_count() -> u32 {
    unsafe { ns_font_pending_count() }
}

unsafe extern "C" fn fonts_idle(user_data: *mut c_void) {
    let js = Js::from_ptr(user_data.cast());
    if js.is_null() || !ready::has_waiting(js) {
        return;
    }
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    if ctx.is_null() {
        return;
    }
    unsafe { quickjs::with_context(ctx, |scope| ready::fonts_idle(scope, js)) };
}

pub(crate) fn wait_for_fonts(js: Js) {
    unsafe { ns_font_add_idle_cb(Some(fonts_idle), js.ptr().cast()) };
}

pub(crate) fn stop_waiting_for_fonts(js: Js) {
    unsafe { ns_font_remove_idle_cb(Some(fonts_idle), js.ptr().cast()) };
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_fontface_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, font_face::construct) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_get_fonts(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe { native(ctx, this_val, 0, ptr::null_mut(), font_set::document_fonts) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fonts_teardown(js: *mut NsJs) {
    ready::teardown(Js::from_ptr(js));
}
