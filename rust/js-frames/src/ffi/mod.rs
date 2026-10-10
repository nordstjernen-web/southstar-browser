//! Southstar — the C ABI of the frame bindings as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) mod qjs;

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::Value;
use southstar_js_engine::quickjs::{self, JSCFunction, JSContext, JSValue};

use crate::exposed::ExposedNames;
use crate::page::{self, Frame};
use crate::{cloner, queue, realm, restore, sandbox};
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
    fn ns_iframe_platform_names(ctx: *mut JSContext, js: *mut NsJs) -> JSValue;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_window_make_post_message(ctx: *mut JSContext, window: JSValue) -> JSValue;
    fn ns_js_frame_window_events(ctx: *mut JSContext) -> JSValue;
    fn ns_js_frame_realm_finish(
        js: *mut NsJs,
        ctx: *mut JSContext,
        window: JSValue,
        parent_global: JSValue,
    );
    fn ns_iframe_content_document(
        ctx: *mut JSContext,
        element: JSValue,
        node: *mut NsNode,
    ) -> JSValue;
    fn ns_js_node_doc_base(js: *mut NsJs, node: *const NsNode) -> *const c_char;
    fn ns_window_current_document_for(ctx: *mut JSContext, window: JSValue) -> *mut NsNode;
    fn ns_window_child_frame_window(
        ctx: *mut JSContext,
        doc: *mut NsNode,
        index: u32,
        name: *const c_char,
        raw: GBoolean,
    ) -> JSValue;
    fn ns_js_run_script_element(js: *mut NsJs, node: *mut NsNode, origin: *const c_char);
    fn ns_js_iframe_beyond_load_range(js: *mut NsJs, frame: *mut NsNode) -> GBoolean;
    fn ns_js_schedule_iframe_load(js: *mut NsJs, frame: *mut NsNode);
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

pub(crate) fn platform_names(js: Js, ctx: Ctx) -> Value {
    ctx.wrap(unsafe { ns_iframe_platform_names(ctx.ptr(), js.ptr()) })
}

pub(crate) fn make_element(ctx: Ctx, node: Node<'_>) -> Value {
    ctx.wrap(unsafe { ns_make_element(ctx.ptr(), node.as_ptr()) })
}

pub(crate) fn make_post_message(ctx: Ctx, window: &Value) -> Value {
    ctx.wrap(unsafe { ns_window_make_post_message(ctx.ptr(), quickjs::raw(window)) })
}

pub(crate) fn frame_window_events(ctx: Ctx) -> Value {
    ctx.wrap(unsafe { ns_js_frame_window_events(ctx.ptr()) })
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

pub(crate) fn content_document(ctx: Ctx, element: &Value, node: Node<'_>) -> Value {
    ctx.wrap(unsafe {
        ns_iframe_content_document(ctx.ptr(), quickjs::raw(element), node.as_mut_ptr())
    })
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

pub(crate) fn beyond_load_range(js: Js, frame: Frame) -> bool {
    unsafe { ns_js_iframe_beyond_load_range(js.ptr(), frame.0 as *mut NsNode) != 0 }
}

pub(crate) fn schedule_iframe_load(js: Js, frame: Frame) {
    unsafe { ns_js_schedule_iframe_load(js.ptr(), frame.0 as *mut NsNode) };
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
        Some(PropertyKey::Index(index)) => unsafe {
            ns_window_child_frame_window(ctx, doc, index, core::ptr::null(), glib::TRUE)
        },
        Some(PropertyKey::Name(name)) => unsafe {
            ns_window_child_frame_window(ctx, doc, 0, name.as_ptr(), glib::TRUE)
        },
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
pub unsafe extern "C" fn ns_iframe_make_scope(
    ctx: *mut JSContext,
    iframe_doc: JSValue,
    initial_url: *const c_char,
    doc_url: *const c_char,
    sandbox: c_uint,
) -> JSValue {
    let Some(cx) = Ctx::from_ptr(ctx) else {
        return quickjs::into_raw(Value::null());
    };
    let document = cx.borrow(iframe_doc);
    let scope = realm::make_scope(
        cx,
        &document,
        unsafe { c_bytes(initial_url) },
        unsafe { c_bytes(doc_url) },
        sandbox,
    );
    quickjs::into_raw(scope)
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

#[allow(clippy::too_many_arguments)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_make_realm_context(
    js: *mut NsJs,
    iframe: *mut NsNode,
    iframe_doc: JSValue,
    initial_url: *const c_char,
    doc_url: *const c_char,
    sandbox: c_uint,
    reuse: *mut JSContext,
    out_window: *mut JSValue,
    out_location: *mut JSValue,
    out_history: *mut JSValue,
) -> *mut JSContext {
    let js = Js::of(js);
    let null = || quickjs::into_raw(Value::null());
    unsafe {
        *out_window = null();
        *out_location = null();
        *out_history = null();
    }
    let ctx = current_realm(js);
    let document = ctx.borrow(iframe_doc);
    let iframe = unsafe { Node::from_ptr(iframe) };
    let Some(made) = realm::make_realm_context(
        js,
        iframe,
        &document,
        unsafe { c_bytes(initial_url) },
        unsafe { c_bytes(doc_url) },
        sandbox,
        Ctx::from_ptr(reuse),
    ) else {
        return core::ptr::null_mut();
    };
    unsafe {
        *out_window = quickjs::into_raw(made.window);
        *out_location = quickjs::into_raw(made.location);
        *out_history = quickjs::into_raw(made.history);
    }
    made.ctx.ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_store_realm_window(
    js: *mut NsJs,
    iframe: *mut NsNode,
    window: JSValue,
) {
    let js = Js::of(js);
    let Some(iframe) = (unsafe { Node::from_ptr(iframe) }) else {
        return;
    };
    if js.is_null() {
        return;
    }
    let window = current_realm(js).borrow(window);
    realm::store_window(js, iframe, &window);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_lookup_realm_window(
    js: *mut NsJs,
    iframe: *mut NsNode,
) -> JSValue {
    let js = Js::of(js);
    let window =
        unsafe { Node::from_ptr(iframe) }.and_then(|iframe| realm::stored_window(js, iframe));
    quickjs::into_raw(window.unwrap_or_else(Value::null))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_realm_window(js: *mut NsJs, frame: *const NsNode) -> JSValue {
    unsafe { ns_iframe_lookup_realm_window(js, frame as *mut NsNode) }
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
pub unsafe extern "C" fn ns_js_run_iframe_modules(
    js: *mut NsJs,
    modules: *const *mut NsNode,
    count: c_uint,
    origin: *const c_char,
    iframe_doc: JSValue,
    iframe_scope: JSValue,
    sandbox: c_uint,
) {
    let js = Js::of(js);
    if js.is_null() || modules.is_null() || count == 0 {
        return;
    }
    let nodes: Vec<Node<'_>> = unsafe { core::slice::from_raw_parts(modules, count as usize) }
        .iter()
        .filter_map(|&node| unsafe { Node::from_ptr(node) })
        .collect();
    let ctx = current_realm(js);
    let document = ctx.borrow(iframe_doc);
    let scope = ctx.borrow(iframe_scope);
    let origin = unsafe { c_bytes(origin) }.map(<[u8]>::to_vec);
    realm::run_modules(js, &nodes, origin.as_deref(), &document, &scope, sandbox);
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
pub unsafe extern "C" fn ns_js_frame_take_initial_blank(
    js: *mut NsJs,
    frame: *const NsNode,
) -> *mut JSContext {
    let js = Js::of(js);
    if js.is_null() || frame.is_null() {
        return core::ptr::null_mut();
    }
    page::peek_mut(js, |page| page.initial_blank.remove(&frame_key(frame)))
        .flatten()
        .map_or(core::ptr::null_mut(), Ctx::ptr)
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
pub extern "C" fn ns_iframe_exposed_names_new() -> *mut ExposedNames {
    Box::into_raw(Box::default())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_exposed_names_scan(
    names: *mut ExposedNames,
    src: *const c_char,
    len: usize,
) {
    let Some(names) = (unsafe { names.as_mut() }) else {
        return;
    };
    if src.is_null() || len == 0 {
        return;
    }
    names.scan(unsafe { core::slice::from_raw_parts(src.cast::<u8>(), len) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_exposed_names_script(names: *const ExposedNames) -> *mut c_char {
    let script = unsafe { names.as_ref() }.and_then(ExposedNames::exposing_script);
    match script {
        Some(script) => {
            let script = CString::new(script).unwrap_or_default();
            unsafe { glib::g_strdup(script.as_ptr()) }
        }
        None => core::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_iframe_exposed_names_free(names: *mut ExposedNames) {
    if !names.is_null() {
        drop(unsafe { Box::from_raw(names) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_proto_snapshot(ctx: *mut JSContext) -> JSValue {
    let Some(cx) = Ctx::from_ptr(ctx) else {
        return quickjs::into_raw(Value::null());
    };
    quickjs::into_raw(restore::proto_snapshot(cx))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_proto_cleanup(ctx: *mut JSContext, before: JSValue) {
    if let Some(cx) = Ctx::from_ptr(ctx) {
        let before = cx.borrow(before);
        restore::proto_cleanup(cx, &before);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_clear_global_zone(ctx: *mut JSContext) {
    if let Some(cx) = Ctx::from_ptr(ctx) {
        restore::clear_global_zone(cx);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_restore_globals(ctx: *mut JSContext) {
    if let Some(cx) = Ctx::from_ptr(ctx) {
        restore::restore_globals(cx);
    }
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
