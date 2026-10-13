//! Southstar — the C ABI of the frame bindings as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) mod load;
pub(crate) mod qjs;

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSCFunction, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};
use southstar_layout::{BoxRef, NsBox};

use crate::page::{self, Frame};
use crate::{cloner, globals, origin, owner, queue, realm, sandbox, schedule};
use qjs::{Ctx, PropertyKey};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    pub fn of_ctx(ctx: Ctx) -> Js {
        Js(ctx.opaque() as usize)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }
}

unsafe extern "C" {
    fn ns_js_main_realm_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_navigator_brand(js: *const NsJs) -> JSValue;
    fn ns_js_pristine_promise(js: *const NsJs) -> JSValue;
    fn ns_js_storage_class_id() -> u32;
    fn ns_js_window_named_class_id() -> u32;
    fn ns_js_new_frame_context(js: *mut NsJs) -> *mut JSContext;
    fn ns_perf_move_timeline(js: *mut NsJs, from: *const c_void, to: *const c_void);
    fn ns_js_adopt_frame_clock(js: *mut NsJs, frame: *const c_void, ctx: *mut JSContext);
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_window_make_post_message(ctx: *mut JSContext, window: JSValue) -> JSValue;
    fn ns_js_frame_realm_finish(
        js: *mut NsJs,
        ctx: *mut JSContext,
        window: JSValue,
        parent_global: JSValue,
    );
    fn ns_js_node_doc_base(js: *mut NsJs, node: *const NsNode) -> *const c_char;
    fn ns_window_current_document_for(ctx: *mut JSContext, window: JSValue) -> *mut NsNode;
    fn ns_js_run_script_element(js: *mut NsJs, node: *mut NsNode, origin: *const c_char);
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_js_ce_main_doc(js: *const NsJs) -> *const NsNode;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_set_doc_ready_state(js: *mut NsJs, doc: *const NsNode, state: c_int);
    fn ns_make_realm_document(
        ctx: *mut JSContext,
        doc: *mut NsNode,
        url: *const c_char,
        charset: *const c_char,
        content_type: *const c_char,
        is_xml: GBoolean,
        inert: GBoolean,
    ) -> JSValue;
    fn ns_js_platform_globals(js: *mut NsJs) -> *mut glib::GHashTable;
    fn ns_window_child_frame(doc: *const NsNode, index: u32, name: *const c_char) -> *const NsNode;
    fn ns_node_new_document() -> *mut NsNode;
    fn ns_node_new_element(name: *mut c_char) -> *mut NsNode;
    fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode);
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_element_remove_attr(el: *mut NsNode, name: *const c_char);
    fn ns_css_mark_visited(abs_url: *const c_char);
    fn ns_css_mark_attr_dirty(target: *mut NsNode, name: *const c_char, old_value: *const c_char);
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_js_layout_root(js: *const NsJs) -> *const NsBox;
    fn ns_box_find_by_dom(root: *const NsBox, target: *const NsNode) -> *const NsBox;
    fn ns_css_viewport_h() -> f64;
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_in_frame_load(js: *const NsJs) -> GBoolean;
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_bind_fn(
        ctx: *mut JSContext,
        obj: JSValue,
        name: *const c_char,
        f: JSCFunction,
        argc: c_int,
    );
    fn ns_window_bind_post_message(ctx: *mut JSContext, global: JSValue);
    fn ns_ce_define(ctx: *mut JSContext, this: JSValue, argc: c_int, argv: *mut JSValue)
    -> JSValue;
    fn ns_ce_get(ctx: *mut JSContext, this: JSValue, argc: c_int, argv: *mut JSValue) -> JSValue;
    fn ns_ce_upgrade(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_ce_when_defined(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_ce_get_name(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_addEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_removeEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_dispatchEvent(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_element_addEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_element_removeEventListener(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_element_dispatchEvent(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_services_set_timeout(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_services_set_interval(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_services_clear_timer(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_services_queue_microtask(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_requestAnimationFrame(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_window_cancelAnimationFrame(
        ctx: *mut JSContext,
        this: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
}

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
    fn native(self) -> (&'static CStr, JSCFunction, c_int) {
        match self {
            Binding::CustomElementsDefine => (c"define", ns_ce_define, 3),
            Binding::CustomElementsGet => (c"get", ns_ce_get, 1),
            Binding::CustomElementsUpgrade => (c"upgrade", ns_ce_upgrade, 1),
            Binding::CustomElementsWhenDefined => (c"whenDefined", ns_ce_when_defined, 1),
            Binding::CustomElementsGetName => (c"getName", ns_ce_get_name, 1),
            Binding::WindowAddEventListener => (c"addEventListener", ns_window_addEventListener, 2),
            Binding::WindowRemoveEventListener => {
                (c"removeEventListener", ns_window_removeEventListener, 2)
            }
            Binding::WindowDispatchEvent => (c"dispatchEvent", ns_window_dispatchEvent, 1),
            Binding::ElementAddEventListener => {
                (c"addEventListener", ns_element_addEventListener, 2)
            }
            Binding::ElementRemoveEventListener => {
                (c"removeEventListener", ns_element_removeEventListener, 2)
            }
            Binding::ElementDispatchEvent => (c"dispatchEvent", ns_element_dispatchEvent, 1),
            Binding::SetTimeout => (c"setTimeout", ns_services_set_timeout, 2),
            Binding::SetInterval => (c"setInterval", ns_services_set_interval, 2),
            Binding::ClearTimeout => (c"clearTimeout", ns_services_clear_timer, 1),
            Binding::ClearInterval => (c"clearInterval", ns_services_clear_timer, 1),
            Binding::RequestAnimationFrame => {
                (c"requestAnimationFrame", ns_window_requestAnimationFrame, 1)
            }
            Binding::CancelAnimationFrame => {
                (c"cancelAnimationFrame", ns_window_cancelAnimationFrame, 1)
            }
            Binding::QueueMicrotask => (c"queueMicrotask", ns_services_queue_microtask, 1),
        }
    }
}

pub(crate) fn bind(ctx: Ctx, object: &Value, binding: Binding) {
    let (name, function, arity) = binding.native();
    unsafe {
        ns_bind_fn(
            ctx.ptr(),
            quickjs::raw(object),
            name.as_ptr(),
            function,
            arity,
        )
    };
}

pub(crate) fn bind_post_message(ctx: Ctx, global: &Value) {
    unsafe { ns_window_bind_post_message(ctx.ptr(), quickjs::raw(global)) };
}

pub(crate) fn main_realm(js: Js) -> Ctx {
    let main = unsafe { ns_js_main_realm_context(js.ptr()) };
    Ctx::from_ptr(main).unwrap_or_else(|| current_realm(js))
}

pub(crate) fn current_realm(js: Js) -> Ctx {
    Ctx::from_ptr(unsafe { ns_js_main_context(js.ptr()) }).unwrap_or(Ctx::NULL)
}

pub(crate) fn navigator_brand(js: Js, ctx: Ctx) -> Value {
    ctx.borrow(unsafe { ns_js_navigator_brand(js.ptr()) })
}

pub(crate) fn pristine_promise(js: Js, ctx: Ctx) -> Value {
    ctx.borrow(unsafe { ns_js_pristine_promise(js.ptr()) })
}

pub(crate) fn storage_class_id() -> u32 {
    unsafe { ns_js_storage_class_id() }
}

pub(crate) fn window_named_class_id() -> u32 {
    unsafe { ns_js_window_named_class_id() }
}

pub(crate) fn debug_realm() -> bool {
    std::env::var_os("NS_DBG_REALM").is_some()
}

pub(crate) fn new_frame_context(js: Js) -> Option<Ctx> {
    Ctx::from_ptr(unsafe { ns_js_new_frame_context(js.ptr()) })
}

pub(crate) fn move_timeline(js: Js, frame: Node<'_>, ctx: Ctx) {
    unsafe { ns_perf_move_timeline(js.ptr(), frame.as_ptr().cast(), ctx.ptr() as *const c_void) };
}

pub(crate) fn adopt_frame_clock(js: Js, frame: Option<Node<'_>>, ctx: Ctx) {
    let frame = Node::ptr_or_null(frame);
    unsafe { ns_js_adopt_frame_clock(js.ptr(), frame.cast(), ctx.ptr()) };
}

pub(crate) fn make_element(ctx: Ctx, node: Node<'_>) -> Value {
    ctx.wrap(unsafe { ns_make_element(ctx.ptr(), node.as_ptr()) })
}

pub(crate) fn make_post_message(ctx: Ctx, window: &Value) -> Value {
    ctx.wrap(unsafe { ns_window_make_post_message(ctx.ptr(), quickjs::raw(window)) })
}

pub(crate) fn finish_frame_realm(js: Js, ctx: Ctx, window: &Value, parent_global: &Value) {
    unsafe {
        ns_js_frame_realm_finish(
            js.ptr(),
            ctx.ptr(),
            quickjs::raw(window),
            quickjs::raw(parent_global),
        )
    };
}

pub(crate) fn node_doc_base(js: Js, node: Node<'_>) -> Option<Vec<u8>> {
    let base = unsafe { ns_js_node_doc_base(js.ptr(), node.as_ptr()) };
    (!base.is_null()).then(|| unsafe { CStr::from_ptr(base) }.to_bytes().to_vec())
}

pub(crate) fn run_script_element(js: Js, node: Node<'_>, origin: Option<&[u8]>) {
    let origin = origin.map(|origin| CString::new(origin).unwrap_or_default());
    let origin_ptr = origin.as_ref().map_or(core::ptr::null(), |o| o.as_ptr());
    unsafe { ns_js_run_script_element(js.ptr(), node.as_mut_ptr(), origin_ptr) };
}

pub(crate) fn frame_node(frame: Frame) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(frame.0 as *const NsNode) }
}

pub(crate) fn scope_ctx(scope: &Scope<'_>) -> Ctx {
    Ctx::from_ptr(quickjs::raw_context(scope)).unwrap_or(Ctx::NULL)
}

pub(crate) fn unwrap_element(value: &Value) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn ce_main_doc(js: Js) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_js_ce_main_doc(js.ptr())) }
}

pub(crate) fn current_document(js: Js) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_js_current_document(js.ptr())) }
}

pub(crate) fn is_cross_origin(js: Js, iframe: Node<'_>) -> bool {
    origin::is_cross_origin(js, iframe)
}

pub(crate) fn set_doc_ready_state(js: Js, doc: Node<'_>, state: c_int) {
    unsafe { ns_js_set_doc_ready_state(js.ptr(), doc.as_ptr(), state) };
}

fn opt_ptr(text: Option<&CStr>) -> *const c_char {
    text.map_or(core::ptr::null(), CStr::as_ptr)
}

pub(crate) fn make_realm_document(
    scope: &mut Scope<'_>,
    doc: Node<'_>,
    url: Option<&CStr>,
    charset: Option<&CStr>,
    content_type: Option<&CStr>,
    is_xml: bool,
) -> Result<Value, Value> {
    let raw = unsafe {
        ns_make_realm_document(
            quickjs::raw_context(scope),
            doc.as_mut_ptr(),
            opt_ptr(url),
            opt_ptr(charset),
            opt_ptr(content_type),
            glib::boolean(is_xml),
            glib::FALSE,
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) fn platform_global_names(js: Js) -> Vec<String> {
    let table = unsafe { ns_js_platform_globals(js.ptr()) };
    let mut names = Vec::new();
    if table.is_null() {
        return names;
    }
    let mut iter = glib::GHashTableIter::new();
    let mut key: *mut c_void = core::ptr::null_mut();
    unsafe { glib::g_hash_table_iter_init(&mut iter, table) };
    while unsafe { glib::g_hash_table_iter_next(&mut iter, &mut key, core::ptr::null_mut()) } != 0 {
        if let Some(name) = unsafe { c_bytes(key.cast()) } {
            names.push(String::from_utf8_lossy(name).into_owned());
        }
    }
    names
}

pub(crate) fn window_event_functions(scope: &mut Scope<'_>) -> [(&'static str, Value); 3] {
    [
        (
            "add",
            quickjs::c_function(scope, "addEventListener", 2, ns_window_addEventListener),
        ),
        (
            "remove",
            quickjs::c_function(
                scope,
                "removeEventListener",
                2,
                ns_window_removeEventListener,
            ),
        ),
        (
            "dispatch",
            quickjs::c_function(scope, "dispatchEvent", 1, ns_window_dispatchEvent),
        ),
    ]
}

pub(crate) fn window_child_frame(
    doc: Option<Node<'_>>,
    index: u32,
    name: Option<&CStr>,
) -> Option<Node<'static>> {
    unsafe {
        Node::from_ptr(ns_window_child_frame(
            Node::ptr_or_null(doc),
            index,
            opt_ptr(name),
        ))
    }
}

pub(crate) fn new_document() -> Node<'static> {
    unsafe { Node::from_ptr(ns_node_new_document()) }.expect("document allocation")
}

pub(crate) fn new_element(name: &CStr) -> Node<'static> {
    let name = unsafe { glib::g_strdup(name.as_ptr()) };
    unsafe { Node::from_ptr(ns_node_new_element(name)) }.expect("element allocation")
}

pub(crate) fn append_child(parent: Node<'_>, child: Node<'_>) {
    unsafe { ns_node_append_child(parent.as_mut_ptr(), child.as_mut_ptr()) };
}

fn owned_text(text: *mut c_char) -> Option<CString> {
    if text.is_null() {
        return None;
    }
    let owned = unsafe { CStr::from_ptr(text) }.to_owned();
    unsafe { glib::g_free(text.cast()) };
    Some(owned)
}

pub(crate) fn set_attr(node: Node<'_>, name: &CStr, value: Option<&CStr>) {
    unsafe { ns_element_set_attr(node.as_mut_ptr(), name.as_ptr(), opt_ptr(value)) };
}

pub(crate) fn remove_attr(node: Node<'_>, name: &CStr) {
    unsafe { ns_element_remove_attr(node.as_mut_ptr(), name.as_ptr()) };
}

pub(crate) fn mark_visited(url: &CStr) {
    unsafe { ns_css_mark_visited(url.as_ptr()) };
}

pub(crate) fn mark_attr_dirty(node: Node<'_>, name: &CStr, old_value: &CStr) {
    unsafe { ns_css_mark_attr_dirty(node.as_mut_ptr(), name.as_ptr(), old_value.as_ptr()) };
}

pub(crate) fn current_url(js: Js) -> Option<CString> {
    let url = unsafe { ns_js_current_url(js.ptr()) };
    (!url.is_null()).then(|| unsafe { CStr::from_ptr(url) }.to_owned())
}

pub(crate) fn resolve_url(base: &CStr, href: &CStr) -> Option<CString> {
    owned_text(unsafe { ns_url_resolve(base.as_ptr(), href.as_ptr()) })
}

pub(crate) fn layout_root(js: Js) -> Option<BoxRef<'static>> {
    unsafe { BoxRef::from_ptr(ns_js_layout_root(js.ptr())) }
}

pub(crate) fn find_box(root: BoxRef<'_>, node: Node<'_>) -> Option<BoxRef<'static>> {
    unsafe { BoxRef::from_ptr(ns_box_find_by_dom(root.as_ptr(), node.as_ptr())) }
}

pub(crate) fn viewport_height() -> f64 {
    unsafe { ns_css_viewport_h() }
}

pub(crate) fn halted(js: Js) -> bool {
    unsafe { ns_js_halted(js.ptr()) != 0 }
}

pub(crate) fn in_frame_load(js: Js) -> bool {
    unsafe { ns_js_in_frame_load(js.ptr()) != 0 }
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.ptr()) };
}

fn child_frame_window(cx: Ctx, doc: *mut NsNode, index: u32, name: Option<&CStr>) -> JSValue {
    cx.enter(|scope| {
        let doc = unsafe { Node::from_ptr(doc) };
        let result = owner::child_frame_window(scope, doc, index, name, true);
        quickjs::result_raw(scope, result)
    })
}

pub(crate) fn child_frame_of_function(ctx: Ctx) -> Value {
    ctx.enter(|scope| quickjs::c_function(scope, "childFrameOf", 2, ns_iframe_child_frame_of))
}

pub(crate) fn realm_clone_function(ctx: Ctx) -> Value {
    ctx.enter(|scope| quickjs::c_function(scope, "realmClone", 1, ns_realm_clone_fn))
}

unsafe fn c_bytes<'a>(text: *const c_char) -> Option<&'a [u8]> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_bytes())
}

unsafe fn args<'a>(argc: c_int, argv: *mut JSValue) -> &'a [JSValue] {
    if argv.is_null() || argc <= 0 {
        &[]
    } else {
        unsafe { core::slice::from_raw_parts(argv, argc as usize) }
    }
}

fn frame_ptr(frame: Option<Frame>) -> *mut NsNode {
    frame.map_or(core::ptr::null_mut(), |frame| frame.0 as *mut NsNode)
}

fn frame_key(node: *const NsNode) -> Frame {
    Frame(node as usize)
}

unsafe extern "C" fn ns_iframe_child_frame_of(
    ctx: *mut JSContext,
    _this: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    let args = unsafe { args(argc, argv) };
    let Some(cx) = Ctx::from_ptr(ctx) else {
        return quickjs::UNDEFINED;
    };
    if args.len() < 2 {
        return quickjs::UNDEFINED;
    }
    let doc = unsafe { ns_window_current_document_for(ctx, args[0]) };
    let key = cx.borrow(args[1]);
    match cx.property_key(&key) {
        None => cx.enter(|scope| {
            let error = quickjs::take_exception(scope);
            quickjs::result_raw(scope, Err(error))
        }),
        Some(PropertyKey::Index(index)) => child_frame_window(cx, doc, index, None),
        Some(PropertyKey::Name(name)) => child_frame_window(cx, doc, 0, Some(name.as_c_str())),
    }
}

unsafe extern "C" fn ns_realm_clone_fn(
    ctx: *mut JSContext,
    _this: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    let args = unsafe { args(argc, argv) };
    let Some(cx) = Ctx::from_ptr(ctx) else {
        return quickjs::UNDEFINED;
    };
    let Some(&value) = args.first() else {
        return quickjs::UNDEFINED;
    };
    let value = cx.borrow(value);
    quickjs::into_raw(cloner::clone_into(Js::of_ctx(cx), cx, &value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_realm_proto_for(
    js: *mut NsJs,
    realm: *mut JSContext,
    proto: JSValue,
) -> JSValue {
    let (Some(cx), js) = (Ctx::from_ptr(realm), Js::of(js)) else {
        return proto;
    };
    let proto_value = cx.borrow(proto);
    match cloner::proto_for(js, cx, &proto_value) {
        Some(mapped) => quickjs::raw(&mapped),
        None => proto,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_realm_cloners_made(js: *const NsJs) -> GBoolean {
    let js = Js::of(js);
    glib::boolean(!js.is_null() && page::peek(js, |page| page.cloners_made).unwrap_or(false))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_cross_origin_window(
    ctx: *mut JSContext,
    target: JSValue,
) -> JSValue {
    let Some(cx) = Ctx::from_ptr(ctx) else {
        return quickjs::into_raw(Value::null());
    };
    let target = cx.wrap(target);
    quickjs::into_raw(realm::cross_origin_window(cx, target))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_realm_window(js: *mut NsJs, frame: *const NsNode) -> JSValue {
    let js = Js::of(js);
    let window = unsafe { Node::from_ptr(frame) }.and_then(|frame| realm::stored_window(js, frame));
    quickjs::into_raw(window.unwrap_or_else(Value::null))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_realm_window(
    ctx: *mut JSContext,
    this_val: JSValue,
    node: *mut NsNode,
) -> JSValue {
    let (Some(cx), Some(node)) = (Ctx::from_ptr(ctx), unsafe { Node::from_ptr(node) }) else {
        return quickjs::into_raw(Value::null());
    };
    let element = cx.borrow(this_val);
    quickjs::into_raw(realm::realm_window(cx, &element, node))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_context(
    js: *const NsJs,
    frame: *const NsNode,
) -> *mut JSContext {
    let js = Js::of(js);
    if js.is_null() || frame.is_null() {
        return core::ptr::null_mut();
    }
    page::frame_entry(js, frame_key(frame), |entry| entry.realm)
        .flatten()
        .map_or(core::ptr::null_mut(), Ctx::ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_of_realm(
    js: *const NsJs,
    realm: *mut JSContext,
) -> *mut NsNode {
    let (js, Some(realm)) = (Js::of(js), Ctx::from_ptr(realm)) else {
        return core::ptr::null_mut();
    };
    if js.is_null() {
        return core::ptr::null_mut();
    }
    frame_ptr(page::frame_of_realm(js, realm))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_realm_context(
    js: *const NsJs,
    node: *const NsNode,
) -> *mut JSContext {
    let js = Js::of(js);
    let Some(node) = (unsafe { Node::from_ptr(node) }) else {
        return core::ptr::null_mut();
    };
    if js.is_null() {
        return core::ptr::null_mut();
    }
    realm::node_realm(js, node).map_or(core::ptr::null_mut(), Ctx::ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_realm_for_node(
    js: *const NsJs,
    node: *const NsNode,
) -> *mut JSContext {
    unsafe { ns_js_node_realm_context(js, node) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_frame_node(js: *mut NsJs, window: JSValue) -> *mut NsNode {
    let js = Js::of(js);
    if js.is_null() || !quickjs::raw_is_object(window) {
        return core::ptr::null_mut();
    }
    let ctx = current_realm(js);
    let window = ctx.borrow(window);
    let found = page::realms(js)
        .into_iter()
        .find(|(_, realm)| realm.global().same_object(&window))
        .map(|(frame, _)| frame);
    frame_ptr(found)
}

fn borrowed_text(text: Option<*const c_char>) -> *const c_char {
    text.unwrap_or(core::ptr::null())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_url(js: *const NsJs, frame: *const NsNode) -> *const c_char {
    let js = Js::of(js);
    if js.is_null() {
        return core::ptr::null();
    }
    borrowed_text(
        page::frame_entry(js, frame_key(frame), |entry| {
            entry.url.as_ref().map(|url| url.as_ptr())
        })
        .flatten(),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_referrer_for(
    js: *const NsJs,
    frame: *const NsNode,
) -> *const c_char {
    let js = Js::of(js);
    if js.is_null() {
        return core::ptr::null();
    }
    borrowed_text(
        page::frame_entry(js, frame_key(frame), |entry| {
            entry.referrer.as_ref().map(|referrer| referrer.as_ptr())
        })
        .flatten(),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_set_source(
    js: *mut NsJs,
    frame: *const NsNode,
    url: *const c_char,
    referrer: *const c_char,
) {
    let js = Js::of(js);
    if js.is_null() || frame.is_null() {
        return;
    }
    let url = unsafe { c_bytes(url) }.map(|url| CString::new(url).unwrap_or_default());
    let referrer =
        unsafe { c_bytes(referrer) }.map(|referrer| CString::new(referrer).unwrap_or_default());
    page::update_frame(js, frame_key(frame), |entry| {
        entry.url = url;
        entry.referrer = referrer;
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frames_forget_node(js: *mut NsJs, node: *const NsNode) {
    let js = Js::of(js);
    if js.is_null() {
        return;
    }
    page::peek_mut(js, |page| {
        if let Some(entry) = page.frames.get_mut(&frame_key(node)) {
            entry.url = None;
            entry.referrer = None;
        }
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frames_clear_initial_blank(js: *mut NsJs) {
    page::peek_mut(Js::of(js), |page| page.initial_blank.clear());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frames_reset(js: *mut NsJs) {
    let js = Js::of(js);
    let frames = page::peek_mut(js, |page| core::mem::take(&mut page.frames));
    drop(frames);
    let cloners = cloner::clear(js);
    drop(cloners);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frames_teardown(js: *mut NsJs) {
    let js = Js::of(js);
    let cloners = cloner::clear(js);
    drop(cloners);
    let page = page::take(js);
    drop(page);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_effective_sandbox(node: *const NsNode) -> c_uint {
    unsafe { Node::from_ptr(node) }.map_or(0, sandbox::effective)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_origin_is_opaque(frame: *mut NsNode) -> GBoolean {
    glib::boolean(unsafe { Node::from_ptr(frame) }.is_some_and(sandbox::origin_is_opaque))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_sandbox_blocks_forms(node: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { Node::from_ptr(node) }.is_some_and(sandbox::blocks_forms))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pending_iframe_count(js: *const NsJs) -> c_uint {
    let js = Js::of(js);
    if js.is_null() {
        return 0;
    }
    queue::pending_count(js) as c_uint
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pending_iframe_add(js: *mut NsJs, frame: *mut NsNode) -> GBoolean {
    let js = Js::of(js);
    if js.is_null() || frame.is_null() {
        return glib::FALSE;
    }
    glib::boolean(queue::add_pending(js, frame_key(frame)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pending_iframe_first(js: *const NsJs) -> *mut NsNode {
    let js = Js::of(js);
    if js.is_null() {
        return core::ptr::null_mut();
    }
    frame_ptr(queue::first_pending(js))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pending_iframe_remove_first(js: *mut NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        queue::remove_first_pending(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_deferred_iframe_add(js: *mut NsJs, frame: *mut NsNode) {
    let js = Js::of(js);
    if !js.is_null() && !frame.is_null() {
        queue::add_deferred(js, frame_key(frame));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_deferred_iframe_remove(js: *mut NsJs, frame: *mut NsNode) {
    let js = Js::of(js);
    if !js.is_null() {
        queue::remove_deferred(js, frame_key(frame));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_promote_deferred_iframes(js: *mut NsJs) {
    let js = Js::of(js);
    if !js.is_null() {
        queue::promote_deferred(js);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_purge_subtree_pending_iframes(js: *mut NsJs, root: *mut NsNode) {
    let js = Js::of(js);
    if let (false, Some(root)) = (js.is_null(), unsafe { Node::from_ptr(root) }) {
        queue::purge_subtree(js, root);
    }
}

unsafe fn c_str<'a>(text: *const c_char) -> Option<&'a CStr> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_document_node(iframe: *const NsNode) -> *mut NsNode {
    unsafe { Node::from_ptr(iframe) }
        .and_then(owner::document_node)
        .map_or(core::ptr::null_mut(), Node::as_mut_ptr)
}

unsafe fn getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    f: fn(&mut Scope<'_>, &Value) -> Result<Value, Value>,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let result = f(scope, &this);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_contentDocument(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, owner::content_document) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_contentWindow(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, owner::content_window) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_getSVGDocument(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    let f: NativeFn = owner::svg_document;
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_child_frame_window(
    ctx: *mut JSContext,
    doc: *mut NsNode,
    index: u32,
    name: *const c_char,
    raw: GBoolean,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let doc = Node::from_ptr(doc);
            let result = owner::child_frame_window(scope, doc, index, c_str(name), raw != 0);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_schedule_iframe_load_full(
    js: *mut NsJs,
    iframe: *mut NsNode,
    force: GBoolean,
) {
    if let Some(iframe) = unsafe { Node::from_ptr(iframe) } {
        schedule::schedule(Js::of(js), iframe, force != 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_schedule_iframe_load(js: *mut NsJs, iframe: *mut NsNode) {
    unsafe { ns_js_schedule_iframe_load_full(js, iframe, glib::FALSE) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_schedule_static_iframes(js: *mut NsJs, root: *mut NsNode) {
    if let Some(root) = unsafe { Node::from_ptr(root) } {
        schedule::schedule_static(Js::of(js), root, 0);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_is_cross_origin(
    js: *mut NsJs,
    iframe: *const NsNode,
) -> GBoolean {
    let Some(iframe) = (unsafe { Node::from_ptr(iframe) }) else {
        return glib::FALSE;
    };
    glib::boolean(origin::is_cross_origin(Js::of(js), iframe))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_scrub_iframe_globals(js: *mut NsJs, iframe: *const NsNode) {
    if let Some(iframe) = unsafe { Node::from_ptr(iframe) } {
        globals::scrub(Js::of(js), iframe);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_load_iframe_now(js: *mut NsJs, iframe: *mut NsNode) {
    if let Some(iframe) = unsafe { Node::from_ptr(iframe) } {
        crate::load::load_now(Js::of(js), iframe);
    }
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_realm_proto_for(
    js: *mut NsJs,
    realm: southstar_js_engine::ContextHandle,
    proto: &southstar_js_engine::Value,
) -> southstar_js_engine::Value {
    let (Some(cx), js) = (Ctx::from_ptr(realm.as_ptr().cast()), Js::of(js)) else {
        return proto.clone();
    };
    cloner::proto_for(js, cx, proto).unwrap_or_else(|| proto.clone())
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_js_frame_realm_window(js: *mut NsJs, frame: *const NsNode) -> Value {
    let js = Js::of(js);
    let window = unsafe { Node::from_ptr(frame) }.and_then(|frame| realm::stored_window(js, frame));
    window.unwrap_or_else(Value::null)
}

#[unsafe(no_mangle)]
pub fn nsr_iframe_cross_origin_window(scope: &mut Scope<'_>, target: &Value) -> Value {
    let raw = unsafe {
        ns_iframe_cross_origin_window(
            quickjs::raw_context(scope),
            quickjs::into_raw(target.clone()),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_window_frame_node(js: *mut NsJs, window: &Value) -> *mut NsNode {
    unsafe { ns_window_frame_node(js, quickjs::raw(window)) }
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_window_child_frame_window(
    scope: &mut Scope<'_>,
    doc: *mut NsNode,
    index: u32,
    name: Option<&CStr>,
    raw: bool,
) -> Result<Value, Value> {
    let doc = unsafe { Node::from_ptr(doc) };
    owner::child_frame_window(scope, doc, index, name, raw)
}
