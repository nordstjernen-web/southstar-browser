//! Southstar — the frame bindings' exports (C for pointer-only calls, Rust ABI for natives) and the engine, DOM and layout calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) mod ctx;
pub(crate) mod form;
mod imports;
pub(crate) mod load;

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_js_bind::bind_native;
use southstar_js_engine::{ContextHandle, NativeFn, Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;
use southstar_layout::BoxRef;

use self::imports::*;
use crate::page::{self, Frame};
use crate::{cloner, globals, origin, owner, queue, realm};
use ctx::{Ctx, PropertyKey};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(Option<Page>);

impl Js {
    pub(crate) fn of(js: Option<Page>) -> Js {
        Js(js)
    }

    pub fn of_ctx(ctx: Ctx) -> Js {
        Js(ctx.page())
    }

    pub fn is_null(self) -> bool {
        self.0.is_none()
    }

    fn ptr(self) -> Option<Page> {
        self.0
    }
}

type JsResult = Result<Value, Value>;

#[derive(Clone, Copy)]
pub(crate) enum Binding {
    CustomElementsDefine,
    CustomElementsGet,
    CustomElementsUpgrade,
    CustomElementsWhenDefined,
    CustomElementsGetName,
    WindowAddEventListener,
    WindowRemoveEventListener,
    WindowDispatchEvent,
    ElementAddEventListener,
    ElementRemoveEventListener,
    ElementDispatchEvent,
    SetTimeout,
    SetInterval,
    ClearTimeout,
    ClearInterval,
    RequestAnimationFrame,
    CancelAnimationFrame,
    QueueMicrotask,
}

impl Binding {
    fn native(self) -> (&'static str, NativeFn, u32) {
        match self {
            Binding::CustomElementsDefine => {
                ("define", southstar_js_custom_elements::define_native, 3)
            }
            Binding::CustomElementsGet => ("get", southstar_js_custom_elements::get_native, 1),
            Binding::CustomElementsUpgrade => {
                ("upgrade", southstar_js_custom_elements::upgrade_native, 1)
            }
            Binding::CustomElementsWhenDefined => (
                "whenDefined",
                southstar_js_custom_elements::when_defined_native,
                1,
            ),
            Binding::CustomElementsGetName => {
                ("getName", southstar_js_custom_elements::get_name_native, 1)
            }
            Binding::WindowAddEventListener => ("addEventListener", nsr_window_addEventListener, 2),
            Binding::WindowRemoveEventListener => {
                ("removeEventListener", nsr_window_removeEventListener, 2)
            }
            Binding::WindowDispatchEvent => ("dispatchEvent", nsr_window_dispatchEvent, 1),
            Binding::ElementAddEventListener => {
                ("addEventListener", nsr_element_addEventListener, 2)
            }
            Binding::ElementRemoveEventListener => {
                ("removeEventListener", nsr_element_removeEventListener, 2)
            }
            Binding::ElementDispatchEvent => ("dispatchEvent", nsr_element_dispatchEvent, 1),
            Binding::SetTimeout => ("setTimeout", nsr_services_set_timeout, 2),
            Binding::SetInterval => ("setInterval", nsr_services_set_interval, 2),
            Binding::ClearTimeout => ("clearTimeout", nsr_services_clear_timer, 1),
            Binding::ClearInterval => ("clearInterval", nsr_services_clear_timer, 1),
            Binding::RequestAnimationFrame => {
                ("requestAnimationFrame", nsr_window_requestAnimationFrame, 1)
            }
            Binding::CancelAnimationFrame => {
                ("cancelAnimationFrame", nsr_window_cancelAnimationFrame, 1)
            }
            Binding::QueueMicrotask => ("queueMicrotask", nsr_services_queue_microtask, 1),
        }
    }
}

pub(crate) fn bind(ctx: Ctx, object: &Value, binding: Binding) {
    let (name, function, arity) = binding.native();
    ctx.try_enter(|scope| bind_native(scope, object, name, arity, function));
}

pub(crate) fn bind_post_message(ctx: Ctx, global: &Value) {
    ctx.try_enter(|scope| nsr_window_bind_post_message(scope, global));
}

pub(crate) fn main_realm(js: Js) -> Ctx {
    let main = api::main_realm(js.ptr());
    Ctx::from_handle(main).unwrap_or_else(|| current_realm(js))
}

pub(crate) fn current_realm(js: Js) -> Ctx {
    Ctx::from_handle(api::main_context(js.ptr())).unwrap_or(Ctx::NULL)
}

pub(crate) fn navigator_brand(js: Js, _ctx: Ctx) -> Value {
    api::navigator_brand(js.ptr())
}

pub(crate) fn pristine_promise(js: Js, _ctx: Ctx) -> Value {
    api::pristine_promise(js.ptr())
}

pub(crate) fn is_window_named(value: &Value) -> bool {
    nsr_is_window_named(value)
}

pub(crate) fn new_window_named(ctx: Ctx) -> Option<Value> {
    ctx.try_enter(|scope| nsr_js_new_window_named(scope).ok())
        .flatten()
}

pub(crate) fn storage_area(value: &Value) -> Option<i32> {
    nsr_storage_area_of(value)
}

pub(crate) fn new_storage(ctx: Ctx, area: i32) -> Option<Value> {
    ctx.try_enter(|scope| nsr_storage_new(scope, area).ok())
        .flatten()
}

pub(crate) fn debug_realm() -> bool {
    std::env::var_os("NS_DBG_REALM").is_some()
}

pub(crate) fn new_frame_context(js: Js) -> Option<Ctx> {
    Ctx::from_handle(api::new_frame_context(js.ptr()))
}

pub(crate) fn move_timeline(js: Js, frame: Node<'_>, ctx: Ctx) {
    southstar_js_perf::move_frame_timeline(js.ptr(), frame, ctx.handle());
}

pub(crate) fn adopt_frame_clock(js: Js, frame: Option<Node<'_>>, ctx: Ctx) {
    southstar_js_perf::adopt_frame_clock(js.ptr(), frame, ctx.handle());
}

pub(crate) fn make_element(ctx: Ctx, node: Node<'_>) -> Value {
    ctx.try_enter(|scope| api::make_element(scope, node.as_ptr()).ok())
        .flatten()
        .unwrap_or_else(Value::undefined)
}

pub(crate) fn make_post_message(ctx: Ctx, window: &Value) -> Value {
    ctx.try_enter(|scope| nsr_window_make_post_message(scope, window))
        .unwrap_or_else(Value::undefined)
}

pub(crate) fn finish_frame_realm(js: Js, ctx: Ctx, window: &Value, parent_global: &Value) {
    api::frame_realm_finish(js.ptr(), ctx.handle(), window, parent_global);
}

pub(crate) fn node_doc_base(js: Js, node: Node<'_>) -> Option<Vec<u8>> {
    api::node_doc_base(js.ptr(), Some(node)).map(CString::into_bytes)
}

pub(crate) fn run_script_element(js: Js, node: Node<'_>, origin: Option<&[u8]>) {
    let origin = origin.map(|origin| CString::new(origin).unwrap_or_default());
    southstar_js_loader::run_script_element(js.ptr(), node, origin.as_deref());
}

pub(crate) fn frame_node(frame: Frame) -> Option<Node<'static>> {
    frame.0.get()
}

pub(crate) fn scope_ctx(scope: &Scope<'_>) -> Ctx {
    Ctx::from_handle(scope.handle()).unwrap_or(Ctx::NULL)
}

pub(crate) fn unwrap_element(value: &Value) -> Option<Node<'static>> {
    api::unwrap_element(value)
}

pub(crate) fn ce_main_doc(js: Js) -> Option<Node<'static>> {
    api::ce_main_doc(js.ptr())
}

pub(crate) fn current_document(js: Js) -> Option<Node<'static>> {
    api::current_document(js.ptr())
}

pub(crate) fn is_cross_origin(js: Js, iframe: Node<'_>) -> bool {
    origin::is_cross_origin(js, iframe)
}

pub(crate) fn set_doc_ready_state(js: Js, doc: Node<'_>, state: c_int) {
    api::set_doc_ready_state(js.ptr(), doc, state);
}

pub(crate) fn make_realm_document(
    scope: &mut Scope<'_>,
    doc: Node<'_>,
    url: Option<&CStr>,
    charset: Option<&CStr>,
    content_type: Option<&CStr>,
    is_xml: bool,
) -> Result<Value, Value> {
    Ok((api::document_hooks().make_realm_document)(
        scope,
        doc,
        url.map(CStr::to_bytes),
        charset.map(CStr::to_bytes),
        content_type.map(CStr::to_bytes),
        is_xml,
        false,
    ))
}

pub(crate) fn platform_global_names(js: Js) -> Vec<String> {
    api::platform_global_names(js.ptr())
        .iter()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

pub(crate) fn window_event_functions(scope: &mut Scope<'_>) -> [(&'static str, Value); 3] {
    [
        (
            "add",
            scope.function("addEventListener", 2, nsr_window_addEventListener),
        ),
        (
            "remove",
            scope.function("removeEventListener", 2, nsr_window_removeEventListener),
        ),
        (
            "dispatch",
            scope.function("dispatchEvent", 1, nsr_window_dispatchEvent),
        ),
    ]
}

pub(crate) fn window_child_frame(
    doc: Option<Node<'_>>,
    index: u32,
    name: Option<&CStr>,
) -> Option<Node<'static>> {
    (api::window_hooks().child_frame)(doc?, index, name)
}

pub(crate) fn new_document() -> Node<'static> {
    Node::alloc(southstar_dom::Kind::Document)
}

pub(crate) fn new_element(name: &CStr) -> Node<'static> {
    southstar_dom::node::new_element(name.to_bytes())
}

pub(crate) fn append_child(parent: Node<'_>, child: Node<'_>) {
    parent.append(child);
}

pub(crate) fn set_attr(node: Node<'_>, name: &CStr, value: Option<&CStr>) {
    southstar_dom::attrs::set_attribute(node, name, value.map(CStr::to_bytes));
}

pub(crate) fn remove_attr(node: Node<'_>, name: &CStr) {
    southstar_dom::attrs::remove_attribute(node, name);
}

pub(crate) fn mark_visited(url: &CStr) {
    southstar_css::mark_visited(url.to_bytes());
}

pub(crate) fn mark_attr_dirty(node: Node<'_>, name: &CStr, old_value: &CStr) {
    southstar_css::mark_attr_dirty(node, Some(name.to_bytes()), Some(old_value.to_bytes()));
}

pub(crate) fn current_url(js: Js) -> Option<CString> {
    Some(api::current_url(js.ptr()))
}

fn c_string(mut bytes: Vec<u8>) -> CString {
    if let Some(nul) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(nul);
    }
    CString::new(bytes).unwrap_or_default()
}

pub(crate) fn resolve_url(base: &CStr, href: &CStr) -> Option<CString> {
    southstar_url::bytes::resolve(Some(base.to_bytes()), href.to_bytes()).map(c_string)
}

pub(crate) fn layout_root(js: Js) -> Option<BoxRef<'static>> {
    api::layout_root(js.ptr())
}

pub(crate) fn find_box<'a>(root: BoxRef<'a>, node: Node<'_>) -> Option<BoxRef<'a>> {
    southstar_js_geometry::find_box_by_dom(root, node.as_ptr().cast())
}

pub(crate) fn viewport_height() -> f64 {
    southstar_css::viewport().1
}

pub(crate) fn halted(js: Js) -> bool {
    api::halted(js.ptr())
}

pub(crate) fn in_frame_load(js: Js) -> bool {
    api::in_frame_load(js.ptr())
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.ptr());
}

fn iframe_child_frame_of(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let [window, key, ..] = args else {
        return Ok(Value::undefined());
    };
    let doc = nsr_window_current_document_for(scope, window);
    match ctx::property_key(scope, key)? {
        PropertyKey::Index(index) => owner::child_frame_window(scope, doc, index, None, true),
        PropertyKey::Name(name) => {
            owner::child_frame_window(scope, doc, 0, Some(name.as_c_str()), true)
        }
    }
}

fn realm_clone(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(value) = args.first() else {
        return Ok(Value::undefined());
    };
    let cx = scope_ctx(scope);
    Ok(cloner::clone_into(Js::of_ctx(cx), cx, value))
}

pub(crate) fn child_frame_of_function(ctx: Ctx) -> Value {
    ctx.enter(|scope| scope.function("childFrameOf", 2, iframe_child_frame_of))
}

pub(crate) fn realm_clone_function(ctx: Ctx) -> Value {
    ctx.enter(|scope| scope.function("realmClone", 1, realm_clone))
}

fn realm_for_node(scope: &Scope<'_>, node: Node<'_>) -> ContextHandle {
    crate::node_realm_context(Page::current(scope), node)
}

fn cloners_made(js: Option<Page>) -> bool {
    let js = Js::of(js);
    !js.is_null() && page::peek(js, |page| page.cloners_made).unwrap_or(false)
}

fn frame_url(js: Option<Page>, frame: Option<Node<'_>>) -> Option<CString> {
    let js = Js::of(js);
    let frame = frame?;
    if js.is_null() {
        return None;
    }
    page::frame_entry(js, crate::realm::frame_of(frame), |entry| entry.url.clone()).flatten()
}

fn realms_same_origin(js: Option<Page>, a: ContextHandle, b: ContextHandle) -> bool {
    let js = Js::of(js);
    match (Ctx::from_handle(a), Ctx::from_handle(b)) {
        (Some(a), Some(b)) if !js.is_null() => crate::origin::realms_same_origin(js, a, b),
        _ => true,
    }
}

fn forget_node(js: Option<Page>, node: Node<'_>) {
    let js = Js::of(js);
    if js.is_null() {
        return;
    }
    page::peek_mut(js, |page| {
        if let Some(entry) = page.frames.get_mut(&crate::realm::frame_of(node)) {
            entry.url = None;
            entry.referrer = None;
        }
    });
}

fn teardown(js: Option<Page>) {
    let js = Js::of(js);
    let cloners = cloner::clear(js);
    drop(cloners);
    let page = page::take(js);
    drop(page);
}

fn pending_count(js: Option<Page>) -> u32 {
    crate::pending_iframe_count(js) as u32
}

fn pending_first(js: Option<Page>) -> Option<Node<'static>> {
    let js = Js::of(js);
    if js.is_null() {
        return None;
    }
    queue::first_pending(js).and_then(frame_node)
}

fn pending_remove_first(js: Option<Page>) {
    let js = Js::of(js);
    if !js.is_null() {
        queue::remove_first_pending(js);
    }
}

fn purge_subtree_pending(js: Option<Page>, root: Node<'_>) {
    let js = Js::of(js);
    if !js.is_null() {
        queue::purge_subtree(js, root);
    }
}

fn scrub_globals(js: Option<Page>, iframe: Node<'_>) {
    globals::scrub(Js::of(js), iframe);
}

fn submit_form(js: Option<Page>, form: Node<'_>, submitter: Option<Node<'_>>) -> bool {
    crate::submit::submit_into_frame(Js::of(js), form, submitter)
}

fn load_now(js: Option<Page>, iframe: Node<'_>) {
    crate::load::load_now(Js::of(js), iframe);
}

pub fn install_runtime_hooks() {
    southstar_canvas::links::set_js_frames_links(southstar_canvas::links::JsFramesLinks {
        realm_for_node,
    });
    api::set_frames_hooks(api::FramesHooks {
        submit_form,
        purge_subtree_pending: |js, a1| purge_subtree_pending(Some(js), a1),
        pending_count: |js| pending_count(Some(js)),
        pending_first: |js| pending_first(Some(js)),
        pending_remove_first: |js| pending_remove_first(Some(js)),
        load_now: |js, a1| load_now(Some(js), a1),
        clear_initial_blank: |js| crate::clear_initial_blank_frames(Some(js)),
        teardown: |js| teardown(Some(js)),
        schedule_load: crate::schedule_iframe_load,
        scrub_globals,
        promote_deferred: crate::promote_deferred_iframes,
        frame_context: crate::frame_context,
        frame_url: |js, a1| frame_url(Some(js), a1),
        document_node: crate::iframe_document_node,
        frame_of_realm: |js, a1| crate::frame_of_realm(Some(js), a1),
        node_realm_context: |js, a1| crate::node_realm_context(Some(js), a1),
        cloners_made: |js| cloners_made(Some(js)),
        forget_node: |js, a1| forget_node(Some(js), a1),
        realms_same_origin: |js, a1, a2| realms_same_origin(Some(js), a1, a2),
        schedule_static_iframes,
        realm_proto_for: |js, a1, a2| realm_proto_for(Some(js), a1, a2),
        frame_realm_window,
        frame_realm_document,
    });
}

fn schedule_static_iframes(js: Option<Page>, root: Node<'_>) {
    if let Some(root) = root.downgrade().get() {
        crate::schedule_static_iframes(js, root);
    }
}

fn realm_proto_for(js: Option<Page>, realm: ContextHandle, proto: &Value) -> Value {
    let (Some(cx), js) = (Ctx::from_handle(realm), Js::of(js)) else {
        return proto.clone();
    };
    cloner::proto_for(js, cx, proto).unwrap_or_else(|| proto.clone())
}

fn frame_realm_window(js: Option<Page>, frame: Node<'_>) -> Value {
    let js = Js::of(js);
    frame
        .downgrade()
        .get()
        .and_then(|frame| realm::stored_window(js, frame))
        .unwrap_or_else(Value::null)
}

fn frame_realm_document(js: Option<Page>, frame: Node<'_>) -> Value {
    let js = Js::of(js);
    frame
        .downgrade()
        .get()
        .and_then(|frame| realm::stored_document(js, frame))
        .unwrap_or_else(Value::null)
}
