//! Southstar — the top layer's C entry points, and the js.c, DOM and binding calls it makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, JsResult};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

unsafe extern "Rust" {
    safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    unsafe fn nsr_make_event(scope: &mut Scope<'_>, kind: &str, target: *const NsNode) -> Value;
    safe fn nsr_event_define_source(scope: &mut Scope<'_>, event: &Value, source: Value);
    safe fn nsr_event_adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &str);
    safe fn nsr_services_set_timeout(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
    unsafe fn nsr_js_request_submit_form(
        scope: &mut Scope<'_>,
        form: *const NsNode,
        submitter: *const NsNode,
    );
    unsafe fn nsr_js_reset_form(scope: &mut Scope<'_>, form: *mut NsNode);
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
        api::main_context(self.ptr()).with(f)
    }

    pub fn hold(self, el: Element) -> Option<Value> {
        self.scope(|scope| wrap(scope, el))
    }

    pub fn current_document(self) -> Option<Element> {
        api::current_document(self.ptr())
    }

    pub fn current_url(self) -> CString {
        api::current_url(self.ptr())
    }

    pub fn focused(self) -> Option<Element> {
        southstar_js_focus::focused_node(self.ptr())
    }

    pub fn set_focus(self, el: Option<Element>) {
        // SAFETY: a Js value names the page's live script engine.
        unsafe { southstar_js_focus::set_focus(self.ptr().cast(), el) };
    }

    pub fn halted(self) -> bool {
        api::halted(self.ptr())
    }

    pub fn in_pump(self) -> bool {
        api::in_pump(self.ptr())
    }

    pub fn mark_mutated(self) {
        api::mark_mutated(self.ptr());
    }

    pub fn request_repaint(self) {
        api::request_repaint(self.ptr());
    }

    pub fn autofocus_processed(self) -> bool {
        api::autofocus_processed(self.ptr())
    }

    pub fn set_autofocus_processed(self) {
        api::set_autofocus_processed(self.ptr());
    }

    pub fn ready_state(self) -> i32 {
        api::ready_state(self.ptr())
    }

    pub fn set_active_modal(self, modal: Option<Element>) {
        api::set_active_modal(self.ptr(), modal);
    }

    pub fn set_attr_recorded(self, el: Element, name: &CStr, value: &[u8]) {
        southstar_js_mutation::set_attr_recorded(self.ptr(), el, name, value);
    }

    pub fn remove_attr_recorded(self, el: Element, name: &CStr) {
        southstar_js_mutation::remove_attr_recorded(self.ptr(), el, name);
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
        let old_state = old_state.to_string_lossy();
        let new_state = new_state.to_string_lossy();
        let toggle = southstar_js_dispatch::Toggle {
            old_state: Some(&old_state),
            new_state: Some(&new_state),
            cancelable,
            source,
        };
        southstar_js_dispatch::dispatch_toggle_event(
            self.ptr(),
            target,
            &kind.to_string_lossy(),
            &toggle,
        )
        .1
    }

    pub fn dispatch_built(self, target: Element, kind: &CStr, event: Value) -> bool {
        let kind = kind.to_string_lossy();
        southstar_js_dispatch::dispatch_built_event(self.ptr(), target, &kind, event).1
    }

    pub fn timer_remove(self, id: i32) {
        southstar_js_services::timer_remove(self.ptr(), id);
    }

    pub fn consume_user_activation(self) {
        api::consume_user_activation(self.ptr());
    }

    pub fn has_transient_activation(self) -> bool {
        api::has_transient_activation(self.ptr())
    }

    pub fn form_owner(self, el: Element) -> Option<Element> {
        southstar_dom::controls::form_owner(el)
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js::of(scope.context_opaque().cast())
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

pub(crate) fn wrap_or_null(scope: &mut Scope<'_>, el: Option<Element>) -> Value {
    el.map_or_else(Value::null, |el| wrap(scope, el))
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &CStr, code: i32, message: &str) -> Value {
    let message = c_string(message.as_bytes());
    nsr_dom_exception(scope, name.to_bytes(), code, message.to_bytes())
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &CStr, target: Element) -> Value {
    let kind = kind.to_string_lossy();
    unsafe { nsr_make_event(scope, &kind, target.as_ptr()) }
}

pub(crate) fn define_event_source(scope: &mut Scope<'_>, event: &Value, source: Value) {
    nsr_event_define_source(scope, event, source);
}

pub(crate) fn adopt_interface(scope: &mut Scope<'_>, event: &Value, iface: &CStr) {
    nsr_event_adopt_interface(scope, event, &iface.to_string_lossy());
}

pub(crate) fn set_timeout(scope: &mut Scope<'_>, callback: Value) -> JsResult<i32> {
    let id = nsr_services_set_timeout(scope, &Value::undefined(), &[callback, Value::int(0)])?;
    scope.to_int32(&id)
}

pub(crate) fn submit_or_reset(scope: &mut Scope<'_>, form: Element, submitter: Option<Element>) {
    match submitter {
        Some(submitter) => unsafe {
            nsr_js_request_submit_form(scope, form.as_ptr(), submitter.as_ptr())
        },
        None => unsafe { nsr_js_reset_form(scope, form.as_mut_ptr()) },
    }
}

pub(crate) fn note(el: Element) -> Element {
    api::arm_js_invalidate(el);
    el
}

pub(crate) fn set_attr(el: Element, name: &CStr, value: &CStr) {
    southstar_dom::attrs::set_attribute(el, name, Some(value.to_bytes()));
}

pub(crate) fn remove_attr(el: Element, name: &CStr) {
    southstar_dom::attrs::remove_attribute(el, name);
}

pub(crate) fn effectively_disabled(el: Element) -> bool {
    southstar_dom::controls::effectively_disabled(el)
}

pub(crate) fn is_focusable(el: Element) -> bool {
    southstar_js_focus::is_focusable(el)
}

pub(crate) fn tabindex(el: Element) -> Option<i32> {
    southstar_js_focus::tabindex(el)
}

pub(crate) fn assigned_slot(n: Element) -> Option<Element> {
    southstar_js_shadow::assigned_slot_node(n)
}

pub(crate) fn mark_attr_dirty(el: Element, name: &CStr, old_value: Option<&CStr>) {
    southstar_css::mark_attr_dirty(el, Some(name.to_bytes()), old_value.map(CStr::to_bytes));
}

pub(crate) fn is_submit_trigger(el: Element) -> bool {
    southstar_js_forms::is_submit_trigger(el)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_top_layer_forget_node(js: *const NsJs, n: *const NsNode) {
    if let Some(n) = node(n) {
        crate::forget_node(Js::of(js), n);
    }
}

fn live(n: Node<'_>) -> Option<Element> {
    n.downgrade().get()
}

fn page(js: *const NsJs) -> Option<Js> {
    let js = Js::of(js);
    (!js.is_null()).then_some(js)
}

pub fn clear_top_layer(js: *const NsJs) {
    crate::clear(Js::of(js));
}

pub fn popover_light_dismiss(js: *const NsJs, target: Node<'_>, up: bool) {
    if let (Some(js), Some(target)) = (page(js), live(target)) {
        crate::popover::light_dismiss(js, target, up);
    }
}

pub fn dialog_light_dismiss(js: *const NsJs, target: Node<'_>, up: bool) {
    if let (Some(js), Some(target)) = (page(js), live(target)) {
        crate::dialog::light_dismiss(js, target, up);
    }
}

pub fn process_close_request(js: *const NsJs) -> bool {
    page(js).is_some_and(|js| !js.halted() && crate::dialog::process_close_request(js))
}

pub fn details_toggle_open(js: *const NsJs, details: Node<'_>, open: bool) {
    if let (Some(js), Some(details)) = (page(js), live(details)) {
        crate::details::toggle_open(js, details, open);
    }
}

pub fn activate_summary(js: *const NsJs, el: Node<'_>) -> bool {
    page(js).is_some_and(|js| crate::details::activate_summary(js, live(el)))
}

pub fn flush_autofocus(js: *const NsJs) {
    if let Some(js) = page(js) {
        crate::focus::flush_autofocus(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_top_layer_clear(js: *const NsJs) {
    clear_top_layer(js);
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
