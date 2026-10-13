//! Southstar — the C ABI of the interface installers as declared in src/js_internal.h, and the js.c class glue they call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod installers;
mod natives;
mod tables;

use core::ffi::{CStr, c_char, c_int};

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) enum Members {
    ElementFunctions = 0,
    ElementClass = 1,
    HtmlElement = 2,
    Dataset = 3,
    Text = 4,
    DocumentFragment = 5,
    Track = 6,
    Dialog = 7,
    Button = 8,
    TextProperty = 9,
    DataProperty = 10,
    ValueProperty = 11,
    OptionsMethods = 12,
}

pub(crate) struct DomProtos {
    pub node: Value,
    pub element: Value,
    pub html_element: Value,
    pub html_unknown_element: Value,
    pub svg_element: Value,
    pub svga_element: Value,
    pub mathml_element: Value,
    pub chardata: Value,
    pub text: Value,
    pub comment: Value,
    pub cdata: Value,
    pub pi: Value,
    pub doctype: Value,
    pub docfrag: Value,
    pub document: Value,
}

#[repr(C)]
struct RawDomProtos {
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

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_live_collection_kind(obj: JSValue) -> c_int;
    fn ns_node_is_shadow_root(n: *const NsNode) -> GBoolean;
    fn ns_js_install_dom_members(ctx: *mut JSContext, proto: JSValue, which: c_int);
    fn ns_js_register_tag_proto(ctx: *mut JSContext, tag: *const c_char, proto: JSValue);
    fn ns_js_adopt_dom_protos(ctx: *mut JSContext, protos: *const RawDomProtos);
    fn ns_perf_now_ms(js: *const NsJs) -> f64;
    fn ns_js_page_time_origin_real_ms(js: *const NsJs) -> f64;
    fn ns_js_navigator_brand(js: *const NsJs) -> JSValue;
    fn ns_install_event_handler_props(ctx: *mut JSContext, target: JSValue);
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn live_collection_kind(value: &Value) -> c_int {
    unsafe { ns_live_collection_kind(quickjs::raw(value)) }
}

pub(crate) fn is_shadow_root(node: Node<'_>) -> bool {
    unsafe { ns_node_is_shadow_root(node.as_ptr()) != glib::FALSE }
}

pub(crate) fn install_members(scope: &mut Scope<'_>, proto: &Value, members: Members) {
    unsafe {
        ns_js_install_dom_members(
            quickjs::raw_context(scope),
            quickjs::raw(proto),
            members as c_int,
        )
    };
}

pub(crate) fn register_tag_proto(scope: &mut Scope<'_>, tag: &str, proto: &Value) {
    let mut name = Vec::with_capacity(tag.len() + 1);
    name.extend_from_slice(tag.as_bytes());
    name.push(0);
    unsafe {
        ns_js_register_tag_proto(
            quickjs::raw_context(scope),
            name.as_ptr().cast(),
            quickjs::raw(proto),
        )
    };
}

pub(crate) fn adopt_dom_protos(scope: &mut Scope<'_>, protos: DomProtos) {
    let raw = RawDomProtos {
        node: quickjs::into_raw(protos.node),
        element: quickjs::into_raw(protos.element),
        html_element: quickjs::into_raw(protos.html_element),
        html_unknown_element: quickjs::into_raw(protos.html_unknown_element),
        svg_element: quickjs::into_raw(protos.svg_element),
        svga_element: quickjs::into_raw(protos.svga_element),
        mathml_element: quickjs::into_raw(protos.mathml_element),
        chardata: quickjs::into_raw(protos.chardata),
        text: quickjs::into_raw(protos.text),
        comment: quickjs::into_raw(protos.comment),
        cdata: quickjs::into_raw(protos.cdata),
        pi: quickjs::into_raw(protos.pi),
        doctype: quickjs::into_raw(protos.doctype),
        docfrag: quickjs::into_raw(protos.docfrag),
        document: quickjs::into_raw(protos.document),
    };
    unsafe { ns_js_adopt_dom_protos(quickjs::raw_context(scope), &raw) };
}

pub(crate) fn navigator_brand(scope: &Scope<'_>) -> Value {
    let js: *const NsJs = quickjs::context_opaque(scope).cast::<NsJs>().cast_const();
    if js.is_null() {
        return Value::undefined();
    }
    unsafe { quickjs::borrow_value(scope, ns_js_navigator_brand(js)) }
}

pub(crate) fn page_clock(scope: &Scope<'_>) -> (f64, f64) {
    let js: *const NsJs = quickjs::context_opaque(scope).cast::<NsJs>().cast_const();
    let origin = if js.is_null() {
        0.0
    } else {
        unsafe { ns_js_page_time_origin_real_ms(js) }
    };
    (origin, unsafe { ns_perf_now_ms(js) })
}

type GlobalInstaller = unsafe extern "C" fn(*mut JSContext, JSValue);

fn run_installer(scope: &mut Scope<'_>, global: &Value, f: GlobalInstaller) {
    unsafe { f(quickjs::raw_context(scope), quickjs::raw(global)) };
}

pub(crate) fn install_event_handler_props(scope: &mut Scope<'_>, global: &Value) {
    run_installer(scope, global, ns_install_event_handler_props);
}

pub(crate) fn hide_shared_array_buffer(scope: &mut Scope<'_>, global: &Value) {
    southstar_js_realm::hide_shared_array_buffer(scope, global);
}

pub(crate) fn install_intl(scope: &mut Scope<'_>, global: &Value) {
    southstar_js_intl::install(scope, global);
}

pub(crate) fn install_temporal(scope: &mut Scope<'_>, global: &Value) {
    let _ = southstar_js_temporal::install(scope, global);
}

pub(crate) fn install_realm(scope: &mut Scope<'_>, global: &Value) {
    southstar_js_realm::install(scope, global);
}

unsafe fn with_scope(ctx: *mut JSContext, f: impl FnOnce(&mut Scope<'_>)) {
    unsafe { quickjs::with_context(ctx, f) }
}

unsafe fn with_object(ctx: *mut JSContext, raw: JSValue, f: impl FnOnce(&mut Scope<'_>, &Value)) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, raw);
            f(scope, &value);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_hasinstance(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_object(ctx, global, crate::install_has_instance) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_tostringtag(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_object(ctx, global, crate::install_to_string_tag) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_dom_hierarchy(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_object(ctx, global, crate::install_dom_hierarchy) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_window_chrome(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_object(ctx, global, crate::install_window_chrome) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_define_element_unscopables(ctx: *mut JSContext, proto: JSValue) {
    unsafe { with_object(ctx, proto, crate::define_element_unscopables) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_node_shapes(ctx: *mut JSContext) {
    unsafe { with_scope(ctx, crate::install_node_shapes) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_element_shapes(ctx: *mut JSContext) {
    unsafe { with_scope(ctx, crate::install_element_shapes) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_pdf_plugins(ctx: *mut JSContext) {
    unsafe { with_scope(ctx, crate::install_pdf_plugins) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_link_interface_ctors(ctx: *mut JSContext) {
    unsafe { with_scope(ctx, crate::link_interface_ctors) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_lock_global_prototypes(ctx: *mut JSContext) {
    unsafe { with_scope(ctx, crate::lock_global_prototypes) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_shape_window_global(ctx: *mut JSContext) {
    unsafe { with_scope(ctx, crate::shape_window_global) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_web_api_shapes(ctx: *mut JSContext) {
    unsafe { with_scope(ctx, crate::install_web_api_shapes) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_navigator_shape(ctx: *mut JSContext) {
    unsafe { with_scope(ctx, crate::install_navigator_shape) };
}

fn text<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        return "";
    }
    unsafe { CStr::from_ptr(p) }.to_str().unwrap_or_default()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_namespace_object(
    ctx: *mut JSContext,
    global: JSValue,
    name: *const c_char,
    obj: JSValue,
    tag: *const c_char,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            let object = quickjs::take_value(scope, obj);
            crate::install_namespace_object(scope, &global, text(name), object, text(tag));
        })
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_install_window_compat(ctx: *mut JSContext, global: JSValue) {
    unsafe { with_object(ctx, global, crate::compat::install_window_compat) };
}

#[unsafe(no_mangle)]
pub fn nsr_install_namespace_object(
    scope: &mut Scope<'_>,
    global: &Value,
    name: &str,
    object: Value,
    tag: &str,
) {
    crate::install_namespace_object(scope, global, name, object, tag);
}
