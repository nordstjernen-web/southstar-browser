//! Southstar — the C ABI of the window services as declared in src/js_internal.h, and the js.c, network and system calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint};
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::navigator;

#[repr(C)]
struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*const NsJs);

pub(crate) enum CMethod {
    DispatchEvent,
    SendBeacon,
    EmeRequestAccess,
    MediaCapabilitiesInfo,
}

unsafe extern "C" {
    fn ns_bind_event_target_listeners(ctx: *mut JSContext, obj: JSValue);
    fn ns_target_dispatchEvent(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_navigator_sendBeacon(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_eme_request_access(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_media_capabilities_info(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_js_user_activation_state(js: *const NsJs, ever: *mut GBoolean) -> GBoolean;
    fn ns_js_clipboard_write(js: *const NsJs, text: *const c_char) -> c_int;
    fn ns_net_navigator_languages() -> *mut *mut c_char;
    fn ns_net_navigator_platform() -> *const c_char;
    fn ns_net_ua_hint_platform() -> *const c_char;
    fn ns_net_is_mobile_mode() -> GBoolean;
    fn ns_user_agent_for_mode(compat_mode: *const c_char) -> *const c_char;
    fn ns_user_agent_has_client_hints(user_agent: *const c_char) -> GBoolean;
    fn g_get_num_processors() -> c_uint;
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope).cast())
}

fn text(p: *const c_char) -> String {
    unsafe { glib::bytes(p) }
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default()
}

pub(crate) fn bind_event_target(scope: &mut Scope<'_>, object: &Value) {
    unsafe { ns_bind_event_target_listeners(quickjs::raw_context(scope), quickjs::raw(object)) };
}

pub(crate) fn c_method(scope: &mut Scope<'_>, name: &str, arity: u32, method: CMethod) -> Value {
    let f: quickjs::JSCFunction = match method {
        CMethod::DispatchEvent => ns_target_dispatchEvent,
        CMethod::SendBeacon => ns_navigator_sendBeacon,
        CMethod::EmeRequestAccess => ns_eme_request_access,
        CMethod::MediaCapabilitiesInfo => ns_media_capabilities_info,
    };
    quickjs::c_function(scope, name, arity, f)
}

pub(crate) fn user_activation(js: Js) -> (bool, bool) {
    if js.0.is_null() {
        return (false, false);
    }
    let mut ever = glib::FALSE;
    let transient = unsafe { ns_js_user_activation_state(js.0, &mut ever) };
    (transient != 0, ever != 0)
}

pub(crate) fn clipboard_write(js: Js, text: &str) -> Option<bool> {
    if js.0.is_null() {
        return None;
    }
    let text = CString::new(text.replace('\0', "")).unwrap_or_default();
    match unsafe { ns_js_clipboard_write(js.0, text.as_ptr()) } {
        status if status < 0 => None,
        status => Some(status != 0),
    }
}

pub(crate) fn languages() -> Vec<String> {
    let list = unsafe { ns_net_navigator_languages() };
    let mut out = Vec::new();
    if list.is_null() {
        return out;
    }
    let mut index = 0;
    loop {
        let item = unsafe { *list.add(index) };
        if item.is_null() {
            break;
        }
        out.push(text(item));
        index += 1;
    }
    unsafe { glib::g_strfreev(list) };
    out
}

pub(crate) fn navigator_platform() -> String {
    text(unsafe { ns_net_navigator_platform() })
}

pub(crate) fn hint_platform() -> String {
    text(unsafe { ns_net_ua_hint_platform() })
}

pub(crate) fn mobile_mode() -> bool {
    unsafe { ns_net_is_mobile_mode() != 0 }
}

pub(crate) struct UserAgentConfig {
    pub configured: Option<String>,
    pub compat_mode: Option<String>,
    pub do_not_track: bool,
    pub global_privacy_control: bool,
}

fn optional_text(p: *const c_char) -> Option<String> {
    unsafe { glib::bytes(p) }.map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

pub(crate) fn user_agent_config() -> UserAgentConfig {
    match southstar_config::get() {
        Some(config) => UserAgentConfig {
            configured: optional_text(config.user_agent).filter(|ua| !ua.is_empty()),
            compat_mode: optional_text(config.compat_mode),
            do_not_track: config.do_not_track != 0,
            global_privacy_control: config.global_privacy_control != 0,
        },
        None => UserAgentConfig {
            configured: None,
            compat_mode: None,
            do_not_track: true,
            global_privacy_control: true,
        },
    }
}

pub(crate) fn user_agent_for_mode(compat_mode: Option<&str>) -> String {
    let mode = compat_mode.and_then(|mode| CString::new(mode).ok());
    text(unsafe { ns_user_agent_for_mode(mode.as_ref().map_or(core::ptr::null(), |m| m.as_ptr())) })
}

pub(crate) fn has_client_hints(user_agent: &str) -> bool {
    let Ok(user_agent) = CString::new(user_agent) else {
        return false;
    };
    unsafe { ns_user_agent_has_client_hints(user_agent.as_ptr()) != 0 }
}

pub(crate) fn processor_count() -> u32 {
    unsafe { g_get_num_processors() }
}

#[cfg(windows)]
pub(crate) fn physical_memory_bytes() -> Option<u64> {
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }
    unsafe extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> c_int;
    }
    let mut status = MemoryStatusEx {
        length: size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.total_phys)
}

#[cfg(not(windows))]
pub(crate) fn physical_memory_bytes() -> Option<u64> {
    None
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_window_navigator(ctx: *mut JSContext) -> JSValue {
    unsafe { quickjs::with_context(ctx, |scope| quickjs::into_raw(navigator::window(scope))) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_services_worker_navigator(ctx: *mut JSContext) -> JSValue {
    unsafe { quickjs::with_context(ctx, |scope| quickjs::into_raw(navigator::worker(scope))) }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_services_chrome_compat() -> GBoolean {
    glib::boolean(navigator::chrome_compat())
}
