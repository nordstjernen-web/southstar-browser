//! Southstar — the C ABI and Rust-ABI entry points of focus, click, fullscreen and pointer lock, and the realm, event and form calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_js_engine::{Scope, Value};
use southstar_js_forms::ClickState;
use southstar_js_runtime::api;

use crate::{Element, JsResult};

use southstar_js_runtime::api::Page;
use southstar_js_runtime::events;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(Option<Page>);

use southstar_js_window::nsr_make_window_event;

impl Js {
    fn of(js: Option<Page>) -> Js {
        Js(js)
    }

    fn of_page(page: Page) -> Js {
        Js(Some(page))
    }

    fn ptr(self) -> Option<Page> {
        self.0
    }

    pub fn is_null(self) -> bool {
        self.0.is_none()
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
        if let Some(page) = self.ptr().filter(|page| page.is_live()) {
            southstar_js_mutation::commit_change(page, el);
        }
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
        events::dispatch_event(self.ptr(), target, &kind.to_string_lossy());
    }

    pub fn dispatch_built(self, target: Element, kind: &CStr, event: Value) -> bool {
        let (_, prevented) =
            events::dispatch_built_event(self.ptr(), target, &kind.to_string_lossy(), event);
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
        events::dispatch_window_only_event(self.ptr(), Some(doc), &kind.to_string_lossy(), event);
    }

    pub fn event_window_for_document(self, doc: Element) -> Value {
        self.scope(|scope| events::window_for_document(self.ptr(), scope, Some(doc)))
            .unwrap_or_else(Value::undefined)
    }

    pub fn checkable_pre_click(self, el: Element, kind: i32) -> ClickState {
        southstar_js_forms::checkable_pre_click(self.ptr(), el, kind)
    }

    pub fn activate_summary(self, el: Element) {
        (api::top_layer_hooks().activate_summary)(self.ptr(), el);
    }

    pub fn button_activation(self, button: Element, target: Element) {
        (api::top_layer_hooks().button_activation)(self.ptr(), button, Some(target));
    }

    pub fn popover_target_activation(self, el: Element, target: Element) {
        (api::top_layer_hooks().popover_target_activation)(self.ptr(), el, Some(target));
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

    pub fn checkable_post_click(self, el: Element, kind: i32, state: &ClickState, prevented: bool) {
        southstar_js_forms::checkable_post_click(self.ptr(), el, kind, state, prevented);
    }
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(Page::current(scope))
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
    events::make_event(scope, &kind.to_string_lossy(), Some(target))
}

pub(crate) fn event_flags(kind: &CStr) -> (bool, bool) {
    events::init_flags(&kind.to_string_lossy(), false)
}

fn c_string(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

pub(crate) fn activation_behavior(scope: &mut Scope<'_>, act: Element, target: Element) {
    crate::activation::activation_behavior(scope, act, target);
}

pub(crate) fn request_submit_form(scope: &mut Scope<'_>, form: Element, submitter: Element) {
    southstar_js_forms::request_submit_form(scope, form, Some(submitter));
}

pub(crate) fn reset_form(scope: &mut Scope<'_>, form: Element) {
    southstar_js_forms::reset_form(scope, form);
}

pub(crate) fn is_labelable(el: Element) -> bool {
    southstar_js_forms::is_labelable(el)
}

pub(crate) fn first_labelable_descendant(el: Element) -> Option<Element> {
    southstar_js_forms::first_labelable_descendant(el)
}

pub(crate) fn is_submit_trigger(el: Element) -> bool {
    southstar_js_forms::is_submit_trigger(el)
}

pub(crate) fn is_reset_trigger(el: Element) -> bool {
    southstar_js_forms::is_reset_trigger(el)
}

pub(crate) fn anchor_resolved_href(js: Js, el: Element) -> Option<Vec<u8>> {
    (api::dom_factory_hooks().anchor_resolved_href)(js.ptr(), el)
}

pub(crate) fn uri_unescape(text: &[u8]) -> Option<Vec<u8>> {
    let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text.iter();
    while let Some(&b) = rest.next() {
        if b != b'%' {
            out.push(b);
            continue;
        }
        let hi = char::from(*rest.next()?).to_digit(16)?;
        let lo = char::from(*rest.next()?).to_digit(16)?;
        let byte = u8::try_from(hi * 16 + lo).ok().filter(|&byte| byte != 0)?;
        out.push(byte);
    }
    Some(out)
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
    el.map_or(0, southstar_js_forms::checkable_input_kind)
}

pub(crate) fn summary_toggle_target(el: Element) -> Option<Element> {
    (api::top_layer_hooks().summary_toggle_target)(el)
}

pub(crate) fn is_button(el: Element) -> bool {
    (api::top_layer_hooks().is_button)(el)
}

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[allow(non_snake_case)]
        pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
            $f(scope, this, args)
        }
    )*};
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {$(
        #[allow(non_snake_case)]
        pub fn $name(scope: &mut Scope<'_>, this: &Value) -> JsResult {
            $f(scope, this, &[])
        }
    )*};
}

methods! {
    nsr_document_has_focus => crate::focus::has_focus,
}

getters! {}

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

pub fn note_pointer_input(js: Page, pointer: bool) {
    crate::with(Js::of_page(js), |page| page.pointer_input = pointer);
}

pub fn set_focus(js: Page, el: Option<Node<'_>>) {
    crate::focus::set_focus_in(Js::of_page(js), el.and_then(element), None);
}

pub fn set_focused_node(js: Page, el: Option<Node<'_>>) {
    crate::focus::set_focused_node(Js::of_page(js), el.and_then(element));
}

pub fn focus_from_pointer(js: Page, target: Option<Node<'_>>) {
    crate::focus::focus_from_pointer(Js::of_page(js), target.and_then(element));
}

pub fn focused_node(js: Option<Page>) -> Option<Element> {
    crate::focused(Js::of(js))
}

pub fn focused_doc(js: Option<Page>) -> Option<Element> {
    crate::peek(Js::of(js), |page| page.focused_doc)
}

pub fn clear_focused_node(js: Option<Page>) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::with(js, |page| page.focused = None);
    }
}

pub fn reset_focus(js: Option<Page>) {
    crate::reset(Js::of(js));
}

pub fn synthetic_activation(js: Option<Page>, act: Node<'_>, target: Node<'_>) {
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

pub fn sequential_focus_target(js: Page, backward: bool) -> Option<Element> {
    crate::focus::sequential_target(Js::of_page(js), backward)
}

pub fn activate_element(js: Page, el: Node<'_>) {
    let (js, Some(el)) = (Js::of_page(js), element(el)) else {
        return;
    };
    if js.has_context() {
        crate::click::click_with_activation(js, el);
    }
}

pub fn keyboard_activate(js: Page, el: Node<'_>, key: &[u8], keyup: bool) -> bool {
    let (js, Some(el)) = (Js::of_page(js), element(el)) else {
        return false;
    };
    js.has_context() && crate::click::keyboard_activate(js, el, key, keyup)
}

pub fn window_action_applied(js: Page) {
    crate::fullscreen::window_action_applied(Js::of_page(js));
}

fn update_focus_visible(js: Option<Page>) {
    let js = Js::of(js);
    if !js.is_null() {
        crate::focus::update_focus_visible(js);
    }
}

fn forget_node(js: Option<Page>, n: Node<'_>) {
    if let Some(n) = n.downgrade().get() {
        crate::forget_node(Js::of(js), n);
    }
}

fn teardown(js: Option<Page>) {
    crate::teardown(Js::of(js));
}

pub fn install_runtime_hooks() {
    api::set_focus_hooks(api::FocusHooks {
        focused_node,
        update_focus_visible,
        forget_node: |js, a1| forget_node(Some(js), a1),
        teardown: |js| teardown(Some(js)),
        forget_subtree,
    });
}

fn forget_subtree(js: Option<Page>, root: Node<'_>) {
    if let Some(root) = root.downgrade().get() {
        crate::forget_subtree(Js::of(js), root);
    }
}
