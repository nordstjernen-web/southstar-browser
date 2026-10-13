//! Southstar — the C ABI shims of EventTarget and dispatch for crates still on raw QuickJS; each goes once its last caller calls the nsr_ entry point.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};

use southstar_dom::NsNode;
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use super::exports::put_prevented;
use super::{Js, NsJs, node, opt_text};
use crate::JsResult;

fn in_context<R>(ctx: *mut JSContext, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
    unsafe { quickjs::with_context(ctx, f) }
}

fn borrowed(scope: &Scope<'_>, raw: JSValue) -> Value {
    unsafe { quickjs::borrow_value(scope, raw) }
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

macro_rules! natives {
    ($($name:ident => $f:path;)*) => {
        $(
            #[allow(non_snake_case)]
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

natives! {
    ns_element_addEventListener => crate::listeners::element_add;
    ns_element_removeEventListener => crate::listeners::element_remove;
    ns_window_addEventListener => crate::listeners::window_add;
    ns_window_removeEventListener => crate::listeners::window_remove;
    ns_element_dispatchEvent => crate::target::element_dispatch;
    ns_window_dispatchEvent => crate::target::window_dispatch;
    ns_target_dispatchEvent => crate::target::dispatch;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_target_dispatch_with_event(
    ctx: *mut JSContext,
    obj: JSValue,
    kind: *const c_char,
    ev: JSValue,
) {
    let kind = opt_text(kind).unwrap_or_default();
    in_context(ctx, |scope| {
        let obj = borrowed(scope, obj);
        let ev = borrowed(scope, ev);
        crate::target::dispatch_with_event(scope, &obj, &kind, &ev);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_target_fire_event(
    ctx: *mut JSContext,
    obj: JSValue,
    kind: *const c_char,
) {
    let kind = opt_text(kind).unwrap_or_default();
    in_context(ctx, |scope| {
        let obj = borrowed(scope, obj);
        let ev = crate::target::make_event(scope, &obj, &kind);
        crate::target::dispatch_with_event(scope, &obj, &kind, &ev);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_target_make_event(
    ctx: *mut JSContext,
    target: JSValue,
    kind: *const c_char,
) -> JSValue {
    let kind = opt_text(kind).unwrap_or_default();
    in_context(ctx, |scope| {
        let target = borrowed(scope, target);
        quickjs::into_raw(crate::target::make_event(scope, &target, &kind))
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_xhr_fire_progress_event(
    ctx: *mut JSContext,
    target: JSValue,
    kind: *const c_char,
    loaded: f64,
    total: f64,
    length_computable: GBoolean,
) {
    let kind = opt_text(kind).unwrap_or_default();
    in_context(ctx, |scope| {
        let target = borrowed(scope, target);
        crate::target::fire_progress_event(
            scope,
            &target,
            &kind,
            loaded,
            total,
            length_computable != 0,
        );
    });
}

fn take_event(js: Js, event: JSValue) -> Value {
    in_context(js.ctx().as_ptr().cast(), |scope| unsafe {
        quickjs::take_value(scope, event)
    })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_built_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
    event: JSValue,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    let js = Js::of(js);
    let kind = opt_text(kind).unwrap_or_default();
    let event = take_event(js, event);
    let (fired, prevented) = match node(target) {
        Some(target) => crate::dispatch::dispatch_built(js, target, &kind, event),
        None => (false, false),
    };
    put_prevented(default_prevented, prevented);
    glib::boolean(fired)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_window_only_event(
    js: *mut NsJs,
    target_doc: *const NsNode,
    kind: *const c_char,
    event: JSValue,
    default_prevented: *mut GBoolean,
) {
    let js = Js::of(js);
    let kind = opt_text(kind).unwrap_or_default();
    let event = take_event(js, event);
    let prevented = crate::dispatch::dispatch_window_only(js, node(target_doc), &kind, event);
    put_prevented(default_prevented, prevented);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_event_window_for_document(
    js: *mut NsJs,
    doc: *const NsNode,
) -> JSValue {
    let js = Js::of(js);
    js.scope(|scope| quickjs::into_raw(crate::dispatch::window_for_document(js, scope, node(doc))))
        .unwrap_or(quickjs::UNDEFINED)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fire_element_handlers(
    js: *mut NsJs,
    element: *const NsNode,
    kind: *const c_char,
    event: JSValue,
) {
    let js = Js::of(js);
    let (Some(element), Some(kind)) = (node(element), opt_text(kind)) else {
        return;
    };
    if js.is_null() {
        return;
    }
    let event = in_context(js.ctx().as_ptr().cast(), |scope| borrowed(scope, event));
    crate::handlers::fire_inline(js, element, &kind, &event);
    crate::handlers::fire_property(js, element, &kind, &event);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_event_handler_accessors(ctx: *mut JSContext, proto: JSValue) {
    in_context(ctx, |scope| {
        let proto = borrowed(scope, proto);
        crate::handlers::install_accessors(scope, &proto);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_report_uncaught(js: *mut NsJs, ex: JSValue, origin: *const c_char) {
    let js = Js::of(js);
    if js.is_null() {
        return;
    }
    let origin = opt_text(origin).map(std::borrow::Cow::into_owned);
    let ex = in_context(js.ctx().as_ptr().cast(), |scope| borrowed(scope, ex));
    crate::errors::report_exception_at(js, &ex, origin.as_deref(), 0, 0);
}
