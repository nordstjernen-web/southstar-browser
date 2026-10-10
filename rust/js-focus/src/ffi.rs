//! Southstar — the C ABI of focus, click, fullscreen and pointer lock as declared in src/js.h and src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

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

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_make_event(ctx: *mut JSContext, kind: *const c_char, target: *const NsNode) -> JSValue;
    fn ns_event_type_init_flags(
        kind: *const c_char,
        at_document: GBoolean,
        bubbles: *mut GBoolean,
        cancelable: *mut GBoolean,
    );
    fn ns_js_dispatch_built_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        event: JSValue,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_fire_window_focus_event(js: *mut NsJs, doc: *mut NsNode, kind: *const c_char);
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_active_modal(js: *const NsJs) -> *const NsNode;
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_in_pump(js: *const NsJs) -> GBoolean;
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_commit_change(js: *mut NsJs, el: *const NsNode);
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_js_has_transient_activation(js: *mut NsJs) -> GBoolean;
    fn ns_js_has_window_action(js: *const NsJs) -> GBoolean;
    fn ns_js_window_action(js: *mut NsJs, action: *const c_char);
    fn ns_node_arm_js_invalidate(n: *mut NsNode);
    fn ns_css_set_focus_visible_node(node: *const NsNode);
    fn ns_css_set_fullscreen_node(node: *const NsNode) -> *const NsNode;
    fn ns_click_activation_target(el: *const NsNode) -> *const NsNode;
    fn ns_element_activation_behavior(
        ctx: *mut JSContext,
        act: *const NsNode,
        target: *const NsNode,
    ) -> JSValue;
    fn ns_node_is_disabled_form_control(el: *const NsNode) -> GBoolean;
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
        if self.is_null() {
            return None;
        }
        let ctx = unsafe { ns_js_main_context(self.ptr()) };
        if ctx.is_null() {
            return None;
        }
        Some(unsafe { quickjs::with_context(ctx, f) })
    }

    pub fn has_context(self) -> bool {
        !self.is_null() && !unsafe { ns_js_main_context(self.ptr()) }.is_null()
    }

    pub fn current_document(self) -> Option<Element> {
        node(unsafe { ns_js_current_document(self.ptr()) })
    }

    pub fn active_modal(self) -> Option<Element> {
        node(unsafe { ns_js_active_modal(self.ptr()) })
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

    pub fn commit_change(self, el: Element) {
        unsafe { ns_js_commit_change(self.ptr(), el.as_ptr()) };
    }

    pub fn log_line(self, line: &CStr) {
        unsafe { ns_js_log_line(self.ptr(), line.as_ptr()) };
    }

    pub fn has_transient_activation(self) -> bool {
        unsafe { ns_js_has_transient_activation(self.ptr()) != 0 }
    }

    pub fn has_window_action(self) -> bool {
        unsafe { ns_js_has_window_action(self.ptr()) != 0 }
    }

    pub fn window_action(self, action: &CStr) {
        unsafe { ns_js_window_action(self.ptr(), action.as_ptr()) };
    }

    pub fn dispatch(self, target: Element, kind: &CStr) {
        unsafe {
            ns_js_dispatch_event(self.ptr(), target.as_ptr(), kind.as_ptr(), ptr::null_mut())
        };
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

    pub fn fire_window_focus_event(self, doc: Element, kind: &CStr) {
        unsafe { ns_js_fire_window_focus_event(self.ptr(), doc.as_mut_ptr(), kind.as_ptr()) };
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

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &CStr, target: Element) -> Value {
    let raw = unsafe { ns_make_event(quickjs::raw_context(scope), kind.as_ptr(), target.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn event_flags(kind: &CStr) -> (bool, bool) {
    let (mut bubbles, mut cancelable): (GBoolean, GBoolean) = (0, 0);
    unsafe { ns_event_type_init_flags(kind.as_ptr(), glib::FALSE, &mut bubbles, &mut cancelable) };
    (bubbles != 0, cancelable != 0)
}

pub(crate) fn activation_behavior(scope: &mut Scope<'_>, act: Element, target: Element) {
    let raw = unsafe {
        ns_element_activation_behavior(quickjs::raw_context(scope), act.as_ptr(), target.as_ptr())
    };
    unsafe { quickjs::free_raw(quickjs::runtime(scope), raw) };
}

pub(crate) fn note(el: Element) {
    unsafe { ns_node_arm_js_invalidate(el.as_mut_ptr()) };
}

pub(crate) fn set_focus_visible(el: Option<Element>) {
    unsafe { ns_css_set_focus_visible_node(Node::ptr_or_null(el)) };
}

pub(crate) fn set_fullscreen_node(el: Option<Element>) {
    unsafe { ns_css_set_fullscreen_node(Node::ptr_or_null(el)) };
}

pub(crate) fn click_activation_target(el: Element) -> Option<Element> {
    node(unsafe { ns_click_activation_target(el.as_ptr()) })
}

pub(crate) fn is_disabled_form_control(el: Element) -> bool {
    unsafe { ns_node_is_disabled_form_control(el.as_ptr()) != 0 }
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

unsafe fn method(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
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

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
            unsafe { method(ctx, this_val, 0, ptr::null_mut(), $f) }
        }
    )*};
}

methods! {
    ns_element_focus => crate::focus::focus_method,
    ns_element_blur => crate::focus::blur_method,
    ns_document_has_focus => crate::focus::has_focus,
    ns_element_click => crate::click::click_method,
    ns_element_request_fullscreen => crate::fullscreen::request_fullscreen,
    ns_document_exit_fullscreen => crate::fullscreen::exit_fullscreen,
    ns_element_requestPointerLock => crate::fullscreen::request_pointer_lock,
    ns_document_exitPointerLock => crate::fullscreen::exit_pointer_lock,
}

getters! {
    ns_document_get_fullscreen_element => crate::fullscreen::get_fullscreen_element,
    ns_document_get_fullscreen_enabled => crate::fullscreen::get_fullscreen_enabled,
    ns_document_get_is_fullscreen => crate::fullscreen::get_is_fullscreen,
    ns_document_get_pointerLockElement => crate::fullscreen::get_pointer_lock_element,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_tabindex(el: *const NsNode, out: *mut c_int) -> GBoolean {
    let Some(value) = node(el).and_then(crate::focus::tabindex) else {
        return glib::FALSE;
    };
    if !out.is_null() {
        unsafe { *out = value };
    }
    glib::TRUE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_focusable(el: *const NsNode) -> GBoolean {
    glib::boolean(node(el).is_some_and(crate::focus::is_focusable))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_note_pointer_input(js: *const NsJs, pointer: GBoolean) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::with(js, |page| page.pointer_input = pointer != 0);
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
pub unsafe extern "C" fn ns_js_set_focus(js: *const NsJs, el: *const NsNode) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::focus::set_focus_in(js, node(el), None);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_set_focused_node(js: *const NsJs, el: *const NsNode) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::focus::set_focused_node(js, node(el));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_focus_from_pointer(js: *const NsJs, target: *const NsNode) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::focus::focus_from_pointer(js, node(target));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_focused_node(js: *const NsJs) -> *const NsNode {
    Node::ptr_or_null(crate::focused(Js::of(js)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_focused_doc(js: *const NsJs) -> *const NsNode {
    Node::ptr_or_null(crate::peek(Js::of(js), |page| page.focused_doc))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_clear_focused_node(js: *const NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::with(js, |page| page.focused = None);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_sequential_focus_target(
    js: *const NsJs,
    backward: GBoolean,
) -> *const NsNode {
    let js = Js::of(js);
    if js.is_null() {
        return ptr::null();
    }
    Node::ptr_or_null(crate::focus::sequential_target(js, backward != 0))
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
pub unsafe extern "C" fn ns_focus_reset(js: *const NsJs) {
    crate::reset(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_focus_teardown(js: *const NsJs) {
    crate::teardown(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_activate_element(js: *const NsJs, el: *const NsNode) {
    let (js, Some(el)) = (Js::of(js), node(el)) else {
        return;
    };
    if js.has_context() {
        crate::click::click_with_activation(js, el);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_synthetic_activation(
    js: *const NsJs,
    act: *const NsNode,
    target: *const NsNode,
) {
    let (js, Some(act), Some(target)) = (Js::of(js), node(act), node(target)) else {
        return;
    };
    if js.has_context() {
        crate::click::synthetic_activation(js, act, target);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_keyboard_activate(
    js: *const NsJs,
    el: *const NsNode,
    key: *const c_char,
    keyup: GBoolean,
) -> GBoolean {
    let (js, Some(el)) = (Js::of(js), node(el)) else {
        return glib::FALSE;
    };
    if key.is_null() || !js.has_context() {
        return glib::FALSE;
    }
    let key = unsafe { CStr::from_ptr(key) };
    glib::boolean(crate::click::keyboard_activate(
        js,
        el,
        key.to_bytes(),
        keyup != 0,
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_keyboard_activates(
    el: *const NsNode,
    key: *const c_char,
) -> GBoolean {
    let Some(el) = node(el) else {
        return glib::FALSE;
    };
    if key.is_null() {
        return glib::FALSE;
    }
    let key = unsafe { CStr::from_ptr(key) };
    glib::boolean(crate::click::keyboard_activates(el, key.to_bytes()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_window_action_applied(js: *const NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::fullscreen::window_action_applied(js);
    }
}
