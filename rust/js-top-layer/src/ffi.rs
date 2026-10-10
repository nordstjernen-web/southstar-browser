//! Southstar — the C ABI of the top layer as declared in src/js.h and src/js_internal.h, and the js.c and DOM calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::ffi::CString;

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

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_focused_node(js: *const NsJs) -> *const NsNode;
    fn ns_js_set_focus(js: *mut NsJs, el: *const NsNode);
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_in_pump(js: *const NsJs) -> GBoolean;
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_request_repaint(js: *mut NsJs);
    fn ns_js_autofocus_processed(js: *const NsJs) -> GBoolean;
    fn ns_js_set_autofocus_processed(js: *mut NsJs);
    fn ns_js_ready_state(js: *const NsJs) -> c_int;
    fn ns_js_set_active_modal(js: *mut NsJs, modal: *const NsNode);
    fn ns_js_set_attr_recorded_len(
        js: *mut NsJs,
        n: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
        len: isize,
    );
    fn ns_js_remove_attr_recorded(js: *mut NsJs, n: *mut NsNode, name: *const c_char);
    fn ns_js_fire_toggle_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        old_state: *const c_char,
        new_state: *const c_char,
        cancelable: GBoolean,
        source: *const NsNode,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_built_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        event: JSValue,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_make_event(ctx: *mut JSContext, kind: *const c_char, target: *const NsNode) -> JSValue;
    fn ns_event_define_source(ctx: *mut JSContext, ev: JSValue, source: JSValue);
    fn ns_event_adopt_interface(ctx: *mut JSContext, ev: JSValue, iface: *const c_char);
    fn ns_services_set_timeout(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_services_timer_remove(js: *mut NsJs, id: c_int);
    fn ns_js_consume_user_activation(js: *mut NsJs);
    fn ns_js_has_transient_activation(js: *mut NsJs) -> GBoolean;
    fn ns_element_effectively_disabled(el: *const NsNode) -> GBoolean;
    fn ns_node_is_focusable(el: *const NsNode) -> GBoolean;
    fn ns_node_tabindex(el: *const NsNode, out: *mut c_int) -> GBoolean;
    fn ns_node_assigned_slot_node(n: *const NsNode) -> *mut NsNode;
    fn ns_css_mark_attr_dirty(target: *mut NsNode, name: *const c_char, old_value: *const c_char);
    fn ns_node_is_submit_trigger(el: *const NsNode) -> GBoolean;
    fn ns_form_owner(control: *const NsNode, doc: *const NsNode) -> *const NsNode;
    fn ns_node_arm_js_invalidate(n: *mut NsNode);
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_element_remove_attr(el: *mut NsNode, name: *const c_char);
    fn ns_js_request_submit_form(
        ctx: *mut JSContext,
        form: *const NsNode,
        submitter: *const NsNode,
    ) -> JSValue;
    fn ns_js_reset_form(ctx: *mut JSContext, form: *mut NsNode) -> JSValue;
}

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn node(ptr: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(ptr) }
}

fn text(text: *const c_char) -> Option<&'static CStr> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) })
}

impl Js {
    fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    pub fn scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        if self.is_null() {
            return None;
        }
        let ctx = unsafe { ns_js_main_context(self.ptr()) };
        if ctx.is_null() {
            return None;
        }
        Some(unsafe { quickjs::with_context(ctx, f) })
    }

    pub fn hold(self, el: Element) -> Option<Value> {
        self.scope(|scope| wrap(scope, el))
    }

    pub fn current_document(self) -> Option<Element> {
        node(unsafe { ns_js_current_document(self.ptr()) })
    }

    pub fn current_url(self) -> &'static CStr {
        text(unsafe { ns_js_current_url(self.ptr()) }).unwrap_or(c"")
    }

    pub fn focused(self) -> Option<Element> {
        node(unsafe { ns_js_focused_node(self.ptr()) })
    }

    pub fn set_focus(self, el: Option<Element>) {
        unsafe { ns_js_set_focus(self.ptr(), Node::ptr_or_null(el)) };
    }

    pub fn halted(self) -> bool {
        unsafe { ns_js_halted(self.ptr()) != 0 }
    }

    pub fn in_pump(self) -> bool {
        unsafe { ns_js_in_pump(self.ptr()) != 0 }
    }

    pub fn mark_mutated(self) {
        unsafe { ns_js_mark_mutated(self.ptr()) };
    }

    pub fn request_repaint(self) {
        unsafe { ns_js_request_repaint(self.ptr()) };
    }

    pub fn autofocus_processed(self) -> bool {
        unsafe { ns_js_autofocus_processed(self.ptr()) != 0 }
    }

    pub fn set_autofocus_processed(self) {
        unsafe { ns_js_set_autofocus_processed(self.ptr()) };
    }

    pub fn ready_state(self) -> i32 {
        unsafe { ns_js_ready_state(self.ptr()) }
    }

    pub fn set_active_modal(self, modal: Option<Element>) {
        unsafe { ns_js_set_active_modal(self.ptr(), Node::ptr_or_null(modal)) };
    }

    pub fn set_attr_recorded(self, el: Element, name: &CStr, value: &[u8]) {
        let mut terminated = Vec::with_capacity(value.len() + 1);
        terminated.extend_from_slice(value);
        terminated.push(0);
        unsafe {
            ns_js_set_attr_recorded_len(
                self.ptr(),
                el.as_mut_ptr(),
                name.as_ptr(),
                terminated.as_ptr().cast(),
                value.len() as isize,
            )
        };
    }

    pub fn remove_attr_recorded(self, el: Element, name: &CStr) {
        unsafe { ns_js_remove_attr_recorded(self.ptr(), el.as_mut_ptr(), name.as_ptr()) };
    }

    pub fn fire_toggle(
        self,
        target: Element,
        kind: &CStr,
        old_state: &CStr,
        new_state: &CStr,
        cancelable: bool,
        source: Option<Element>,
    ) -> bool {
        let mut prevented: GBoolean = 0;
        unsafe {
            ns_js_fire_toggle_event(
                self.ptr(),
                target.as_ptr(),
                kind.as_ptr(),
                old_state.as_ptr(),
                new_state.as_ptr(),
                glib::boolean(cancelable),
                Node::ptr_or_null(source),
                &mut prevented,
            )
        };
        prevented != 0
    }

    pub fn dispatch_built(self, target: Element, kind: &CStr, event: Value) -> bool {
        let mut prevented: GBoolean = 0;
        unsafe {
            ns_js_dispatch_built_event(
                self.ptr(),
                target.as_ptr(),
                kind.as_ptr(),
                quickjs::into_raw(event),
                &mut prevented,
            )
        };
        prevented != 0
    }

    pub fn timer_remove(self, id: i32) {
        unsafe { ns_services_timer_remove(self.ptr(), id) };
    }

    pub fn consume_user_activation(self) {
        unsafe { ns_js_consume_user_activation(self.ptr()) };
    }

    pub fn has_transient_activation(self) -> bool {
        unsafe { ns_js_has_transient_activation(self.ptr()) != 0 }
    }

    pub fn form_owner(self, el: Element) -> Option<Element> {
        let doc = self.current_document().unwrap_or_else(|| el.root());
        node(unsafe { ns_form_owner(el.as_ptr(), doc.as_ptr()) })
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(quickjs::context_opaque(scope).cast())
}

pub(crate) fn unwrap(value: &Value) -> Option<Element> {
    if !value.is_object() {
        return None;
    }
    node(unsafe { ns_unwrap_element(quickjs::raw(value)) })
}

pub(crate) fn wrap(scope: &mut Scope<'_>, el: Element) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), el.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn wrap_or_null(scope: &mut Scope<'_>, el: Option<Element>) -> Value {
    el.map_or_else(Value::null, |el| wrap(scope, el))
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &CStr, code: i32, message: &str) -> Value {
    let message = c_string(message.as_bytes());
    unsafe {
        ns_throw_dom_exception(
            quickjs::raw_context(scope),
            name.as_ptr(),
            code,
            message.as_ptr(),
        )
    };
    quickjs::take_exception(scope)
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &CStr, target: Element) -> Value {
    let raw = unsafe { ns_make_event(quickjs::raw_context(scope), kind.as_ptr(), target.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn define_event_source(scope: &mut Scope<'_>, event: &Value, source: Value) {
    unsafe {
        ns_event_define_source(
            quickjs::raw_context(scope),
            quickjs::raw(event),
            quickjs::into_raw(source),
        )
    };
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &CStr) {
    unsafe {
        ns_event_adopt_interface(
            quickjs::raw_context(scope),
            quickjs::raw(event),
            iface.as_ptr(),
        )
    };
}

pub(crate) fn set_timeout(scope: &mut Scope<'_>, callback: Value) -> JsResult<i32> {
    let id = quickjs::call_c_function(
        scope,
        ns_services_set_timeout,
        &Value::undefined(),
        &[callback, Value::int(0)],
    )?;
    scope.to_int32(&id)
}

pub(crate) fn submit_or_reset(scope: &mut Scope<'_>, form: Element, submitter: Option<Element>) {
    let ctx = quickjs::raw_context(scope);
    let raw = match submitter {
        Some(submitter) => unsafe {
            ns_js_request_submit_form(ctx, form.as_ptr(), submitter.as_ptr())
        },
        None => unsafe { ns_js_reset_form(ctx, form.as_mut_ptr()) },
    };
    unsafe { quickjs::free_raw(quickjs::runtime(scope), raw) };
}

pub(crate) fn note(el: Element) -> Element {
    unsafe { ns_node_arm_js_invalidate(el.as_mut_ptr()) };
    el
}

pub(crate) fn set_attr(el: Element, name: &CStr, value: &CStr) {
    unsafe { ns_element_set_attr(el.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub(crate) fn remove_attr(el: Element, name: &CStr) {
    unsafe { ns_element_remove_attr(el.as_mut_ptr(), name.as_ptr()) };
}

pub(crate) fn effectively_disabled(el: Element) -> bool {
    unsafe { ns_element_effectively_disabled(el.as_ptr()) != 0 }
}

pub(crate) fn is_focusable(el: Element) -> bool {
    unsafe { ns_node_is_focusable(el.as_ptr()) != 0 }
}

pub(crate) fn tabindex(el: Element) -> Option<i32> {
    let mut value: c_int = 0;
    (unsafe { ns_node_tabindex(el.as_ptr(), &mut value) } != 0).then_some(value)
}

pub(crate) fn assigned_slot(n: Element) -> Option<Element> {
    node(unsafe { ns_node_assigned_slot_node(n.as_ptr()) })
}

pub(crate) fn mark_attr_dirty(el: Element, name: &CStr, old_value: Option<&CStr>) {
    unsafe {
        ns_css_mark_attr_dirty(
            el.as_mut_ptr(),
            name.as_ptr(),
            old_value.map_or(ptr::null(), CStr::as_ptr),
        )
    };
}

pub(crate) fn is_submit_trigger(el: Element) -> bool {
    unsafe { ns_node_is_submit_trigger(el.as_ptr()) != 0 }
}

unsafe fn method(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

unsafe fn getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), f) }
}

unsafe fn setter(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    let mut val = val;
    unsafe { quickjs::call_native(ctx, this_val, 1, &mut val, f) }
}

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            ctx: *mut JSContext,
            this_val: JSValue,
            argc: c_int,
            argv: *mut JSValue,
        ) -> JSValue {
            unsafe { method(ctx, this_val, argc, argv, $f) }
        }
    )*};
}

macro_rules! accessors {
    ($($get:ident, $set:ident => $g:path, $s:path),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $get(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
            unsafe { getter(ctx, this_val, $g) }
        }

        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $set(
            ctx: *mut JSContext,
            this_val: JSValue,
            val: JSValue,
        ) -> JSValue {
            unsafe { setter(ctx, this_val, val, $s) }
        }
    )*};
}

methods! {
    ns_element_showPopover => crate::popover::show_popover,
    ns_element_hidePopover => crate::popover::hide_popover,
    ns_element_togglePopover => crate::popover::toggle_popover,
    ns_element_show => crate::dialog::show,
    ns_element_showModal => crate::dialog::show_modal_method,
    ns_element_close => crate::dialog::close_method,
    ns_element_requestClose => crate::dialog::request_close_method,
}

accessors! {
    ns_element_get_popover, ns_element_set_popover
        => crate::popover::get_popover, crate::popover::set_popover,
    ns_element_get_popoverTargetElement, ns_element_set_popoverTargetElement
        => crate::invoker::get_popover_target, crate::invoker::set_popover_target,
    ns_dialog_get_returnValue, ns_dialog_set_returnValue
        => crate::dialog::get_return_value, crate::dialog::set_return_value,
    ns_dialog_get_closedBy, ns_dialog_set_closedBy
        => crate::dialog::get_closed_by, crate::dialog::set_closed_by,
    ns_button_get_command, ns_button_set_command
        => crate::invoker::get_command, crate::invoker::set_command,
    ns_button_get_commandForElement, ns_button_set_commandForElement
        => crate::invoker::get_command_for, crate::invoker::set_command_for,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_top_layer_forget_node(js: *const NsJs, n: *const NsNode) {
    if let Some(n) = node(n) {
        crate::forget_node(Js::of(js), n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_top_layer_clear(js: *const NsJs) {
    crate::clear(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_popover_removing(js: *const NsJs, el: *mut NsNode) {
    let (js, Some(el)) = (Js::of(js), node(el)) else {
        return;
    };
    if !js.is_null() && el.is_element() {
        crate::popover::removing_steps(js, el);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_popover_attr_changed(
    js: *const NsJs,
    el: *mut NsNode,
    attr: *const c_char,
    old_value: *const c_char,
    new_value: *const c_char,
) {
    let (js, Some(el), Some(attr)) = (Js::of(js), node(el), text(attr)) else {
        return;
    };
    crate::popover::attr_changed(js, el, attr.to_bytes(), text(old_value), text(new_value));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_popover_light_dismiss(
    js: *const NsJs,
    target: *const NsNode,
    up: GBoolean,
) {
    let (js, Some(target)) = (Js::of(js), node(target)) else {
        return;
    };
    if !js.is_null() {
        crate::popover::light_dismiss(js, target, up != 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dialog_light_dismiss(
    js: *const NsJs,
    target: *const NsNode,
    up: GBoolean,
) {
    let (js, Some(target)) = (Js::of(js), node(target)) else {
        return;
    };
    if !js.is_null() {
        crate::dialog::light_dismiss(js, target, up != 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_refresh_top_layer(js: *const NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::dialog::refresh_top_layer(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_process_close_request(js: *const NsJs) -> GBoolean {
    let js = Js::of(js);
    if js.is_null() || js.halted() {
        return glib::FALSE;
    }
    glib::boolean(crate::dialog::process_close_request(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dialog_close(
    js: *const NsJs,
    dialog: *mut NsNode,
    return_value: *const c_char,
) {
    let (js, Some(dialog)) = (Js::of(js), node(dialog)) else {
        return;
    };
    if !js.is_null() {
        let rv = text(return_value).map(CStr::to_owned);
        crate::dialog::close(js, dialog, rv.as_deref(), None);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_details_toggle_open(
    js: *const NsJs,
    details: *mut NsNode,
    open: GBoolean,
) {
    let (js, Some(details)) = (Js::of(js), node(details)) else {
        return;
    };
    if !js.is_null() {
        crate::details::toggle_open(js, details, open != 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_activate_summary(js: *const NsJs, el: *const NsNode) -> GBoolean {
    let js = Js::of(js);
    if js.is_null() {
        return glib::FALSE;
    }
    glib::boolean(crate::details::activate_summary(js, node(el)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_summary_toggle_target(el: *const NsNode) -> *mut NsNode {
    Node::ptr_or_null(crate::details::summary_toggle_target(node(el))).cast_mut()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_button(el: *const NsNode) -> GBoolean {
    glib::boolean(crate::invoker::is_button(node(el)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_button_activation(
    js: *const NsJs,
    button: *mut NsNode,
    event_target: *const NsNode,
) {
    let (js, Some(button)) = (Js::of(js), node(button)) else {
        return;
    };
    if !js.is_null() {
        crate::invoker::button_activation(js, button, node(event_target));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_popover_target_activation(
    js: *const NsJs,
    el: *mut NsNode,
    event_target: *const NsNode,
) {
    let (js, Some(el)) = (Js::of(js), node(el)) else {
        return;
    };
    if !js.is_null() {
        crate::invoker::popover_target_activation(js, el, node(event_target));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_flush_autofocus(js: *const NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::focus::flush_autofocus(js);
    }
}
