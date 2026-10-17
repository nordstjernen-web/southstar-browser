//! Southstar — the C ABI and Rust-ABI entry points of focus, click, fullscreen and pointer lock, and the realm, event and form calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, JsResult};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

#[repr(C)]
pub(crate) struct CheckableState {
    checked: GBoolean,
    indeterminate: GBoolean,
    checked_radio: *mut NsNode,
}

unsafe extern "C" {
    fn ns_event_type_init_flags(
        kind: *const c_char,
        at_document: GBoolean,
        bubbles: *mut GBoolean,
        cancelable: *mut GBoolean,
    );
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_node_is_labelable(n: *const NsNode) -> GBoolean;
    fn ns_js_first_labelable_descendant(n: *const NsNode, depth: c_int) -> *const NsNode;
    fn ns_node_is_submit_trigger(el: *const NsNode) -> GBoolean;
    fn ns_node_is_reset_trigger(el: *const NsNode) -> GBoolean;
    fn ns_js_activate_summary(js: *const NsJs, el: *const NsNode) -> GBoolean;
    fn ns_button_activation(js: *mut NsJs, button: *mut NsNode, event_target: *const NsNode);
    fn ns_popover_target_activation(js: *mut NsJs, el: *mut NsNode, event_target: *const NsNode);
    fn ns_element_anchor_resolved_href(n: *const NsNode, js: *mut NsJs) -> *mut c_char;
    fn g_uri_unescape_string(escaped: *const c_char, illegal: *const c_char) -> *mut c_char;
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
    fn ns_summary_toggle_target(el: *const NsNode) -> *mut NsNode;
    fn ns_node_is_button(el: *const NsNode) -> GBoolean;
}

unsafe extern "Rust" {
    unsafe fn nsr_make_event(scope: &mut Scope<'_>, kind: &str, target: *const NsNode) -> Value;
    safe fn nsr_make_window_event(scope: &mut Scope<'_>, kind: &str) -> Value;
    unsafe fn nsr_js_dispatch_built_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: &str,
        event: Value,
    ) -> (bool, bool);
    unsafe fn nsr_js_dispatch_window_only_event(
        js: *mut NsJs,
        target_doc: *const NsNode,
        kind: &str,
        event: Value,
    ) -> bool;
    unsafe fn nsr_js_event_window_for_document(
        js: *mut NsJs,
        scope: &mut Scope<'_>,
        doc: *const NsNode,
    ) -> Value;
    unsafe fn nsr_js_request_submit_form(
        scope: &mut Scope<'_>,
        form: *const NsNode,
        submitter: *const NsNode,
    );
    unsafe fn nsr_js_reset_form(scope: &mut Scope<'_>, form: *mut NsNode);
}

pub(crate) fn node(ptr: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(ptr) }
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

    pub fn has_context(self) -> bool {
        !api::main_context(self.ptr()).is_null()
    }

    pub fn current_document(self) -> Option<Element> {
        api::current_document(self.ptr())
    }

    pub fn active_modal(self) -> Option<Element> {
        api::active_modal(self.ptr())
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

    pub fn commit_change(self, el: Element) {
        southstar_js_mutation::commit_change(self.ptr(), el);
    }

    pub fn log_line(self, line: &CStr) {
        api::log_line(self.ptr(), line);
    }

    pub fn has_transient_activation(self) -> bool {
        api::has_transient_activation(self.ptr())
    }

    pub fn has_window_action(self) -> bool {
        api::has_window_action(self.ptr())
    }

    pub fn window_action(self, action: &CStr) {
        api::window_action(self.ptr(), action);
    }

    pub fn dispatch(self, target: Element, kind: &CStr) {
        unsafe {
            ns_js_dispatch_event(self.ptr(), target.as_ptr(), kind.as_ptr(), ptr::null_mut())
        };
    }

    pub fn dispatch_built(self, target: Element, kind: &CStr, event: Value) -> bool {
        let (_, prevented) = unsafe {
            nsr_js_dispatch_built_event(self.ptr(), target.as_ptr(), &kind.to_string_lossy(), event)
        };
        prevented
    }

    pub fn in_document_realm(self, doc: Element, f: impl FnOnce()) {
        let realm = match doc.parent() {
            Some(_) => southstar_js_frames::node_realm_context(self.ptr(), doc),
            None => {
                let main = api::main_realm(self.ptr());
                if main.is_null() {
                    api::main_context(self.ptr())
                } else {
                    main
                }
            }
        };
        if realm.is_null() {
            return;
        }
        match doc.parent() {
            Some(frame) if realm != api::main_context(self.ptr()) => {
                api::in_frame_scope(self.ptr(), realm, frame, f);
            }
            Some(_) => f(),
            None => api::in_realm(self.ptr(), realm, f),
        }
    }

    pub fn dispatch_window_only(self, doc: Element, kind: &CStr, event: Value) {
        unsafe {
            nsr_js_dispatch_window_only_event(
                self.ptr(),
                doc.as_ptr(),
                &kind.to_string_lossy(),
                event,
            )
        };
    }

    pub fn event_window_for_document(self, doc: Element) -> Value {
        self.scope(|scope| unsafe {
            nsr_js_event_window_for_document(self.ptr(), scope, doc.as_ptr())
        })
        .unwrap_or_else(Value::undefined)
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

    pub fn activate_summary(self, el: Element) {
        unsafe { ns_js_activate_summary(self.ptr(), el.as_ptr()) };
    }

    pub fn button_activation(self, button: Element, target: Element) {
        unsafe { ns_button_activation(self.ptr(), button.as_mut_ptr(), target.as_ptr()) };
    }

    pub fn popover_target_activation(self, el: Element, target: Element) {
        unsafe { ns_popover_target_activation(self.ptr(), el.as_mut_ptr(), target.as_ptr()) };
    }

    pub fn form_owner_for(self, el: Element) -> Option<Element> {
        southstar_dom::controls::form_owner(el)
    }

    pub fn eval_source(self, code: &[u8], origin: &CStr) {
        let _ = southstar_js_loader::eval_source(self.ptr(), &c_string(code), Some(origin));
    }

    pub fn request_download(self, url: &[u8], filename: &CStr) -> bool {
        api::request_download(self.ptr(), &c_string(url), Some(filename))
    }

    pub fn anchor_fragment_navigate(self, url: &[u8]) -> bool {
        southstar_js_window::anchor_fragment_navigate(self.ptr(), url)
    }

    pub fn navigate(self, url: &[u8]) {
        api::navigate(self.ptr(), &c_string(url), false);
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
    api::make_element(scope, el.as_ptr()).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn wrap_or_null(scope: &mut Scope<'_>, el: Option<Element>) -> Value {
    el.map_or_else(Value::null, |el| wrap(scope, el))
}

pub(crate) fn make_window_event(scope: &mut Scope<'_>, kind: &CStr) -> Value {
    nsr_make_window_event(scope, &kind.to_string_lossy())
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &CStr, target: Element) -> Value {
    unsafe { nsr_make_event(scope, &kind.to_string_lossy(), target.as_ptr()) }
}

pub(crate) fn event_flags(kind: &CStr) -> (bool, bool) {
    let (mut bubbles, mut cancelable): (GBoolean, GBoolean) = (0, 0);
    unsafe { ns_event_type_init_flags(kind.as_ptr(), glib::FALSE, &mut bubbles, &mut cancelable) };
    (bubbles != 0, cancelable != 0)
}

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn owned_text(text: *mut c_char) -> Option<Vec<u8>> {
    if text.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
    unsafe { glib::g_free(text.cast()) };
    Some(bytes)
}

pub(crate) fn activation_behavior(scope: &mut Scope<'_>, act: Element, target: Element) {
    crate::activation::activation_behavior(scope, act, target);
}

pub(crate) fn request_submit_form(scope: &mut Scope<'_>, form: Element, submitter: Element) {
    unsafe { nsr_js_request_submit_form(scope, form.as_ptr(), submitter.as_ptr()) };
}

pub(crate) fn reset_form(scope: &mut Scope<'_>, form: Element) {
    unsafe { nsr_js_reset_form(scope, form.as_mut_ptr()) };
}

pub(crate) fn is_labelable(el: Element) -> bool {
    unsafe { ns_js_node_is_labelable(el.as_ptr()) != 0 }
}

pub(crate) fn first_labelable_descendant(el: Element) -> Option<Element> {
    node(unsafe { ns_js_first_labelable_descendant(el.as_ptr(), 0) })
}

pub(crate) fn is_submit_trigger(el: Element) -> bool {
    unsafe { ns_node_is_submit_trigger(el.as_ptr()) != 0 }
}

pub(crate) fn is_reset_trigger(el: Element) -> bool {
    unsafe { ns_node_is_reset_trigger(el.as_ptr()) != 0 }
}

pub(crate) fn anchor_resolved_href(js: Js, el: Element) -> Option<Vec<u8>> {
    owned_text(unsafe { ns_element_anchor_resolved_href(el.as_ptr(), js.ptr()) })
}

pub(crate) fn uri_unescape(text: &[u8]) -> Option<Vec<u8>> {
    let text = c_string(text);
    owned_text(unsafe { g_uri_unescape_string(text.as_ptr(), ptr::null()) })
}

pub(crate) fn note(el: Element) {
    api::arm_js_invalidate(el);
}

pub(crate) fn set_focus_visible(el: Option<Element>) {
    southstar_css::set_focus_visible_node(el);
}

pub(crate) fn set_fullscreen_node(el: Option<Element>) {
    southstar_css::set_fullscreen_node(el);
}

pub(crate) fn is_disabled_form_control(el: Element) -> bool {
    api::is_disabled_form_control(el)
}

pub(crate) fn checkable_kind(el: Option<Element>) -> i32 {
    el.map_or(0, |el| unsafe { ns_checkable_input_kind(el.as_ptr()) })
}

pub(crate) fn summary_toggle_target(el: Element) -> Option<Element> {
    node(unsafe { ns_summary_toggle_target(el.as_ptr()) })
}

pub(crate) fn is_button(el: Element) -> bool {
    unsafe { ns_node_is_button(el.as_ptr()) != 0 }
}

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[allow(non_snake_case)]
        #[unsafe(no_mangle)]
        pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
            $f(scope, this, args)
        }
    )*};
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[allow(non_snake_case)]
        #[unsafe(no_mangle)]
        pub fn $name(scope: &mut Scope<'_>, this: &Value) -> JsResult {
            $f(scope, this, &[])
        }
    )*};
}

methods! {
    nsr_element_focus => crate::focus::focus_method,
    nsr_element_blur => crate::focus::blur_method,
    nsr_document_has_focus => crate::focus::has_focus,
    nsr_element_click => crate::click::click_method,
    nsr_element_request_fullscreen => crate::fullscreen::request_fullscreen,
    nsr_document_exit_fullscreen => crate::fullscreen::exit_fullscreen,
    nsr_element_requestPointerLock => crate::fullscreen::request_pointer_lock,
    nsr_document_exitPointerLock => crate::fullscreen::exit_pointer_lock,
}

getters! {
    nsr_document_get_fullscreen_element => crate::fullscreen::get_fullscreen_element,
    nsr_document_get_fullscreen_enabled => crate::fullscreen::get_fullscreen_enabled,
    nsr_document_get_is_fullscreen => crate::fullscreen::get_is_fullscreen,
    nsr_document_get_pointerLockElement => crate::fullscreen::get_pointer_lock_element,
}

fn element(n: Node<'_>) -> Option<Element> {
    n.downgrade().get()
}

pub fn tabindex(el: Node<'_>) -> Option<c_int> {
    element(el).and_then(crate::focus::tabindex)
}

pub fn is_focusable(el: Node<'_>) -> bool {
    element(el).is_some_and(crate::focus::is_focusable)
}

pub fn keyboard_activates(el: Node<'_>, key: &[u8]) -> bool {
    element(el).is_some_and(|el| crate::click::keyboard_activates(el, key))
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn note_pointer_input(js: *mut c_void, pointer: bool) {
    let js = Js::of(js.cast());
    if !js.is_null() {
        crate::with(js, |page| page.pointer_input = pointer);
    }
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn set_focus(js: *mut c_void, el: Option<Node<'_>>) {
    let js = Js::of(js.cast());
    if !js.is_null() {
        crate::focus::set_focus_in(js, el.and_then(element), None);
    }
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn set_focused_node(js: *mut c_void, el: Option<Node<'_>>) {
    let js = Js::of(js.cast());
    if !js.is_null() {
        crate::focus::set_focused_node(js, el.and_then(element));
    }
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn focus_from_pointer(js: *mut c_void, target: Option<Node<'_>>) {
    let js = Js::of(js.cast());
    if !js.is_null() {
        crate::focus::focus_from_pointer(js, target.and_then(element));
    }
}

pub fn focused_node(js: *const NsJs) -> Option<Element> {
    crate::focused(Js::of(js))
}

pub fn focused_doc(js: *const NsJs) -> Option<Element> {
    crate::peek(Js::of(js), |page| page.focused_doc)
}

pub fn clear_focused_node(js: *const NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::with(js, |page| page.focused = None);
    }
}

pub fn reset_focus(js: *const NsJs) {
    crate::reset(Js::of(js));
}

pub fn synthetic_activation(js: *const NsJs, act: Node<'_>, target: Node<'_>) {
    let (js, Some(act), Some(target)) = (Js::of(js), element(act), element(target)) else {
        return;
    };
    if js.has_context() {
        crate::click::synthetic_activation(js, act, target);
    }
}

pub fn has_activation_behavior(el: Node<'_>) -> bool {
    element(el).is_some_and(crate::activation::has_activation_behavior)
}

pub fn click_activation_target(el: Node<'_>) -> Option<Element> {
    element(el).and_then(crate::activation::click_activation_target)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn sequential_focus_target(js: *mut c_void, backward: bool) -> Option<Element> {
    let js = Js::of(js.cast());
    if js.is_null() {
        return None;
    }
    crate::focus::sequential_target(js, backward)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn activate_element(js: *mut c_void, el: Node<'_>) {
    let (js, Some(el)) = (Js::of(js.cast()), element(el)) else {
        return;
    };
    if js.has_context() {
        crate::click::click_with_activation(js, el);
    }
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn keyboard_activate(js: *mut c_void, el: Node<'_>, key: &[u8], keyup: bool) -> bool {
    let (js, Some(el)) = (Js::of(js.cast()), element(el)) else {
        return false;
    };
    js.has_context() && crate::click::keyboard_activate(js, el, key, keyup)
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn window_action_applied(js: *mut c_void) {
    let js = Js::of(js.cast());
    if !js.is_null() {
        crate::fullscreen::window_action_applied(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_update_focus_visible(js: *const NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::focus::update_focus_visible(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_focused_node(js: *const NsJs) -> *const NsNode {
    Node::ptr_or_null(focused_node(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_focus_forget_node(js: *const NsJs, n: *const NsNode) {
    if let Some(n) = node(n) {
        crate::forget_node(Js::of(js), n);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_focus_forget_subtree(js: *const NsJs, root: *const NsNode) {
    if let Some(root) = node(root) {
        crate::forget_subtree(Js::of(js), root);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_focus_teardown(js: *const NsJs) {
    crate::teardown(Js::of(js));
}
