//! Southstar — the event core's nsr_ entry points, and the js.c and binding calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Node, NsNode};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, JsResult};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

unsafe extern "Rust" {
    safe fn nsr_submit_event_ctor(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}

fn js_of(scope: &Scope<'_>) -> *mut NsJs {
    scope.context_opaque().cast()
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    api::make_element(scope, node.as_ptr()).unwrap_or_else(|_| Value::null())
}

pub(crate) fn event_state(_scope: &mut Scope<'_>, value: &Value) -> Option<Value> {
    crate::class::state_of(value).filter(Value::is_object)
}

pub(crate) fn new_event(scope: &mut Scope<'_>) -> Value {
    crate::class::new_event(scope, None).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn new_event_with_proto(scope: &mut Scope<'_>, proto: &Value) -> JsResult {
    crate::class::new_event(scope, Some(proto))
}

pub(crate) fn has_page(scope: &Scope<'_>) -> bool {
    !js_of(scope).is_null()
}

pub(crate) fn html_element_construct(scope: &mut Scope<'_>, new_target: &Value) -> JsResult {
    Ok(southstar_js_custom_elements::html_element_construct(
        scope, new_target,
    ))
}

pub(crate) fn realm_now_ms(scope: &mut Scope<'_>) -> f64 {
    southstar_js_perf::scope_now_ms(scope)
}

pub(crate) fn submit_event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    nsr_submit_event_ctor(scope, this, args)
}

pub(crate) fn dispatch_path(scope: &mut Scope<'_>) -> Option<(Vec<Element>, bool)> {
    southstar_js_dispatch::current_dispatch_path(scope)
}

pub fn nsr_event_new(scope: &mut Scope<'_>) -> JsResult {
    crate::class::new_event(scope, None)
}

macro_rules! natives {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

natives! {
    nsr_event_ctor => crate::ctors::event_ctor,
    nsr_window_event_ctor => crate::ctors::window_event_ctor,
    nsr_event_prevent_default => crate::event::prevent_default,
    nsr_event_stop_propagation => crate::event::stop_propagation,
    nsr_event_stop_immediate => crate::event::stop_immediate,
    nsr_event_composed_path => crate::event::composed_path,
}

pub fn nsr_event_define_cancel_bubble(scope: &mut Scope<'_>, event: &Value) {
    crate::event::define_cancel_bubble(scope, event);
}

pub fn nsr_event_mark_default_prevented(scope: &mut Scope<'_>, event: &Value) {
    crate::event::mark_default_prevented(scope, event);
}

/// # Safety
///
/// `target` is null or points at a live node.
#[unsafe(no_mangle)]
pub unsafe fn nsr_make_event(scope: &mut Scope<'_>, kind: &str, target: *const NsNode) -> Value {
    crate::event::make_event(scope, kind, unsafe { Node::from_ptr(target) })
}

pub fn nsr_event_adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    crate::event::adopt_interface(scope, event, iface);
}

pub fn nsr_event_define_source(scope: &mut Scope<'_>, event: &Value, source: Value) {
    crate::event::define_source(scope, event, source);
}

pub fn nsr_install_event_attribute_getters(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_attribute_getters(scope, global);
}

pub fn nsr_events_install_window_base(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_window_base(scope, global);
}

pub fn nsr_events_install_window(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_window(scope, global);
}

pub fn nsr_events_install_worker(scope: &mut Scope<'_>, global: &Value) {
    crate::install::install_worker(scope, global);
}

pub fn nsr_dom_exception(scope: &mut Scope<'_>, name: &[u8], code: i32, message: &[u8]) -> Value {
    crate::exceptions::dom_exception(scope, name, code, message)
}

pub fn nsr_make_abort_error(scope: &mut Scope<'_>) -> Value {
    crate::exceptions::abort_error(scope)
}
