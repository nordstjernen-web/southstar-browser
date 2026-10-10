//! Southstar — the C ABI of the window's navigation bindings as declared in src/js_internal.h, and the js.c and URL calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use std::ffi::CString;

use southstar_glib::GBoolean;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::{history, navigation, set};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
struct NsUrlParts {
    href: *mut c_char,
    protocol: *mut c_char,
    origin: *mut c_char,
    host: *mut c_char,
    hostname: *mut c_char,
    port: *mut c_char,
    pathname: *mut c_char,
    search: *mut c_char,
    hash: *mut c_char,
    username: *mut c_char,
    password: *mut c_char,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }
}

pub(crate) struct UrlOrigin {
    pub protocol: Vec<u8>,
    pub hostname: Vec<u8>,
    pub port: Vec<u8>,
}

unsafe extern "C" {
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_set_current_url(js: *mut NsJs, url: *const c_char);
    fn ns_js_soft_navigate(js: *mut NsJs, url: *const c_char, replace: GBoolean);
    fn ns_js_navigate(js: *mut NsJs, url: *const c_char, reload: GBoolean) -> GBoolean;
    fn ns_js_window_events_blocked(js: *const NsJs) -> GBoolean;
    fn ns_js_dispatch_document_window_event(js: *mut NsJs, kind: *const c_char, event: JSValue);
    fn ns_window_structured_clone(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_target_make_event(ctx: *mut JSContext, target: JSValue, kind: *const c_char) -> JSValue;
    fn ns_target_dispatch_with_event(
        ctx: *mut JSContext,
        target: JSValue,
        kind: *const c_char,
        event: JSValue,
    );
    fn ns_bind_event_target_listeners(ctx: *mut JSContext, object: JSValue);
    fn ns_make_window_event(ctx: *mut JSContext, kind: *const c_char) -> JSValue;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_url_parts_new(url: *const c_char) -> *mut NsUrlParts;
    fn ns_url_parts_free(parts: *mut NsUrlParts);
}

fn c_string(bytes: &[u8]) -> CString {
    CString::new(bytes).unwrap_or_default()
}

unsafe fn borrowed(p: *const c_char) -> Vec<u8> {
    if p.is_null() {
        Vec::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_bytes().to_vec()
    }
}

unsafe fn taken(p: *mut c_char) -> Option<Vec<u8>> {
    if p.is_null() {
        return None;
    }
    let bytes = unsafe { borrowed(p) };
    unsafe { southstar_glib::g_free(p.cast()) };
    Some(bytes)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope) as usize)
}

pub(crate) fn with_main_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    if ctx.is_null() {
        return None;
    }
    Some(unsafe { quickjs::with_context(ctx, f) })
}

pub(crate) fn current_url(js: Js) -> Vec<u8> {
    unsafe { borrowed(ns_js_current_url(js.ptr())) }
}

pub(crate) fn set_current_url(js: Js, url: &[u8]) {
    let url = c_string(url);
    unsafe { ns_js_set_current_url(js.ptr(), url.as_ptr()) };
}

pub(crate) fn soft_navigate(js: Js, url: &[u8], replace: bool) {
    let url = c_string(url);
    unsafe { ns_js_soft_navigate(js.ptr(), url.as_ptr(), southstar_glib::boolean(replace)) };
}

pub(crate) fn navigate(js: Js, url: &[u8], reload: bool) -> bool {
    let url = c_string(url);
    unsafe { ns_js_navigate(js.ptr(), url.as_ptr(), southstar_glib::boolean(reload)) != 0 }
}

pub(crate) fn window_events_blocked(js: Js) -> bool {
    unsafe { ns_js_window_events_blocked(js.ptr()) != 0 }
}

pub(crate) fn dispatch_document_window_event(js: Js, kind: &str, event: Value) {
    let kind = c_string(kind.as_bytes());
    unsafe {
        ns_js_dispatch_document_window_event(js.ptr(), kind.as_ptr(), quickjs::into_raw(event))
    };
}

pub(crate) fn structured_clone(scope: &mut Scope<'_>, value: &Value) -> Result<Value, Value> {
    quickjs::call_c_function(
        scope,
        ns_window_structured_clone,
        &Value::undefined(),
        core::slice::from_ref(value),
    )
}

pub(crate) fn target_make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    let kind = c_string(kind.as_bytes());
    let raw = unsafe {
        ns_target_make_event(
            quickjs::raw_context(scope),
            quickjs::raw(target),
            kind.as_ptr(),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn target_dispatch(scope: &mut Scope<'_>, target: &Value, kind: &str, event: &Value) {
    let kind = c_string(kind.as_bytes());
    unsafe {
        ns_target_dispatch_with_event(
            quickjs::raw_context(scope),
            quickjs::raw(target),
            kind.as_ptr(),
            quickjs::raw(event),
        )
    };
}

pub(crate) fn bind_event_target_listeners(scope: &mut Scope<'_>, object: &Value) {
    unsafe { ns_bind_event_target_listeners(quickjs::raw_context(scope), quickjs::raw(object)) };
}

pub(crate) fn make_window_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    let kind = c_string(kind.as_bytes());
    let raw = unsafe { ns_make_window_event(quickjs::raw_context(scope), kind.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn url_resolve(base: Option<&[u8]>, href: &[u8]) -> Option<Vec<u8>> {
    let base = base.map(c_string);
    let href = c_string(href);
    unsafe {
        taken(ns_url_resolve(
            base.as_ref()
                .map_or(core::ptr::null(), |base| base.as_ptr()),
            href.as_ptr(),
        ))
    }
}

pub(crate) fn url_origin(url: &[u8]) -> Option<Vec<u8>> {
    let url = c_string(url);
    unsafe { taken(ns_url_origin_from(url.as_ptr())) }
}

pub(crate) fn url_parts(url: &[u8]) -> Option<UrlOrigin> {
    let url = c_string(url);
    let parts = unsafe { ns_url_parts_new(url.as_ptr()) };
    let p = unsafe { parts.as_ref() }?;
    let origin = unsafe {
        UrlOrigin {
            protocol: borrowed(p.protocol),
            hostname: borrowed(p.hostname),
            port: borrowed(p.port),
        }
    };
    unsafe { ns_url_parts_free(parts) };
    Some(origin)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_install_history(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            let history = history::make_history(scope);
            set(scope, &global, "history", history);
            let navigation = navigation::make_navigation(scope);
            set(scope, &global, "navigation", navigation);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_history_teardown(js: *const NsJs) {
    crate::teardown(Js::of(js));
}
