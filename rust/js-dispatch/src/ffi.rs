//! Southstar — the C ABI of EventTarget and event dispatch as declared in src/js.h and src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::borrow::Cow;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::{Element, JsResult};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

#[repr(C)]
pub(crate) struct CheckableState {
    checked: GBoolean,
    indeterminate: GBoolean,
    checked_radio: *mut NsNode,
}

#[repr(C)]
struct EventAtTarget {
    phase: JSValue,
    current: JSValue,
    nested: c_int,
}

#[repr(C, align(16))]
struct ScopeBuffer([u8; 128]);

impl ScopeBuffer {
    fn new() -> ScopeBuffer {
        ScopeBuffer([0; 128])
    }

    fn as_mut_ptr(&mut self) -> *mut c_void {
        self.0.as_mut_ptr().cast()
    }
}

type JSCFunction = unsafe extern "C" fn(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue;

type JSCFunctionMagic = unsafe extern "C" fn(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    magic: c_int,
) -> JSValue;

const DLOG_ERROR: c_uint = 2;
const CFUNC_GENERIC_MAGIC: c_int = 1;

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_make_event(ctx: *mut JSContext, kind: *const c_char, target: *const NsNode) -> JSValue;
    fn ns_event_new(ctx: *mut JSContext) -> JSValue;
    fn ns_event_mark_default_prevented(ctx: *mut JSContext, ev: JSValue);
    fn ns_event_at_target_begin(
        ctx: *mut JSContext,
        ev: JSValue,
        target: JSValue,
        st: *mut EventAtTarget,
    );
    fn ns_event_at_target_end(ctx: *mut JSContext, ev: JSValue, st: *mut EventAtTarget);
    fn ns_event_prevent_default(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_stop_propagation(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_stop_immediate(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_event_define_cancel_bubble(ctx: *mut JSContext, ev: JSValue);
    fn ns_event_adopt_interface(ctx: *mut JSContext, ev: JSValue, iface: *const c_char);
    fn ns_event_define_source(ctx: *mut JSContext, ev: JSValue, source: JSValue);
    fn ns_js_note_user_activation(js: *mut NsJs);
    fn ns_bind_fn(
        ctx: *mut JSContext,
        obj: JSValue,
        name: *const c_char,
        f: JSCFunction,
        argc: c_int,
    );
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_document_root_for(ctx: *mut JSContext, this_val: JSValue) -> *mut NsNode;
    fn ns_window_document_for(ctx: *mut JSContext, window: JSValue) -> *mut NsNode;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_main_realm(js: *const NsJs) -> *mut JSContext;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_log_enabled(js: *const NsJs) -> GBoolean;
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_in_pump(js: *const NsJs) -> GBoolean;
    fn ns_js_net_page_url(js: *const NsJs) -> *const c_char;
    fn ns_js_inline_handlers_allowed(js: *const NsJs) -> GBoolean;
    fn ns_js_is_worker(js: *const NsJs) -> GBoolean;
    fn ns_js_realm_for_node(js: *mut NsJs, node: *const NsNode) -> *mut JSContext;
    fn ns_js_frame_context(js: *mut NsJs, frame: *const NsNode) -> *mut JSContext;
    fn ns_js_frame_realm_window(js: *mut NsJs, frame: *const NsNode) -> JSValue;
    fn ns_iframe_is_cross_origin(js: *mut NsJs, frame: *const NsNode) -> GBoolean;
    fn ns_js_set_realm(js: *mut NsJs, ctx: *mut JSContext, doc: *const NsNode);
    fn ns_js_dispatch_scope_enter(
        js: *mut NsJs,
        ctx: *mut JSContext,
        doc: *const NsNode,
        frame: *const NsNode,
        buf: *mut c_void,
    );
    fn ns_js_dispatch_scope_leave(js: *mut NsJs, buf: *mut c_void);
    fn ns_js_realm_scope_push(js: *mut NsJs, realm: *mut JSContext, buf: *mut c_void);
    fn ns_js_realm_scope_pop(js: *mut NsJs, buf: *mut c_void);
    fn ns_js_dispatch_depth_add(js: *mut NsJs, delta: c_int);
    fn ns_js_microtask_checkpoint(js: *mut NsJs);
    fn ns_js_dispatch_finish(js: *mut NsJs);
    fn ns_js_budget_enter(js: *mut NsJs) -> i64;
    fn ns_js_budget_leave(js: *mut NsJs, saved: i64);
    fn ns_window_named_suspend();
    fn ns_window_named_resume();
    fn ns_node_arm_js_invalidate(n: *mut NsNode);
    fn ns_node_find_first_element(root: *const NsNode, tag: *const c_char) -> *mut NsNode;
    fn ns_node_is_shadow_root(n: *const NsNode) -> GBoolean;
    fn ns_services_console_emit(
        js: *mut NsJs,
        prefix: *const c_char,
        ctx: *mut JSContext,
        argc: c_int,
        argv: *mut JSValue,
    );
    fn ns_worker_report_exception(js: *mut NsJs, ex: JSValue) -> GBoolean;
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
    fn JS_IsHostCaller(ctx: *mut JSContext) -> bool;
    fn JS_NewCFunction2(
        ctx: *mut JSContext,
        func: JSCFunction,
        name: *const c_char,
        length: c_int,
        cproto: c_int,
        magic: c_int,
    ) -> JSValue;
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

    pub fn ctx(self) -> *mut JSContext {
        if self.is_null() {
            return ptr::null_mut();
        }
        unsafe { ns_js_main_context(self.ptr()) }
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        let ctx = self.ctx();
        if ctx.is_null() {
            return None;
        }
        Some(unsafe { quickjs::with_context(ctx, f) })
    }

    pub fn main_realm(self) -> *mut JSContext {
        unsafe { ns_js_main_realm(self.ptr()) }
    }

    pub fn current_document(self) -> Option<Element> {
        node(unsafe { ns_js_current_document(self.ptr()) })
    }

    pub fn log_enabled(self) -> bool {
        unsafe { ns_js_log_enabled(self.ptr()) != 0 }
    }

    pub fn log(self, line: &str) {
        if !self.log_enabled() {
            return;
        }
        with_c(line, |line| unsafe { ns_js_log_line(self.ptr(), line) });
    }

    pub fn halted(self) -> bool {
        unsafe { ns_js_halted(self.ptr()) != 0 }
    }

    pub fn in_pump(self) -> bool {
        unsafe { ns_js_in_pump(self.ptr()) != 0 }
    }

    pub fn current_url(self) -> Option<String> {
        opt_text(unsafe { ns_js_net_page_url(self.ptr()) }).map(Cow::into_owned)
    }

    pub fn inline_handlers_allowed(self) -> bool {
        unsafe { ns_js_inline_handlers_allowed(self.ptr()) != 0 }
    }

    pub fn is_worker(self) -> bool {
        unsafe { ns_js_is_worker(self.ptr()) != 0 }
    }

    pub fn realm_for_node(self, node: Element) -> *mut JSContext {
        unsafe { ns_js_realm_for_node(self.ptr(), node.as_ptr()) }
    }

    pub fn frame_context(self, frame: Element) -> *mut JSContext {
        unsafe { ns_js_frame_context(self.ptr(), frame.as_ptr()) }
    }

    pub fn frame_realm_window(self, scope: &mut Scope<'_>, frame: Element) -> Value {
        let raw = unsafe { ns_js_frame_realm_window(self.ptr(), frame.as_ptr()) };
        unsafe { quickjs::take_value(scope, raw) }
    }

    pub fn iframe_is_cross_origin(self, frame: Element) -> bool {
        unsafe { ns_iframe_is_cross_origin(self.ptr(), frame.as_ptr()) != 0 }
    }

    pub fn set_realm(self, ctx: *mut JSContext, doc: Option<Element>) {
        unsafe { ns_js_set_realm(self.ptr(), ctx, Node::ptr_or_null(doc)) };
    }

    pub fn in_dispatch_scope<R>(
        self,
        ctx: *mut JSContext,
        frame: Option<(Element, Element)>,
        f: impl FnOnce() -> R,
    ) -> R {
        let mut buffer = ScopeBuffer::new();
        let (doc, frame) = frame.map_or((ptr::null(), ptr::null()), |(doc, frame)| {
            (doc.as_ptr(), frame.as_ptr())
        });
        unsafe { ns_js_dispatch_scope_enter(self.ptr(), ctx, doc, frame, buffer.as_mut_ptr()) };
        let result = f();
        unsafe { ns_js_dispatch_scope_leave(self.ptr(), buffer.as_mut_ptr()) };
        result
    }

    pub fn in_realm_scope<R>(self, realm: *mut JSContext, f: impl FnOnce() -> R) -> R {
        if self.is_null() {
            return f();
        }
        let mut buffer = ScopeBuffer::new();
        unsafe { ns_js_realm_scope_push(self.ptr(), realm, buffer.as_mut_ptr()) };
        let result = f();
        unsafe { ns_js_realm_scope_pop(self.ptr(), buffer.as_mut_ptr()) };
        result
    }

    pub fn dispatch_depth_add(self, delta: i32) {
        unsafe { ns_js_dispatch_depth_add(self.ptr(), delta) };
    }

    pub fn microtask_checkpoint(self) {
        if !self.is_null() {
            unsafe { ns_js_microtask_checkpoint(self.ptr()) };
        }
    }

    pub fn finish_dispatch(self) {
        unsafe { ns_js_dispatch_finish(self.ptr()) };
    }

    pub fn budget_enter(self) -> i64 {
        unsafe { ns_js_budget_enter(self.ptr()) }
    }

    pub fn budget_leave(self, saved: i64) {
        unsafe { ns_js_budget_leave(self.ptr(), saved) };
    }

    pub fn console_emit(self, scope: &mut Scope<'_>, prefix: &str, value: &Value) {
        let mut raw = [quickjs::raw(value)];
        let ctx = quickjs::raw_context(scope);
        with_c(prefix, |prefix| unsafe {
            ns_services_console_emit(self.ptr(), prefix, ctx, 1, raw.as_mut_ptr())
        });
    }

    pub fn worker_report_exception(self, exception: &Value) -> bool {
        unsafe { ns_worker_report_exception(self.ptr(), quickjs::raw(exception)) != 0 }
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
        unsafe { ns_js_note_user_activation(self.ptr()) };
    }

    pub fn synthetic_activation(self, act: Element, target: Element) {
        unsafe { ns_js_synthetic_activation(self.ptr(), act.as_ptr(), target.as_ptr()) };
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(quickjs::context_opaque(scope).cast())
}

pub(crate) fn ctx_of(scope: &Scope<'_>) -> *mut JSContext {
    quickjs::raw_context(scope)
}

pub(crate) fn in_context<R>(ctx: *mut JSContext, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
    unsafe { quickjs::with_context(ctx, f) }
}

pub(crate) fn unwrap(value: &Value) -> Option<Element> {
    if !value.is_object() {
        return None;
    }
    node(unsafe { ns_unwrap_element(quickjs::raw(value)) })
}

pub(crate) fn wrap(scope: &mut Scope<'_>, el: Element) -> Value {
    let raw = unsafe { ns_make_element(ctx_of(scope), el.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str, target: Option<Element>) -> Value {
    let ctx = ctx_of(scope);
    let raw = with_c(kind, |kind| unsafe {
        ns_make_event(ctx, kind, Node::ptr_or_null(target))
    });
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn new_event(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_event_new(ctx_of(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn mark_default_prevented(scope: &mut Scope<'_>, event: &Value) {
    unsafe { ns_event_mark_default_prevented(ctx_of(scope), quickjs::raw(event)) };
}

pub(crate) fn at_target<R>(
    scope: &mut Scope<'_>,
    event: &Value,
    target: &Value,
    f: impl FnOnce(&mut Scope<'_>) -> R,
) -> R {
    let ctx = ctx_of(scope);
    let mut state = EventAtTarget {
        phase: quickjs::UNDEFINED,
        current: quickjs::UNDEFINED,
        nested: 0,
    };
    unsafe { ns_event_at_target_begin(ctx, quickjs::raw(event), quickjs::raw(target), &mut state) };
    let result = f(scope);
    unsafe { ns_event_at_target_end(ctx, quickjs::raw(event), &mut state) };
    result
}

pub(crate) fn bind_event_methods(scope: &mut Scope<'_>, event: &Value) {
    let ctx = ctx_of(scope);
    let raw = quickjs::raw(event);
    unsafe {
        ns_bind_fn(
            ctx,
            raw,
            c"preventDefault".as_ptr(),
            ns_event_prevent_default,
            0,
        );
        ns_bind_fn(
            ctx,
            raw,
            c"stopPropagation".as_ptr(),
            ns_event_stop_propagation,
            0,
        );
        ns_event_define_cancel_bubble(ctx, raw);
    }
}

pub(crate) fn bind_stop_immediate(scope: &mut Scope<'_>, event: &Value) {
    let ctx = ctx_of(scope);
    unsafe {
        ns_bind_fn(
            ctx,
            quickjs::raw(event),
            c"stopImmediatePropagation".as_ptr(),
            ns_event_stop_immediate,
            0,
        )
    };
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &CStr) {
    unsafe { ns_event_adopt_interface(ctx_of(scope), quickjs::raw(event), iface.as_ptr()) };
}

pub(crate) fn define_source(scope: &mut Scope<'_>, event: &Value, source: Value) {
    unsafe {
        ns_event_define_source(
            ctx_of(scope),
            quickjs::raw(event),
            quickjs::into_raw(source),
        )
    };
}

pub(crate) fn invalid_state(scope: &mut Scope<'_>, message: &CStr) -> Value {
    let raw = unsafe {
        ns_throw_dom_exception(
            ctx_of(scope),
            c"InvalidStateError".as_ptr(),
            11,
            message.as_ptr(),
        )
    };
    let thrown = unsafe { quickjs::take_value(scope, raw) };
    match quickjs::checked(scope, thrown) {
        Ok(value) | Err(value) => value,
    }
}

pub(crate) fn document_root_for(scope: &mut Scope<'_>, this: &Value) -> Option<Element> {
    node(unsafe { ns_document_root_for(ctx_of(scope), quickjs::raw(this)) })
}

pub(crate) fn window_document_for(scope: &mut Scope<'_>, window: &Value) -> Option<Element> {
    node(unsafe { ns_window_document_for(ctx_of(scope), quickjs::raw(window)) })
}

pub(crate) fn suspend_named<R>(f: impl FnOnce() -> R) -> R {
    unsafe { ns_window_named_suspend() };
    let result = f();
    unsafe { ns_window_named_resume() };
    result
}

pub(crate) fn arm_invalidate(el: Element) {
    unsafe { ns_node_arm_js_invalidate(el.as_mut_ptr()) };
}

pub(crate) fn first_element(root: Element, tag: &CStr) -> Option<Element> {
    node(unsafe { ns_node_find_first_element(root.as_ptr(), tag.as_ptr()) })
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

pub(crate) fn is_host_caller(scope: &Scope<'_>) -> bool {
    unsafe { JS_IsHostCaller(ctx_of(scope)) }
}

pub(crate) fn function_realm(scope: &mut Scope<'_>, function: &Value) -> *mut JSContext {
    if !scope.is_function(function) {
        return ctx_of(scope);
    }
    quickjs::function_realm(scope, function).unwrap_or_else(|_| ctx_of(scope))
}

pub(crate) fn global_of(ctx: *mut JSContext) -> Value {
    in_context(ctx, |scope| scope.global())
}

pub(crate) fn debug_error(category: &CStr, message: &str) {
    with_c(message, |message| unsafe {
        ns_debug_log_emit_take(DLOG_ERROR, category.as_ptr(), glib::g_strdup(message))
    });
}

unsafe extern "C" fn handler_getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, argc, argv, |scope, this, _| {
            crate::handlers::on_get(scope, this, magic as usize)
        })
    }
}

unsafe extern "C" fn handler_setter(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, argc, argv, |scope, this, args| {
            crate::handlers::on_set(scope, this, args, magic as usize)
        })
    }
}

fn magic_function(
    scope: &mut Scope<'_>,
    name: &CStr,
    arity: c_int,
    f: JSCFunctionMagic,
    magic: usize,
) -> Value {
    let generic: JSCFunction = unsafe { core::mem::transmute::<JSCFunctionMagic, JSCFunction>(f) };
    let raw = unsafe {
        JS_NewCFunction2(
            ctx_of(scope),
            generic,
            name.as_ptr(),
            arity,
            CFUNC_GENERIC_MAGIC,
            magic as c_int,
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn handler_accessors(scope: &mut Scope<'_>, name: &str, index: usize) -> (Value, Value) {
    crate::with_c_key(&[name], |name| {
        (
            magic_function(scope, name, 0, handler_getter, index),
            magic_function(scope, name, 1, handler_setter, index),
        )
    })
}

pub(crate) fn take_exception(scope: &mut Scope<'_>) -> Value {
    quickjs::take_exception(scope)
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

fn borrowed(scope: &Scope<'_>, raw: JSValue) -> Value {
    unsafe { quickjs::borrow_value(scope, raw) }
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
    ns_document_addEventListener => crate::listeners::document_add;
    ns_document_removeEventListener => crate::listeners::document_remove;
    ns_window_addEventListener => crate::listeners::window_add;
    ns_window_removeEventListener => crate::listeners::window_remove;
    ns_target_addEventListener => crate::target::add;
    ns_target_removeEventListener => crate::target::remove;
    ns_element_dispatchEvent => crate::target::element_dispatch;
    ns_document_dispatchEvent => crate::target::document_dispatch;
    ns_window_dispatchEvent => crate::target::window_dispatch;
    ns_target_dispatchEvent => crate::target::dispatch;
    ns_window_report_error => crate::errors::report_error;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_listener_parse_options(
    ctx: *mut JSContext,
    opts: JSValue,
    capture: *mut GBoolean,
    once: *mut GBoolean,
    passive: *mut GBoolean,
    passive_set: *mut GBoolean,
    signal_out: *mut JSValue,
    strict_signal: GBoolean,
) -> GBoolean {
    let parsed = in_context(ctx, |scope| {
        let opts = borrowed(scope, opts);
        match crate::listeners::parse_options(scope, &opts, strict_signal != 0) {
            Ok(parsed) => Ok(parsed),
            Err(error) => Err(quickjs::result_raw(scope, Err(error))),
        }
    });
    let put = |slot: *mut GBoolean, value: bool| {
        if let Some(slot) = unsafe { slot.as_mut() } {
            *slot = glib::boolean(value);
        }
    };
    put(capture, false);
    put(once, false);
    put(passive, false);
    put(passive_set, false);
    if let Some(slot) = unsafe { signal_out.as_mut() } {
        *slot = quickjs::into_raw(Value::null());
    }
    let Ok(parsed) = parsed else {
        return glib::FALSE;
    };
    put(capture, parsed.capture);
    put(once, parsed.once);
    put(passive, parsed.passive);
    put(passive_set, parsed.passive_set);
    if let (Some(slot), Some(signal)) = (unsafe { signal_out.as_mut() }, parsed.signal) {
        *slot = quickjs::into_raw(signal);
    }
    glib::TRUE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_listeners_compact_dead(ctx: *mut JSContext, owner: JSValue) {
    in_context(ctx, |scope| {
        let owner = borrowed(scope, owner);
        crate::target::compact_dead(scope, &owner);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_target_handler_realm(
    ctx: *mut JSContext,
    obj: JSValue,
    kind: *const c_char,
    listener_key: *const c_char,
) -> *mut JSContext {
    let kind = opt_text(kind).unwrap_or_default();
    let key = opt_text(listener_key).unwrap_or_default();
    in_context(ctx, |scope| {
        let obj = borrowed(scope, obj);
        crate::target::handler_realm(scope, &obj, &kind, &key)
    })
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_engine_event(
    ctx: *mut JSContext,
    target: JSValue,
    ev: JSValue,
) {
    in_context(ctx, |scope| {
        let target = borrowed(scope, target);
        let ev = borrowed(scope, ev);
        crate::target::dispatch_engine_event(scope, &target, &ev);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_install_event_target(ctx: *mut JSContext, global: JSValue) {
    in_context(ctx, |scope| {
        let global = borrowed(scope, global);
        crate::target::install_event_target(scope, &global);
    });
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
    let event = in_context(js.ctx(), |scope| unsafe {
        quickjs::take_value(scope, event)
    });
    let (fired, prevented) = match node(target) {
        Some(target) => crate::dispatch::dispatch_built(js, target, &kind, event),
        None => (false, false),
    };
    if let Some(slot) = unsafe { default_prevented.as_mut() } {
        *slot = glib::boolean(prevented);
    }
    glib::boolean(fired)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_event(
    js: *mut NsJs,
    target: *const NsNode,
    kind: *const c_char,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    if let Some(slot) = unsafe { default_prevented.as_mut() } {
        *slot = glib::FALSE;
    }
    let js = Js::of(js);
    let (Some(target), Some(kind)) = (node(target), opt_text(kind)) else {
        return glib::FALSE;
    };
    if js.is_null() {
        return glib::FALSE;
    }
    let (fired, prevented) = crate::dispatch::dispatch_event(js, target, &kind);
    if let Some(slot) = unsafe { default_prevented.as_mut() } {
        *slot = glib::boolean(prevented);
    }
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
    let event = in_context(js.ctx(), |scope| unsafe {
        quickjs::take_value(scope, event)
    });
    let prevented = crate::dispatch::dispatch_window_only(js, node(target_doc), &kind, event);
    if let Some(slot) = unsafe { default_prevented.as_mut() } {
        *slot = glib::boolean(prevented);
    }
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
pub unsafe extern "C" fn ns_js_event_window_for_document(
    js: *mut NsJs,
    doc: *const NsNode,
) -> JSValue {
    let js = Js::of(js);
    js.scope(|scope| quickjs::into_raw(crate::dispatch::window_for_document(js, scope, node(doc))))
        .unwrap_or(quickjs::UNDEFINED)
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
    let event = in_context(js.ctx(), |scope| borrowed(scope, event));
    crate::handlers::fire_inline(js, element, &kind, &event);
    crate::handlers::fire_property(js, element, &kind, &event);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_event_handler_props(ctx: *mut JSContext, target: JSValue) {
    in_context(ctx, |scope| {
        let target = borrowed(scope, target);
        crate::handlers::install_props(scope, &target);
    });
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
    let origin = opt_text(origin).map(Cow::into_owned);
    let ex = in_context(js.ctx(), |scope| borrowed(scope, ex));
    crate::errors::report_exception_at(js, &ex, origin.as_deref(), 0, 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_report_error_event_in(
    js: *mut NsJs,
    ctx: *mut JSContext,
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
pub unsafe extern "C" fn ns_target_report_exception(
    js: *mut NsJs,
    ctx: *mut JSContext,
    kind: *const c_char,
) {
    let js = Js::of(js);
    let kind = opt_text(kind).map(Cow::into_owned);
    in_context(ctx, |scope| {
        let ex = take_exception(scope);
        crate::errors::report_handler_exception(js, scope, kind.as_deref(), &ex);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_promise_rejection_tracker(
    ctx: *mut JSContext,
    promise: JSValue,
    reason: JSValue,
    is_handled: bool,
    _opaque: *mut c_void,
) {
    in_context(ctx, |scope| {
        let promise = borrowed(scope, promise);
        let reason = borrowed(scope, reason);
        crate::errors::track_rejection(scope, &promise, &reason, is_handled);
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

fn put_prevented(slot: *mut GBoolean, prevented: bool) {
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
