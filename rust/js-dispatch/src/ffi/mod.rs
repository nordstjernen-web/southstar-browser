//! Southstar — the engine-neutral exports of EventTarget and event dispatch, and the page, DOM and binding calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod exports;
mod imports;

use core::ffi::{CStr, c_char, c_int, c_uint};
use core::ptr;
use std::borrow::Cow;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, get, set};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

#[repr(C)]
pub(crate) struct CheckableState {
    checked: GBoolean,
    indeterminate: GBoolean,
    checked_radio: *mut NsNode,
}

const DLOG_ERROR: c_uint = 2;

unsafe extern "C" {
    fn ns_js_realm_for_node(js: *mut NsJs, node: *const NsNode) -> ContextHandle;
    fn ns_js_frame_context(js: *mut NsJs, frame: *const NsNode) -> ContextHandle;
    fn ns_iframe_is_cross_origin(js: *mut NsJs, frame: *const NsNode) -> GBoolean;
    fn ns_window_named_suspend();
    fn ns_window_named_resume();
    fn ns_node_is_shadow_root(n: *const NsNode) -> GBoolean;
    fn ns_checkable_input_kind(el: *const NsNode) -> c_int;
    fn ns_checkable_pre_click(
        js: *mut NsJs,
        el: *mut NsNode,
        kind: c_int,
        state: *mut CheckableState,
    );
    fn ns_checkable_post_click(
        js: *mut NsJs,
        el: *mut NsNode,
        kind: c_int,
        state: *const CheckableState,
        prevented: GBoolean,
    );
    fn ns_click_activation_target(el: *const NsNode) -> *const NsNode;
    fn ns_node_has_activation_behavior(el: *const NsNode) -> GBoolean;
    fn ns_js_synthetic_activation(js: *mut NsJs, act: *const NsNode, target: *const NsNode);
    fn ns_debug_log_emit_take(level: c_uint, category: *const c_char, message: *mut c_char);
}

pub(crate) fn node(ptr: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(ptr) }
}

fn opt_text<'a>(text: *const c_char) -> Option<Cow<'a, str>> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_string_lossy())
}

fn with_c<R>(text: &str, f: impl FnOnce(*const c_char) -> R) -> R {
    let owned = std::ffi::CString::new(text.replace('\0', " ")).unwrap_or_default();
    f(owned.as_ptr())
}

impl Js {
    pub fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    pub fn ctx(self) -> ContextHandle {
        api::main_context(self.ptr())
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        self.ctx().with(f)
    }

    pub fn main_realm(self) -> ContextHandle {
        api::main_realm(self.ptr())
    }

    pub fn current_document(self) -> Option<Element> {
        api::current_document(self.ptr())
    }

    pub fn log_enabled(self) -> bool {
        api::log_enabled(self.ptr())
    }

    pub fn log(self, line: &str) {
        if !self.log_enabled() {
            return;
        }
        let line = std::ffi::CString::new(line.replace('\0', " ")).unwrap_or_default();
        api::log_line(self.ptr(), &line);
    }

    pub fn halted(self) -> bool {
        api::halted(self.ptr())
    }

    pub fn in_pump(self) -> bool {
        api::in_pump(self.ptr())
    }

    pub fn current_url(self) -> Option<String> {
        api::page_url(self.ptr()).map(|url| url.to_string_lossy().into_owned())
    }

    pub fn inline_handlers_allowed(self) -> bool {
        api::inline_handlers_allowed(self.ptr())
    }

    pub fn is_worker(self) -> bool {
        api::is_worker(self.ptr())
    }

    pub fn realm_for_node(self, node: Element) -> ContextHandle {
        unsafe { ns_js_realm_for_node(self.ptr(), node.as_ptr()) }
    }

    pub fn frame_context(self, frame: Element) -> ContextHandle {
        unsafe { ns_js_frame_context(self.ptr(), frame.as_ptr()) }
    }

    pub fn frame_realm_window(self, frame: Element) -> Value {
        unsafe { imports::nsr_js_frame_realm_window(self.ptr(), frame.as_ptr()) }
    }

    pub fn iframe_is_cross_origin(self, frame: Element) -> bool {
        unsafe { ns_iframe_is_cross_origin(self.ptr(), frame.as_ptr()) != 0 }
    }

    pub fn set_realm(self, ctx: ContextHandle, doc: Option<Element>) {
        api::set_realm(self.ptr(), ctx, doc);
    }

    pub fn in_dispatch_scope<R>(
        self,
        ctx: ContextHandle,
        frame: Option<(Element, Element)>,
        f: impl FnOnce() -> R,
    ) -> R {
        api::in_dispatch_scope(self.ptr(), ctx, frame, f)
    }

    pub fn in_realm_scope<R>(self, realm: ContextHandle, f: impl FnOnce() -> R) -> R {
        api::in_realm(self.ptr(), realm, f)
    }

    pub fn dispatch_depth_add(self, delta: i32) {
        api::dispatch_depth_add(self.ptr(), delta);
    }

    pub fn microtask_checkpoint(self) {
        api::microtask_checkpoint(self.ptr());
    }

    pub fn finish_dispatch(self) {
        api::dispatch_finish(self.ptr());
    }

    pub fn budget_enter(self) -> i64 {
        api::budget_enter(self.ptr())
    }

    pub fn budget_leave(self, saved: i64) {
        api::budget_leave(self.ptr(), saved);
    }

    pub fn console_emit(self, scope: &mut Scope<'_>, prefix: &str, value: &Value) {
        imports::nsr_services_console_emit(self.ptr(), scope, prefix, core::slice::from_ref(value));
    }

    pub fn worker_report_exception(self, exception: &Value) -> bool {
        imports::nsr_worker_report_exception(self.ptr().cast(), exception)
    }

    pub fn checkable_pre_click(self, el: Element, kind: i32) -> CheckableState {
        let mut state = CheckableState {
            checked: 0,
            indeterminate: 0,
            checked_radio: ptr::null_mut(),
        };
        unsafe { ns_checkable_pre_click(self.ptr(), el.as_mut_ptr(), kind, &mut state) };
        state
    }

    pub fn checkable_post_click(
        self,
        el: Element,
        kind: i32,
        state: &CheckableState,
        prevented: bool,
    ) {
        unsafe {
            ns_checkable_post_click(
                self.ptr(),
                el.as_mut_ptr(),
                kind,
                state,
                glib::boolean(prevented),
            )
        };
    }

    pub fn note_user_activation(self) {
        api::note_user_activation(self.ptr());
    }

    pub fn synthetic_activation(self, act: Element, target: Element) {
        unsafe { ns_js_synthetic_activation(self.ptr(), act.as_ptr(), target.as_ptr()) };
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(scope.context_opaque().cast())
}

pub(crate) fn ctx_of(scope: &Scope<'_>) -> ContextHandle {
    scope.handle()
}

pub(crate) fn in_context<R>(ctx: ContextHandle, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    ctx.with(f)
}

pub(crate) fn unwrap(value: &Value) -> Option<Element> {
    if !value.is_object() {
        return None;
    }
    api::unwrap_element(value)
}

pub(crate) fn wrap(scope: &mut Scope<'_>, el: Element) -> Value {
    api::make_element(scope, el.as_ptr()).unwrap_or_else(|_| Value::null())
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str, target: Option<Element>) -> Value {
    unsafe { imports::nsr_make_event(scope, kind, Node::ptr_or_null(target)) }
}

pub(crate) fn new_event(scope: &mut Scope<'_>) -> Value {
    imports::nsr_event_new(scope).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn mark_default_prevented(scope: &mut Scope<'_>, event: &Value) {
    imports::nsr_event_mark_default_prevented(scope, event);
}

pub(crate) fn at_target<R>(
    scope: &mut Scope<'_>,
    event: &Value,
    target: &Value,
    f: impl FnOnce(&mut Scope<'_>) -> R,
) -> R {
    let dispatching = get(scope, event, "_dispatching");
    let nested = scope.to_bool(&dispatching);
    let phase = get(scope, event, "eventPhase");
    let current = get(scope, event, "currentTarget");
    set(scope, event, "currentTarget", target.clone());
    set(scope, event, "eventPhase", Value::int(2));
    set(scope, event, "_dispatching", Value::boolean(true));
    let result = f(scope);
    if nested {
        set(scope, event, "eventPhase", phase);
        set(scope, event, "currentTarget", current);
    } else {
        set(scope, event, "eventPhase", Value::int(0));
        set(scope, event, "currentTarget", Value::null());
        set(scope, event, "_dispatching", Value::boolean(false));
    }
    result
}

pub(crate) fn bind_event_methods(scope: &mut Scope<'_>, event: &Value) {
    southstar_js_bind::bind_native(
        scope,
        event,
        "preventDefault",
        0,
        imports::nsr_event_prevent_default,
    );
    southstar_js_bind::bind_native(
        scope,
        event,
        "stopPropagation",
        0,
        imports::nsr_event_stop_propagation,
    );
    imports::nsr_event_define_cancel_bubble(scope, event);
}

pub(crate) fn bind_stop_immediate(scope: &mut Scope<'_>, event: &Value) {
    southstar_js_bind::bind_native(
        scope,
        event,
        "stopImmediatePropagation",
        0,
        imports::nsr_event_stop_immediate,
    );
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str) {
    imports::nsr_event_adopt_interface(scope, event, iface);
}

pub(crate) fn define_source(scope: &mut Scope<'_>, event: &Value, source: Value) {
    imports::nsr_event_define_source(scope, event, source);
}

pub(crate) fn invalid_state(scope: &mut Scope<'_>, message: &str) -> Value {
    imports::nsr_dom_exception(scope, b"InvalidStateError", 11, message.as_bytes())
}

pub(crate) fn document_root_for(scope: &mut Scope<'_>, this: &Value) -> Option<Element> {
    node(api::document_root_for(scope, this))
}

pub(crate) fn window_document_for(scope: &mut Scope<'_>, window: &Value) -> Option<Element> {
    node(imports::nsr_window_document_for(scope, window))
}

pub(crate) fn suspend_named<R>(f: impl FnOnce() -> R) -> R {
    unsafe { ns_window_named_suspend() };
    let result = f();
    unsafe { ns_window_named_resume() };
    result
}

pub(crate) fn arm_invalidate(el: Element) {
    api::arm_js_invalidate(el);
}

pub(crate) fn first_element(root: Element, tag: &CStr) -> Option<Element> {
    southstar_dom::index::find_first_element(root, tag)
}

pub(crate) fn is_shadow_root(n: Element) -> bool {
    unsafe { ns_node_is_shadow_root(n.as_ptr()) != 0 }
}

pub(crate) fn checkable_input_kind(el: Element) -> i32 {
    unsafe { ns_checkable_input_kind(el.as_ptr()) }
}

pub(crate) fn click_activation_target(el: Element) -> Option<Element> {
    node(unsafe { ns_click_activation_target(el.as_ptr()) })
}

pub(crate) fn has_activation_behavior(el: Element) -> bool {
    unsafe { ns_node_has_activation_behavior(el.as_ptr()) != 0 }
}

pub(crate) fn is_host_caller(scope: &mut Scope<'_>) -> bool {
    scope.is_host_caller()
}

pub(crate) fn function_realm(scope: &mut Scope<'_>, function: &Value) -> ContextHandle {
    if !scope.is_function(function) {
        return ctx_of(scope);
    }
    scope
        .function_handle(function)
        .unwrap_or_else(|_| ctx_of(scope))
}

pub(crate) fn global_of(ctx: ContextHandle) -> Value {
    in_context(ctx, |scope| scope.global()).unwrap_or_else(Value::undefined)
}

pub(crate) fn debug_error(category: &CStr, message: &str) {
    with_c(message, |message| unsafe {
        ns_debug_log_emit_take(DLOG_ERROR, category.as_ptr(), glib::g_strdup(message))
    });
}

fn handler_index(scope: &mut Scope<'_>, data: &[Value]) -> usize {
    data.first()
        .and_then(|index| scope.to_int32(index).ok())
        .map_or(usize::MAX, |index| index as usize)
}

fn handler_getter(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let index = handler_index(scope, data);
    crate::handlers::on_get(scope, this, index)
}

fn handler_setter(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let index = handler_index(scope, data);
    crate::handlers::on_set(scope, this, args, index)
}

pub(crate) fn handler_accessors(scope: &mut Scope<'_>, name: &str, index: usize) -> (Value, Value) {
    let data = [Value::int(index as i32)];
    (
        scope.bound_function(name, 0, handler_getter, &data),
        scope.bound_function(name, 1, handler_setter, &data),
    )
}
