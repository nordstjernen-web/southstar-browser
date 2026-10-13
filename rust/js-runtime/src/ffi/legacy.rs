//! Southstar — the C ABI shims of the realm record for crates still on raw QuickJS, and the raw natives and installers the window and worker realms bind until their crates export Rust ones.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

use southstar_dom::NsNode;
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSCFunction, JSContext, JSValue};
use southstar_js_engine::{ContextHandle, Scope, Value};

use super::record::AdoptedProtos;
use super::{Js, NsJs, c_text};
use crate::{api, worker};

pub(crate) type Native = JSCFunction;
pub(crate) type Installer = unsafe extern "C" fn(ctx: *mut JSContext, target: JSValue);
pub(crate) type ContextInstaller = unsafe extern "C" fn(ctx: *mut JSContext);
type JobFn = unsafe extern "C" fn(ctx: *mut JSContext, argc: c_int, argv: *mut JSValue) -> JSValue;

#[repr(C)]
pub(crate) struct RawDomProtos {
    node: JSValue,
    element: JSValue,
    html_element: JSValue,
    html_unknown_element: JSValue,
    svg_element: JSValue,
    svga_element: JSValue,
    mathml_element: JSValue,
    chardata: JSValue,
    text: JSValue,
    comment: JSValue,
    cdata: JSValue,
    pi: JSValue,
    doctype: JSValue,
    docfrag: JSValue,
    document: JSValue,
}

impl RawDomProtos {
    fn take(&self, mut take: impl FnMut(JSValue) -> Value) -> AdoptedProtos {
        AdoptedProtos {
            node: take(self.node),
            element: take(self.element),
            html_element: take(self.html_element),
            html_unknown_element: take(self.html_unknown_element),
            svg_element: take(self.svg_element),
            svga_element: take(self.svga_element),
            mathml_element: take(self.mathml_element),
            chardata: take(self.chardata),
            text: take(self.text),
            comment: take(self.comment),
            cdata: take(self.cdata),
            pi: take(self.pi),
            doctype: take(self.doctype),
            docfrag: take(self.docfrag),
            document: take(self.document),
        }
    }
}

pub(crate) fn c_function(scope: &mut Scope<'_>, name: &str, arity: u32, native: Native) -> Value {
    quickjs::c_function(scope, name, arity, native)
}

pub(crate) fn c_constructor_or_function(
    scope: &mut Scope<'_>,
    name: &str,
    arity: u32,
    native: Native,
) -> Value {
    quickjs::c_constructor_or_function(scope, name, arity, native)
}

pub(crate) fn c_constructor(
    scope: &mut Scope<'_>,
    name: &str,
    arity: u32,
    native: Native,
) -> Value {
    quickjs::c_constructor(scope, name, arity, native)
}

pub(crate) fn install(scope: &mut Scope<'_>, installer: Installer, target: &Value) {
    unsafe { installer(quickjs::raw_context(scope), quickjs::raw(target)) };
}

pub(crate) fn install_context(scope: &mut Scope<'_>, installer: ContextInstaller) {
    unsafe { installer(quickjs::raw_context(scope)) };
}

fn handle(ctx: *mut JSContext) -> ContextHandle {
    ContextHandle::from_ptr(ctx.cast())
}

unsafe fn with_scope<R>(ctx: *mut JSContext, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
    unsafe { quickjs::with_context(ctx, f) }
}

unsafe fn borrowed(ctx: *mut JSContext, raw: JSValue) -> Value {
    unsafe { with_scope(ctx, |scope| quickjs::borrow_value(scope, raw)) }
}

unsafe fn raw_args<'a>(argc: c_int, argv: *mut JSValue) -> &'a [JSValue] {
    if argc > 0 && !argv.is_null() {
        unsafe { core::slice::from_raw_parts(argv, argc as usize) }
    } else {
        &[]
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_worker_js_eval(
    js: *mut NsJs,
    src: *const c_char,
    len: usize,
    url: *const c_char,
    module: GBoolean,
    exception: *mut JSValue,
) -> c_int {
    unsafe { *exception = quickjs::UNDEFINED };
    if js.is_null() {
        return 2;
    }
    let source = unsafe { c_text(src) }.unwrap_or(c"");
    let url = unsafe { c_text(url) }.unwrap_or(c"<worker>");
    let status = Js(js).scope(|scope| worker::eval(scope, source, len, url, module != 0));
    match status {
        None => 2,
        Some(worker::EvalStatus::Done) => 0,
        Some(worker::EvalStatus::ParseError(thrown)) => {
            unsafe { *exception = quickjs::into_raw(thrown) };
            1
        }
        Some(worker::EvalStatus::Thrown(thrown)) => {
            unsafe { *exception = quickjs::into_raw(thrown) };
            2
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_bytes_view(
    ctx: *mut JSContext,
    value: JSValue,
    out_data: *mut *const u8,
    out_len: *mut usize,
    out_holder: *mut JSValue,
) -> GBoolean {
    unsafe {
        if !out_data.is_null() {
            *out_data = ptr::null();
        }
        if !out_len.is_null() {
            *out_len = 0;
        }
        if !out_holder.is_null() {
            *out_holder = quickjs::UNDEFINED;
        }
    }
    let value = unsafe { borrowed(ctx, value) };
    let view = unsafe { with_scope(ctx, |scope| api::bytes_view(scope, &value)) };
    let Some(view) = view else {
        return 0;
    };
    unsafe {
        if !out_data.is_null() {
            *out_data = view.data;
        }
        if !out_len.is_null() {
            *out_len = view.len;
        }
        if !out_holder.is_null() {
            *out_holder = quickjs::into_raw(view.holder);
        }
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_promise_reject(
    ctx: *mut JSContext,
    resolvers: *mut JSValue,
    message: *const c_char,
) {
    let message = super::glue::text(message).map_or(&b""[..], |m| m.to_bytes());
    unsafe {
        with_scope(ctx, |scope| {
            let resolve = quickjs::take_value(scope, *resolvers);
            let reject = quickjs::take_value(scope, *resolvers.add(1));
            api::reject_with_message(scope, &reject, message);
            drop(resolve);
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_queue_message_task(
    ctx: *mut JSContext,
    func: JobFn,
    argc: c_int,
    argv: *mut JSValue,
) {
    let args: Vec<Value> = unsafe { raw_args(argc, argv) }
        .iter()
        .map(|&raw| unsafe { borrowed(ctx, raw) })
        .collect();
    let queued = Js::of_context(handle(ctx)).filter(|_| argc <= 3).map(|js| {
        let run = Box::new(move |scope: &mut Scope<'_>| {
            let mut raw: Vec<JSValue> = args.iter().map(quickjs::raw).collect();
            let ctx = quickjs::raw_context(scope);
            let result = unsafe { func(ctx, raw.len() as c_int, raw.as_mut_ptr()) };
            let value = unsafe { quickjs::take_value(scope, result) };
            quickjs::checked(scope, value)
        });
        api::queue_task(js, handle(ctx), run);
    });
    if queued.is_none() {
        unsafe extern "C" {
            fn JS_EnqueueJob(
                ctx: *mut JSContext,
                func: JobFn,
                argc: c_int,
                argv: *mut JSValue,
            ) -> c_int;
        }
        unsafe { JS_EnqueueJob(ctx, func, argc, argv) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_timer_scope_leave(
    scope: *mut c_void,
    threw: GBoolean,
    exception: JSValue,
) {
    let ctx = unsafe { super::scopes::timer_scope_context(scope) };
    let exception = (threw != 0).then(|| unsafe { borrowed(ctx.as_ptr().cast(), exception) });
    unsafe { api::timer_scope_leave(scope, exception) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_timer_this_is_detached_window(
    js: *mut NsJs,
    ctx: *mut JSContext,
    this_val: JSValue,
) -> GBoolean {
    let this = unsafe { borrowed(ctx, this_val) };
    let detached =
        unsafe { with_scope(ctx, |scope| api::this_is_detached_window(js, scope, &this)) };
    glib::boolean(detached)
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_js_call_observer(
    js: *mut NsJs,
    ctx: *mut JSContext,
    cb: JSValue,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    report_type: *const c_char,
    fresh_budget: GBoolean,
) -> JSValue {
    let cb = unsafe { borrowed(ctx, cb) };
    let this = unsafe { borrowed(ctx, this_val) };
    let args: Vec<Value> = unsafe { raw_args(argc, argv) }
        .iter()
        .map(|&raw| unsafe { borrowed(ctx, raw) })
        .collect();
    let report = super::glue::text(report_type).map(|text| text.to_string_lossy().into_owned());
    let result = api::call_observer(
        js,
        handle(ctx),
        &cb,
        &this,
        &args,
        report.as_deref(),
        fresh_budget != 0,
    );
    unsafe { with_scope(ctx, |scope| quickjs::result_raw(scope, result)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_document_window_event(
    js: *mut NsJs,
    kind: *const c_char,
    event: JSValue,
) {
    let Some(page) = Js::of(js) else {
        return;
    };
    let Some(event) = page
        .ctx()
        .with(|scope| unsafe { quickjs::borrow_value(scope, event) })
    else {
        return;
    };
    let kind =
        super::glue::text(kind).map_or_else(String::new, |k| k.to_string_lossy().into_owned());
    api::dispatch_document_window_event(js, &kind, event);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_iframe_document(js: *const NsJs, out: *mut JSValue) -> GBoolean {
    let document = api::iframe_document(js);
    unsafe { *out = document.as_ref().map_or(quickjs::UNDEFINED, quickjs::raw) };
    glib::boolean(document.is_some())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_load_enter(
    js: *mut NsJs,
    frame: *mut NsNode,
    doc: *mut NsNode,
    url: *const c_char,
    realm_doc: JSValue,
) -> *mut c_void {
    let page = Js(js);
    let realm_doc = quickjs::raw_is_object(realm_doc)
        .then(|| {
            page.ctx()
                .with(|scope| unsafe { quickjs::borrow_value(scope, realm_doc) })
        })
        .flatten();
    super::scopes::frame_load_enter(page, frame, doc, url, realm_doc)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_array_length(ctx: *mut JSContext, arr: JSValue) -> u32 {
    let arr = unsafe { borrowed(ctx, arr) };
    unsafe { with_scope(ctx, |scope| api::array_length(scope, &arr)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_kind_proto(js: *const NsJs, n: *const NsNode) -> JSValue {
    let proto = unsafe { super::record::kind_proto_value(js, n) };
    proto.as_ref().map_or(quickjs::UNDEFINED, quickjs::raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_bind_event_target_listeners(ctx: *mut JSContext, obj: JSValue) {
    let obj = unsafe { borrowed(ctx, obj) };
    unsafe { with_scope(ctx, |scope| api::bind_event_target_listeners(scope, &obj)) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_namedmap_owner(this_val: JSValue) -> *mut NsNode {
    unsafe { quickjs::with_raw(this_val, super::imports::nsr_live_owner_node) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_apply_proto(ctx: *mut JSContext, obj: JSValue) {
    let obj = unsafe { borrowed(ctx, obj) };
    unsafe { with_scope(ctx, |scope| api::attr_apply_proto(scope, &obj)) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_build_attributes(ctx: *mut JSContext, owner: JSValue) -> JSValue {
    let owner = unsafe { borrowed(ctx, owner) };
    unsafe {
        with_scope(ctx, |scope| {
            let attributes = api::build_attributes(scope, &owner);
            quickjs::into_raw(attributes)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_frame_realm_finish(
    js: *mut NsJs,
    fctx: *mut JSContext,
    fg: JSValue,
    parent_global: JSValue,
) {
    let frame_global = unsafe { borrowed(fctx, fg) };
    let parent_global = unsafe { borrowed(fctx, parent_global) };
    unsafe { super::record::frame_realm_finish(js, handle(fctx), &frame_global, &parent_global) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_root_for(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> *mut NsNode {
    let this = unsafe { borrowed(ctx, this_val) };
    unsafe { with_scope(ctx, |scope| api::document_root_for(scope, &this)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_wrapper(ctx: *mut JSContext, n: *const NsNode) -> JSValue {
    let wrapper = handle(ctx).with(|scope| api::node_wrapper(scope, n));
    wrapper
        .flatten()
        .map_or(quickjs::UNDEFINED, quickjs::into_raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_new_orphan_element(
    ctx: *mut JSContext,
    name: *const c_char,
) -> JSValue {
    let name = unsafe { glib::bytes(name) }.unwrap_or_default().to_vec();
    unsafe {
        with_scope(ctx, |scope| {
            let element = api::new_orphan_element(scope, &name);
            quickjs::result_raw(scope, element)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_illegal_constructor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, api::illegal_constructor) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_register_tag_proto(
    ctx: *mut JSContext,
    tag: *const c_char,
    proto: JSValue,
) {
    let Some(tag) = (unsafe { glib::bytes(tag) }) else {
        return;
    };
    let proto = unsafe { borrowed(ctx, proto) };
    unsafe { with_scope(ctx, |scope| api::register_tag_proto(scope, tag, proto)) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_adopt_dom_protos(ctx: *mut JSContext, protos: *const RawDomProtos) {
    let Some(raw) = (unsafe { protos.as_ref() }) else {
        return;
    };
    unsafe {
        with_scope(ctx, |scope| {
            let protos = raw.take(|raw| quickjs::take_value(scope, raw));
            api::adopt_dom_protos(scope, protos);
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tag_caller_document(ctx: *mut JSContext, node_val: JSValue) {
    let node = unsafe { borrowed(ctx, node_val) };
    unsafe { with_scope(ctx, |scope| api::tag_caller_document(scope, &node)) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_exception_message(ctx: *mut JSContext, ex: JSValue) -> *mut c_char {
    let ex = unsafe { borrowed(ctx, ex) };
    let text = unsafe { with_scope(ctx, |scope| api::exception_message(scope, &ex)) };
    glib::strdup(&text)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_navigator_brand(js: *const NsJs) -> JSValue {
    let brand = unsafe { super::record::navigator_brand_ref(js) };
    brand.map_or(quickjs::UNDEFINED, quickjs::raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_pristine_promise(js: *const NsJs) -> JSValue {
    let promise = unsafe { super::record::pristine_promise_ref(js) };
    promise.map_or(quickjs::UNDEFINED, quickjs::raw)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_register_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        with_scope(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            api::register_element_class(scope, proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_element(ctx: *mut JSContext, cnode: *const NsNode) -> JSValue {
    unsafe {
        with_scope(ctx, |scope| {
            let element = api::make_element(scope, cnode);
            quickjs::result_raw(scope, element)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_unwrap_element(value: JSValue) -> *const NsNode {
    let node = unsafe { quickjs::with_raw(value, api::unwrap_element) };
    southstar_dom::Node::ptr_or_null(node)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_element_class_id() -> u32 {
    quickjs::class_id(&super::wrapper::ELEMENT_CLASS)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_new_document_object(js: *mut NsJs) -> JSValue {
    let Some(page) = Js::of(js) else {
        return quickjs::UNDEFINED;
    };
    let document = page.scope(|scope| {
        let document = api::new_document_object(js, scope);
        quickjs::result_raw(scope, document)
    });
    document.unwrap_or(quickjs::UNDEFINED)
}
