//! Southstar — the dispatch entry points other code calls: C exports over page and node pointers, and the Rust-ABI nsr_ functions over scopes and values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_uint};
use core::ptr;
use std::borrow::Cow;

use southstar_dom::NsNode;
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};

use super::{Js, NsJs, in_context, node, opt_text};
use crate::JsResult;

macro_rules! natives {
    ($($name:ident => $f:path;)*) => {
        $(
            #[allow(non_snake_case)]
            #[unsafe(no_mangle)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

natives! {
    nsr_element_addEventListener => crate::listeners::element_add;
    nsr_element_removeEventListener => crate::listeners::element_remove;
    nsr_document_addEventListener => crate::listeners::document_add;
    nsr_document_removeEventListener => crate::listeners::document_remove;
    nsr_window_addEventListener => crate::listeners::window_add;
    nsr_window_removeEventListener => crate::listeners::window_remove;
    nsr_target_addEventListener => crate::target::add;
    nsr_target_removeEventListener => crate::target::remove;
    nsr_element_dispatchEvent => crate::target::element_dispatch;
    nsr_document_dispatchEvent => crate::target::document_dispatch;
    nsr_window_dispatchEvent => crate::target::window_dispatch;
    nsr_target_dispatchEvent => crate::target::dispatch;
    nsr_window_report_error => crate::errors::report_error;
}

#[unsafe(no_mangle)]
pub fn nsr_listener_parse_options(
    scope: &mut Scope<'_>,
    opts: &Value,
    strict_signal: bool,
) -> Result<(bool, bool, bool, bool, Option<Value>), Value> {
    crate::listeners::parse_options(scope, opts, strict_signal).map(|parsed| {
        (
            parsed.capture,
            parsed.once,
            parsed.passive,
            parsed.passive_set,
            parsed.signal,
        )
    })
}

#[unsafe(no_mangle)]
pub fn nsr_listeners_compact_dead(scope: &mut Scope<'_>, owner: &Value) {
    crate::target::compact_dead(scope, owner);
}

#[unsafe(no_mangle)]
pub fn nsr_target_handler_realm(
    scope: &mut Scope<'_>,
    obj: &Value,
    kind: &str,
    listener_key: &str,
) -> ContextHandle {
    crate::target::handler_realm(scope, obj, kind, listener_key)
}

#[unsafe(no_mangle)]
pub fn nsr_target_dispatch_with_event(scope: &mut Scope<'_>, obj: &Value, kind: &str, ev: &Value) {
    crate::target::dispatch_with_event(scope, obj, kind, ev);
}

#[unsafe(no_mangle)]
pub fn nsr_target_fire_event(scope: &mut Scope<'_>, obj: &Value, kind: &str) {
    let ev = crate::target::make_event(scope, obj, kind);
    crate::target::dispatch_with_event(scope, obj, kind, &ev);
}

#[unsafe(no_mangle)]
pub fn nsr_target_make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    crate::target::make_event(scope, target, kind)
}

#[unsafe(no_mangle)]
pub fn nsr_xhr_fire_progress_event(
    scope: &mut Scope<'_>,
    target: &Value,
    kind: &str,
    loaded: f64,
    total: f64,
    length_computable: bool,
) {
    crate::target::fire_progress_event(scope, target, kind, loaded, total, length_computable);
}

#[unsafe(no_mangle)]
pub fn nsr_js_dispatch_engine_event(scope: &mut Scope<'_>, target: &Value, ev: &Value) {
    crate::target::dispatch_engine_event(scope, target, ev);
}

#[unsafe(no_mangle)]
pub fn nsr_js_install_event_target(scope: &mut Scope<'_>, global: &Value) {
    crate::target::install_event_target(scope, global);
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_js_dispatch_built_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: &str,
    event: Value,
) -> (bool, bool) {
    match node(target) {
        Some(target) => crate::dispatch::dispatch_built(Js::of(js), target, kind, event),
        None => (false, false),
    }
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_js_dispatch_window_only_event(
    js: *mut NsJs,
    target_doc: *const NsNode,
    kind: &str,
    event: Value,
) -> bool {
    crate::dispatch::dispatch_window_only(Js::of(js), node(target_doc), kind, event)
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_js_event_window_for_document(
    js: *mut NsJs,
    scope: &mut Scope<'_>,
    doc: *const NsNode,
) -> Value {
    crate::dispatch::window_for_document(Js::of(js), scope, node(doc))
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_js_fire_element_handlers(
    js: *mut NsJs,
    element: *const NsNode,
    kind: &str,
    event: &Value,
) {
    let js = Js::of(js);
    let Some(element) = node(element) else {
        return;
    };
    if js.is_null() {
        return;
    }
    crate::handlers::fire_inline(js, element, kind, event);
    crate::handlers::fire_property(js, element, kind, event);
}

#[unsafe(no_mangle)]
pub fn nsr_install_event_handler_props(scope: &mut Scope<'_>, target: &Value) {
    crate::handlers::install_props(scope, target);
}

#[unsafe(no_mangle)]
pub fn nsr_install_event_handler_accessors(scope: &mut Scope<'_>, proto: &Value) {
    crate::handlers::install_accessors(scope, proto);
}

#[unsafe(no_mangle)]
pub fn nsr_install_body_reflected_handlers(scope: &mut Scope<'_>, wrapper: &Value) {
    crate::body_handlers::install(scope, wrapper);
}

#[unsafe(no_mangle)]
pub fn nsr_js_promise_rejection_tracker(
    scope: &mut Scope<'_>,
    promise: &Value,
    reason: &Value,
    handled: bool,
) {
    crate::errors::track_rejection(scope, promise, reason, handled);
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_js_report_uncaught(js: *mut NsJs, exception: &Value, origin: Option<&str>) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::errors::report_exception_at(js, exception, origin, 0, 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_target_report_exception(
    js: *mut NsJs,
    scope: &mut Scope<'_>,
    kind: Option<&str>,
    exception: &Value,
) {
    crate::errors::report_handler_exception(Js::of(js), scope, kind, exception);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    put_prevented(default_prevented, false);
    let js = Js::of(js);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    if js.is_null() {
        return glib::FALSE;
    }
    let (fired, prevented) = crate::dispatch::dispatch_event(js, target, &kind);
    put_prevented(default_prevented, prevented);
    glib::boolean(fired)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_path(
    js: *mut NsJs,
    len: *mut c_uint,
    window: *mut GBoolean,
) -> *const *const NsNode {
    let js = Js::of(js);
    if js.is_null() {
        return ptr::null();
    }
    let Some((nodes, len_value, window_value)) = crate::peek(js, |page| {
        page.paths
            .last()
            .map(|path| (path.nodes, path.len, path.window))
    }) else {
        return ptr::null();
    };
    if let Some(slot) = unsafe { len.as_mut() } {
        *slot = len_value as c_uint;
    }
    if let Some(slot) = unsafe { window.as_mut() } {
        *slot = glib::boolean(window_value);
    }
    nodes
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_has_click_handler(
    js: *mut NsJs,
    target: *const NsNode,
) -> GBoolean {
    let js = Js::of(js);
    let Some(target) = node(target) else {
        return glib::FALSE;
    };
    if js.is_null() {
        return glib::FALSE;
    }
    let found = ["click", "pointerdown", "mousedown", "touchstart"]
        .iter()
        .any(|kind| crate::dispatch::path_has_active_listener(js, target, kind));
    glib::boolean(found)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_path_has_active_listener(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
) -> GBoolean {
    let js = Js::of(js);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    if js.is_null() {
        return glib::FALSE;
    }
    glib::boolean(crate::dispatch::path_has_active_listener(js, target, &kind))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_type_init_flags(
    kind: *const c_char,
    at_document: GBoolean,
    bubbles: *mut GBoolean,
    cancelable: *mut GBoolean,
) {
    let kind = opt_text(kind).unwrap_or_default();
    let (b, c) = crate::dispatch::init_flags(&kind, at_document != 0);
    if let Some(slot) = unsafe { bubbles.as_mut() } {
        *slot = glib::boolean(b);
    }
    if let Some(slot) = unsafe { cancelable.as_mut() } {
        *slot = glib::boolean(c);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_report_error_event_in(
    js: *mut NsJs,
    ctx: ContextHandle,
    message: *const c_char,
    filename: *const c_char,
    lineno: c_int,
    colno: c_int,
) {
    let js = Js::of(js);
    if js.is_null() {
        return;
    }
    let message = opt_text(message).map(Cow::into_owned);
    let filename = opt_text(filename).map(Cow::into_owned);
    js.in_realm_scope(ctx, || {
        crate::errors::report_error_event(
            js,
            message.as_deref(),
            filename.as_deref(),
            lineno,
            colno,
            &Value::null(),
        );
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_report_pending_rejections(js: *mut NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::errors::report_pending_rejections(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_drop_pending_rejections(js: *mut NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::errors::drop_rejections(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dispatch_forget_node(js: *mut NsJs, node: *const NsNode) {
    let js = Js::of(js);
    if !js.is_null() && !node.is_null() {
        crate::forget_node(js, node);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dispatch_reset(js: *mut NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::reset(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dispatch_teardown_listeners(js: *mut NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::teardown_listeners(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dispatch_teardown(js: *mut NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::teardown(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dispatch_listener_count(js: *const NsJs) -> c_uint {
    let js = Js::of(js);
    if js.is_null() {
        return 0;
    }
    crate::listener_count(js) as c_uint
}

pub(super) fn put_prevented(slot: *mut GBoolean, prevented: bool) {
    if let Some(slot) = unsafe { slot.as_mut() } {
        *slot = glib::boolean(prevented);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_key_event_full(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
    key: *const c_char,
    code: *const c_char,
    key_code: c_int,
    char_code: c_int,
    shift: GBoolean,
    ctrl: GBoolean,
    alt: GBoolean,
    meta: GBoolean,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    put_prevented(default_prevented, false);
    let js = Js::of(js);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    if js.is_null() {
        return glib::FALSE;
    }
    let key = opt_text(key);
    let code = opt_text(code);
    let press = crate::ui_events::KeyPress {
        key: key.as_deref(),
        code: code.as_deref(),
        key_code,
        char_code,
        modifiers: crate::ui_events::Modifiers {
            shift: shift != 0,
            ctrl: ctrl != 0,
            alt: alt != 0,
            meta: meta != 0,
        },
    };
    let (fired, prevented) = crate::ui_events::key_event(js, target, &kind, &press);
    put_prevented(default_prevented, prevented);
    glib::boolean(fired)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_key_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
    key: *const c_char,
    code: *const c_char,
    key_code: c_int,
    shift: GBoolean,
    ctrl: GBoolean,
    alt: GBoolean,
    meta: GBoolean,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    unsafe {
        ns_js_dispatch_key_event_full(
            js,
            target,
            kind,
            key,
            code,
            key_code,
            0,
            shift,
            ctrl,
            alt,
            meta,
            default_prevented,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_input_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
    input_type: *const c_char,
    data: *const c_char,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    put_prevented(default_prevented, false);
    let js = Js::of(js);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    if js.is_null() {
        return glib::FALSE;
    }
    let input_type = opt_text(input_type);
    let data = opt_text(data);
    let (fired, prevented) =
        crate::ui_events::input_event(js, target, &kind, input_type.as_deref(), data.as_deref());
    put_prevented(default_prevented, prevented);
    glib::boolean(fired)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fire_toggle_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
    old_state: *const c_char,
    new_state: *const c_char,
    cancelable: GBoolean,
    source: *const NsNode,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    put_prevented(default_prevented, false);
    let js = Js::of(js);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    if js.is_null() {
        return glib::FALSE;
    }
    let old_state = opt_text(old_state);
    let new_state = opt_text(new_state);
    let toggle = crate::ui_events::Toggle {
        old_state: old_state.as_deref(),
        new_state: new_state.as_deref(),
        cancelable: cancelable != 0,
        source: node(source),
    };
    let (fired, prevented) = crate::ui_events::toggle_event(js, target, &kind, &toggle);
    put_prevented(default_prevented, prevented);
    glib::boolean(fired)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_resource_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
) -> GBoolean {
    let js = Js::of(js);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    if js.is_null() {
        return glib::FALSE;
    }
    glib::boolean(crate::ui_events::resource_event(js, target, &kind))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_body_forward_content_handler(
    ctx: ContextHandle,
    n: *const NsNode,
    name: *const c_char,
    code: *const c_char,
) {
    let (Some(element), Some(name)) = (node(n), opt_text(name)) else {
        return;
    };
    let code = opt_text(code);
    in_context(ctx, |scope| {
        crate::body_handlers::forward_content_handler(scope, element, &name, code.as_deref());
    });
}
