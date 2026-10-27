//! Southstar — the dispatch entry points other code calls: C exports over page and node pointers, and the Rust-ABI nsr_ functions over scopes and values.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{ContextHandle, Scope, Value};

use super::{Js, NsJs};
use crate::JsResult;

macro_rules! natives {
    ($($name:ident => $f:path;)*) => {
        $(
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

natives! {
    nsr_element_addEventListener => crate::listeners::element_add;
    nsr_element_removeEventListener => crate::listeners::element_remove;
    nsr_window_addEventListener => crate::listeners::window_add;
    nsr_window_removeEventListener => crate::listeners::window_remove;
    nsr_target_addEventListener => crate::target::add;
    nsr_target_removeEventListener => crate::target::remove;
    nsr_element_dispatchEvent => crate::target::element_dispatch;
    nsr_window_dispatchEvent => crate::target::window_dispatch;
    nsr_target_dispatchEvent => crate::target::dispatch;
    nsr_window_report_error => crate::errors::report_error;
}

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

pub fn nsr_listeners_compact_dead(scope: &mut Scope<'_>, owner: &Value) {
    crate::target::compact_dead(scope, owner);
}

pub fn nsr_target_handler_realm(
    scope: &mut Scope<'_>,
    obj: &Value,
    kind: &str,
    listener_key: &str,
) -> ContextHandle {
    crate::target::handler_realm(scope, obj, kind, listener_key)
}

pub fn nsr_target_dispatch_with_event(scope: &mut Scope<'_>, obj: &Value, kind: &str, ev: &Value) {
    crate::target::dispatch_with_event(scope, obj, kind, ev);
}

pub fn nsr_target_fire_event(scope: &mut Scope<'_>, obj: &Value, kind: &str) {
    if !crate::target::has_handlers(scope, obj, kind) {
        return;
    }
    let ev = crate::target::make_event(scope, obj, kind);
    crate::target::dispatch_with_event(scope, obj, kind, &ev);
}

pub fn nsr_target_make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    crate::target::make_event(scope, target, kind)
}

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

pub fn nsr_js_dispatch_engine_event(scope: &mut Scope<'_>, target: &Value, ev: &Value) {
    crate::target::dispatch_engine_event(scope, target, ev);
}

pub fn nsr_js_install_event_target(scope: &mut Scope<'_>, global: &Value) {
    crate::target::install_event_target(scope, global);
}

pub fn nsr_install_event_handler_accessors(scope: &mut Scope<'_>, proto: &Value) {
    crate::handlers::install_accessors(scope, proto);
}

pub fn nsr_install_body_reflected_handlers(scope: &mut Scope<'_>, wrapper: &Value) {
    crate::body_handlers::install(scope, wrapper);
}

pub fn nsr_js_promise_rejection_tracker(
    scope: &mut Scope<'_>,
    promise: &Value,
    reason: &Value,
    handled: bool,
) {
    crate::errors::track_rejection(scope, promise, reason, handled);
}

fn report_pending_rejections(js: *const NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::errors::report_pending_rejections(js);
    }
}

fn forget_node(js: *const NsJs, node: southstar_dom::Node<'_>) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::forget_node(js, node.as_ptr());
    }
}

fn teardown_listeners(js: *const NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::teardown_listeners(js);
    }
}

fn teardown(js: *const NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::teardown(js);
    }
}

pub fn install_runtime_hooks() {
    southstar_js_runtime::events::set_event_hooks(southstar_js_runtime::events::EventHooks {
        dispatch_event: crate::api::dispatch_event,
        dispatch_built_event: crate::api::dispatch_built_event,
        dispatch_window_only_event: crate::api::dispatch_window_only_event,
        dispatch_resource_event: crate::api::dispatch_resource_event,
        window_for_document: crate::api::window_for_document,
        fire_element_handlers: crate::api::fire_element_handlers,
        report_uncaught: crate::api::report_uncaught,
        report_handler_exception: crate::api::report_handler_exception,
        init_flags: crate::api::init_flags,
    });
    southstar_js_runtime::api::set_dispatch_hooks(southstar_js_runtime::api::DispatchHooks {
        report_pending_rejections,
        drop_pending_rejections: crate::drop_pending_rejections,
        forget_node,
        teardown_listeners,
        teardown,
    });
    southstar_js_net::links::set_js_dispatch_links(southstar_js_net::links::JsDispatchLinks {
        target_dispatch_event: crate::ffi::exports::nsr_target_dispatchEvent,
        xhr_fire_progress_event: crate::ffi::exports::nsr_xhr_fire_progress_event,
        target_fire_event: crate::ffi::exports::nsr_target_fire_event,
    });
    southstar_js_runtime::links::set_js_dispatch_links(
        southstar_js_runtime::links::JsDispatchLinks {
            window_report_error: crate::ffi::exports::nsr_window_report_error,
            target_dispatch_event: crate::ffi::exports::nsr_target_dispatchEvent,
            window_dispatch_event: crate::ffi::exports::nsr_window_dispatchEvent,
            window_remove_event_listener: crate::ffi::exports::nsr_window_removeEventListener,
            window_add_event_listener: crate::ffi::exports::nsr_window_addEventListener,
            target_remove_event_listener: crate::ffi::exports::nsr_target_removeEventListener,
            target_add_event_listener: crate::ffi::exports::nsr_target_addEventListener,
            js_install_event_target: crate::ffi::exports::nsr_js_install_event_target,
            target_handler_realm: crate::ffi::exports::nsr_target_handler_realm,
            js_promise_rejection_tracker: crate::ffi::exports::nsr_js_promise_rejection_tracker,
            install_body_reflected_handlers:
                crate::ffi::exports::nsr_install_body_reflected_handlers,
        },
    );
    southstar_js_services::links::set_js_dispatch_links(
        southstar_js_services::links::JsDispatchLinks {
            target_dispatch_event: crate::ffi::exports::nsr_target_dispatchEvent,
            target_dispatch_with_event: crate::ffi::exports::nsr_target_dispatch_with_event,
            target_make_event: crate::ffi::exports::nsr_target_make_event,
            target_fire_event: crate::ffi::exports::nsr_target_fire_event,
        },
    );
    southstar_js_url::links::set_js_dispatch_links(southstar_js_url::links::JsDispatchLinks {
        xhr_fire_progress_event: crate::ffi::exports::nsr_xhr_fire_progress_event,
    });
    southstar_js_window::links::set_js_dispatch_links(
        southstar_js_window::links::JsDispatchLinks {
            target_dispatch_with_event: crate::ffi::exports::nsr_target_dispatch_with_event,
            target_make_event: crate::ffi::exports::nsr_target_make_event,
        },
    );
    southstar_js_frames::links::set_js_dispatch_links(
        southstar_js_frames::links::JsDispatchLinks {
            element_dispatch_event: crate::ffi::exports::nsr_element_dispatchEvent,
            element_remove_event_listener: crate::ffi::exports::nsr_element_removeEventListener,
            element_add_event_listener: crate::ffi::exports::nsr_element_addEventListener,
            window_dispatch_event: crate::ffi::exports::nsr_window_dispatchEvent,
            window_remove_event_listener: crate::ffi::exports::nsr_window_removeEventListener,
            window_add_event_listener: crate::ffi::exports::nsr_window_addEventListener,
        },
    );
}
