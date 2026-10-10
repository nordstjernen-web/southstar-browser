//! Southstar — the C ABI of the media bindings as declared in src/js_internal.h, and the js.c and microphone calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::{audio, eme};

unsafe extern "C" {
    fn ns_bind_event_target_listeners(ctx: *mut JSContext, obj: JSValue);
    fn ns_target_dispatchEvent(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_new(ctx: *mut JSContext) -> JSValue;
    fn ns_event_adopt_interface(ctx: *mut JSContext, event: JSValue, iface: *const c_char);
    fn ns_mic_fill_time_domain(out: *mut u8, n: c_int);
    fn ns_mic_fill_frequency(out: *mut u8, n: c_int);
}

fn ctx_of(scope: &Scope<'_>) -> *mut JSContext {
    quickjs::raw_context(scope)
}

pub(crate) fn bind_event_target(scope: &mut Scope<'_>, object: &Value) {
    unsafe { ns_bind_event_target_listeners(ctx_of(scope), quickjs::raw(object)) };
    let dispatch = quickjs::c_function(scope, "dispatchEvent", 1, ns_target_dispatchEvent);
    crate::set(scope, object, "dispatchEvent", dispatch);
}

pub(crate) fn new_event(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_event_new(ctx_of(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    let iface = std::ffi::CString::new(iface).unwrap_or_default();
    unsafe { ns_event_adopt_interface(ctx_of(scope), quickjs::raw(event), iface.as_ptr()) };
}

fn clamp_len(bytes: &[u8]) -> c_int {
    c_int::try_from(bytes.len()).unwrap_or(c_int::MAX)
}

pub(crate) fn mic_time_domain(bytes: &mut [u8]) {
    let n = clamp_len(bytes);
    unsafe { ns_mic_fill_time_domain(bytes.as_mut_ptr(), n) };
}

pub(crate) fn mic_frequency(bytes: &mut [u8]) {
    let n = clamp_len(bytes);
    unsafe { ns_mic_fill_frequency(bytes.as_mut_ptr(), n) };
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
pub unsafe extern "C" fn ns_eme_request_access(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, eme::request_access) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_media_set_media_keys(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, eme::set_media_keys) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_media_install_audio(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            audio::install(scope, &global);
        })
    }
}
