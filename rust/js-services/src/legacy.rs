//! Southstar — the C ABI of the window services that raw QuickJS callers still use, each forwarding to its nsr_ twin.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, NsJs};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_window_navigator(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(ffi::nsr_services_window_navigator(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_worker_navigator(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(ffi::nsr_services_worker_navigator(scope))
        })
    }
}

macro_rules! export_native {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { quickjs::call_native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

export_native! {
    ns_services_alert => ffi::nsr_services_alert,
    ns_services_confirm => ffi::nsr_services_confirm,
    ns_services_prompt => ffi::nsr_services_prompt,
    ns_services_exec_command => ffi::nsr_services_exec_command,
    ns_services_query_command_supported => ffi::nsr_services_query_command_supported,
    ns_services_query_command_enabled => ffi::nsr_services_query_command_enabled,
    ns_services_queue_microtask => ffi::nsr_services_queue_microtask,
    ns_services_notification_ctor => ffi::nsr_services_notification_ctor,
    ns_services_match_media => ffi::nsr_services_match_media,
    ns_services_set_timeout => ffi::nsr_services_set_timeout,
    ns_services_set_interval => ffi::nsr_services_set_interval,
    ns_services_clear_timer => ffi::nsr_services_clear_timer,
    ns_element_async_method => ffi::nsr_element_async_method,
    ns_cache_open => ffi::nsr_cache_open,
    ns_services_request_idle_callback => ffi::nsr_services_request_idle_callback,
    ns_services_camera_request => ffi::nsr_services_camera_request,
    ns_services_camera_release => ffi::nsr_services_camera_release,
    ns_services_mic_release => ffi::nsr_services_mic_release,
    ns_services_camera_label => ffi::nsr_services_camera_label,
    ns_services_camera_enumerate => ffi::nsr_services_camera_enumerate,
}

unsafe fn with_value(ctx: *mut JSContext, value: JSValue, f: fn(&mut Scope<'_>, &Value)) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            f(scope, &value);
        })
    }
}

macro_rules! installers {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, global: JSValue) {
                unsafe { with_value(ctx, global, $f) }
            }
        )*
    };
}

installers! {
    ns_install_idle_deadline => ffi::nsr_install_idle_deadline,
    ns_services_install_console => ffi::nsr_services_install_console,
    ns_services_install_rtc => ffi::nsr_services_install_rtc,
    ns_services_install_clipboard_item => ffi::nsr_services_install_clipboard_item,
    ns_services_install_screen => ffi::nsr_services_install_screen,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_console_emit(
    js: *const NsJs,
    prefix: *const c_char,
    ctx: *mut JSContext,
    argc: c_int,
    argv: *mut JSValue,
) {
    let prefix = ffi::text(prefix);
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
            ffi::nsr_services_console_emit(js, scope, &prefix, &args);
        })
    }
}
