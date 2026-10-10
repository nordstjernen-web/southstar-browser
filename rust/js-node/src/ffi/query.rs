//! Southstar — the C ABI of the Node getters, comparisons and attribute methods, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use super::{Js, NsJs, ns_unwrap_element, raw_js};
use crate::{Element, JsResult};

unsafe extern "C" {
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_attr_owner(value: JSValue) -> *mut NsNode;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_doc_wrapper_is_xml(ctx: *mut JSContext, doc_val: JSValue) -> GBoolean;
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_element_reflect_str_get(
        ctx: *mut JSContext,
        this_val: JSValue,
        attr: *const c_char,
        null_if_absent: GBoolean,
    ) -> JSValue;
    fn ns_make_live(
        ctx: *mut JSContext,
        owner: JSValue,
        kind: c_int,
        param: *const c_char,
    ) -> JSValue;
    fn ns_valid_attr_name(s: *const c_char) -> GBoolean;
    fn ns_validate_attr_ns(
        ctx: *mut JSContext,
        ns_uri: *const c_char,
        qname: *const c_char,
    ) -> JSValue;
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        n: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_js_remove_attr_recorded(js: *mut NsJs, n: *mut NsNode, name: *const c_char);
    fn ns_js_set_attr_ns_recorded(
        js: *mut NsJs,
        n: *mut NsNode,
        namespace_uri: *const c_char,
        prefix: *const c_char,
        local_name: *const c_char,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_js_remove_attr_ns_recorded(
        js: *mut NsJs,
        n: *mut NsNode,
        namespace_uri: *const c_char,
        local_name: *const c_char,
    );
    fn ns_input_resanitize_value(el: *mut NsNode);
    fn ns_js_details_toggle_open(js: *mut NsJs, details: *mut NsNode, open: GBoolean);
    fn ns_js_start_image_load(js: *mut NsJs, el: *mut NsNode, src: *const c_char);
    fn ns_js_schedule_iframe_load_full(js: *mut NsJs, iframe: *mut NsNode, force: GBoolean);
    fn ns_js_schedule_iframe_load(js: *mut NsJs, iframe: *mut NsNode);
    fn ns_js_img_src_layout_neutral(n: *const NsNode) -> GBoolean;
    fn ns_body_forward_content_handler(
        ctx: *mut JSContext,
        n: *const NsNode,
        name: *const c_char,
        code: *const c_char,
    );
    fn ns_css_attr_may_affect_style(target: *const NsNode, name: *const c_char) -> GBoolean;
    fn ns_js_request_repaint(js: *mut NsJs);
    fn ns_js_record_attr_change(
        js: *mut NsJs,
        target: *mut NsNode,
        name: *const c_char,
        old_value: *const c_char,
    );
    fn ns_ce_attr_changed(
        js: *mut NsJs,
        node: *mut NsNode,
        attr: *const c_char,
        old_value: *const c_char,
        new_value: *const c_char,
    );
    fn JS_ToCStringLen2(
        ctx: *mut JSContext,
        plen: *mut usize,
        val: JSValue,
        cesu8: GBoolean,
    ) -> *const c_char;
    fn JS_FreeCString(ctx: *mut JSContext, ptr: *const c_char);
}

fn c_ptr(text: Option<&CStr>) -> *const c_char {
    text.map_or(ptr::null(), CStr::as_ptr)
}

pub(crate) fn make_node(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), Node::ptr_or_null(node)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn attr_owner(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_attr_owner(quickjs::raw(value))) }
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    unsafe { Node::from_ptr(ns_js_current_document(js.0)) }
}

pub(crate) fn doc_is_xml(scope: &mut Scope<'_>, doc: &Value) -> bool {
    unsafe { ns_doc_wrapper_is_xml(quickjs::raw_context(scope), quickjs::raw(doc)) != 0 }
}

pub(crate) fn doc_base_url(js: Js) -> Option<Vec<u8>> {
    let base = unsafe { ns_js_doc_base_url(js.0) };
    if base.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(base) }.to_bytes().to_vec();
    unsafe { glib::g_free(base.cast()) };
    Some(bytes)
}

fn checked_raw(scope: &mut Scope<'_>, raw: JSValue) -> JsResult {
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) fn reflect_str_get(scope: &mut Scope<'_>, this: &Value, attr: &CStr) -> JsResult {
    let raw = unsafe {
        ns_element_reflect_str_get(
            quickjs::raw_context(scope),
            quickjs::raw(this),
            attr.as_ptr(),
            glib::FALSE,
        )
    };
    checked_raw(scope, raw)
}

pub(crate) fn make_live(scope: &mut Scope<'_>, owner: &Value, kind: c_int) -> JsResult {
    let raw = unsafe {
        ns_make_live(
            quickjs::raw_context(scope),
            quickjs::raw(owner),
            kind,
            ptr::null(),
        )
    };
    checked_raw(scope, raw)
}

pub(crate) fn valid_attr_name(name: &CStr) -> bool {
    unsafe { ns_valid_attr_name(name.as_ptr()) != 0 }
}

pub(crate) fn validate_attr_ns(
    scope: &mut Scope<'_>,
    namespace: Option<&CStr>,
    name: &CStr,
) -> JsResult<()> {
    let raw = unsafe {
        ns_validate_attr_ns(quickjs::raw_context(scope), c_ptr(namespace), name.as_ptr())
    };
    checked_raw(scope, raw).map(drop)
}

pub(crate) fn set_attr_recorded(js: Option<Js>, node: Element, name: &CStr, value: &CStr) {
    unsafe {
        ns_js_set_attr_recorded(raw_js(js), node.as_mut_ptr(), name.as_ptr(), value.as_ptr())
    };
}

pub(crate) fn remove_attr_recorded(js: Option<Js>, node: Element, name: &CStr) {
    unsafe { ns_js_remove_attr_recorded(raw_js(js), node.as_mut_ptr(), name.as_ptr()) };
}

pub(crate) struct NsAttrName<'a> {
    pub(crate) namespace: Option<&'a CStr>,
    pub(crate) prefix: Option<&'a CStr>,
    pub(crate) local: &'a CStr,
    pub(crate) name: &'a CStr,
}

pub(crate) fn set_attr_ns_recorded(
    js: Option<Js>,
    node: Element,
    qname: NsAttrName<'_>,
    value: &CStr,
) {
    unsafe {
        ns_js_set_attr_ns_recorded(
            raw_js(js),
            node.as_mut_ptr(),
            c_ptr(qname.namespace),
            c_ptr(qname.prefix),
            qname.local.as_ptr(),
            qname.name.as_ptr(),
            value.as_ptr(),
        )
    };
}

pub(crate) fn remove_attr_ns_recorded(
    js: Option<Js>,
    node: Element,
    namespace: Option<&CStr>,
    local: &CStr,
) {
    unsafe {
        ns_js_remove_attr_ns_recorded(
            raw_js(js),
            node.as_mut_ptr(),
            c_ptr(namespace),
            local.as_ptr(),
        )
    };
}

pub(crate) fn input_resanitize_value(node: Element) {
    unsafe { ns_input_resanitize_value(node.as_mut_ptr()) };
}

pub(crate) fn details_toggle_open(js: Option<Js>, node: Element, open: bool) {
    unsafe { ns_js_details_toggle_open(raw_js(js), node.as_mut_ptr(), glib::boolean(open)) };
}

pub(crate) fn start_image_load(js: Option<Js>, node: Element, src: &CStr) {
    unsafe { ns_js_start_image_load(raw_js(js), node.as_mut_ptr(), src.as_ptr()) };
}

pub(crate) fn schedule_iframe_load_full(js: Option<Js>, node: Element) {
    unsafe { ns_js_schedule_iframe_load_full(raw_js(js), node.as_mut_ptr(), glib::TRUE) };
}

pub(crate) fn schedule_iframe_load(js: Option<Js>, node: Element) {
    unsafe { ns_js_schedule_iframe_load(raw_js(js), node.as_mut_ptr()) };
}

pub(crate) fn img_src_layout_neutral(node: Element) -> bool {
    unsafe { ns_js_img_src_layout_neutral(node.as_ptr()) != 0 }
}

pub(crate) fn body_forward_content_handler(
    scope: &mut Scope<'_>,
    node: Element,
    name: &CStr,
    code: Option<&CStr>,
) {
    unsafe {
        ns_body_forward_content_handler(
            quickjs::raw_context(scope),
            node.as_ptr(),
            name.as_ptr(),
            c_ptr(code),
        )
    };
}

pub(crate) fn attr_may_affect_style(node: Element, name: &CStr) -> bool {
    unsafe { ns_css_attr_may_affect_style(node.as_ptr(), name.as_ptr()) != 0 }
}

pub(crate) fn request_repaint(js: Js) {
    unsafe { ns_js_request_repaint(js.0) };
}

pub(crate) fn record_attr_change(js: Js, node: Element, name: &CStr, old: Option<&CStr>) {
    unsafe { ns_js_record_attr_change(js.0, node.as_mut_ptr(), name.as_ptr(), c_ptr(old)) };
}

pub(crate) fn ce_attr_changed(
    js: Js,
    node: Element,
    name: &CStr,
    old: Option<&CStr>,
    new: Option<&CStr>,
) {
    unsafe {
        ns_ce_attr_changed(
            js.0,
            node.as_mut_ptr(),
            name.as_ptr(),
            c_ptr(old),
            c_ptr(new),
        )
    };
}

pub(crate) fn with_text<R>(
    scope: &mut Scope<'_>,
    value: &Value,
    f: impl FnOnce(&mut Scope<'_>, &[u8], &CStr) -> R,
) -> JsResult<R> {
    let ctx = quickjs::raw_context(scope);
    let mut len = 0usize;
    let text = unsafe { JS_ToCStringLen2(ctx, &mut len, quickjs::raw(value), glib::FALSE) };
    if text.is_null() {
        return Err(quickjs::take_exception(scope));
    }
    let bytes = unsafe { core::slice::from_raw_parts(text.cast::<u8>(), len) };
    let result = f(scope, bytes, unsafe { CStr::from_ptr(text) });
    unsafe { JS_FreeCString(ctx, text) };
    Ok(result)
}

const MAX_ARGS: usize = 3;

unsafe fn native_fixed(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let count = if argv.is_null() {
                0
            } else {
                (argc.max(0) as usize).min(MAX_ARGS)
            };
            let arg = |scope: &Scope<'_>, i: usize| {
                if i < count {
                    quickjs::borrow_value(scope, *argv.add(i))
                } else {
                    Value::undefined()
                }
            };
            let args = [arg(scope, 0), arg(scope, 1), arg(scope, 2)];
            let this = quickjs::borrow_value(scope, this_val);
            let result = f(scope, &this, &args[..count]);
            quickjs::result_raw(scope, result)
        })
    }
}

macro_rules! fixed_methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native_fixed(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

fixed_methods! {
    ns_element_setAttribute => crate::attributes::set_attribute,
    ns_element_removeAttribute => crate::attributes::remove_attribute,
    ns_element_toggleAttribute => crate::attributes::toggle_attribute,
    ns_element_getAttributeNS => crate::attributes::get_attribute_ns,
    ns_element_hasAttributeNS => crate::attributes::has_attribute_ns,
    ns_element_setAttributeNS => crate::attributes::set_attribute_ns,
    ns_element_removeAttributeNS => crate::attributes::remove_attribute_ns,
    ns_element_getAttributeNames => crate::attributes::get_attribute_names,
    ns_element_hasAttributes => crate::attributes::has_attributes,
    ns_element_isSameNode => crate::attributes::is_same_node,
}

unsafe fn with_raw_text<R>(
    ctx: *mut JSContext,
    value: JSValue,
    f: impl FnOnce(&CStr) -> R,
) -> Option<R> {
    let mut len = 0usize;
    let text = unsafe { JS_ToCStringLen2(ctx, &mut len, value, glib::FALSE) };
    if text.is_null() {
        return None;
    }
    let result = f(unsafe { CStr::from_ptr(text) });
    unsafe { JS_FreeCString(ctx, text) };
    Some(result)
}

unsafe fn rethrow(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let error = quickjs::take_exception(scope);
            quickjs::result_raw(scope, Err(error))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_getAttribute(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    let Some(node) = this_node(this_val).filter(|_| has_arg(argc, argv)) else {
        return quickjs::into_raw(Value::null());
    };
    let found = unsafe {
        with_raw_text(ctx, *argv, |raw| {
            crate::attributes::attribute_value(node, raw).map(|value| {
                quickjs::with_context(ctx, |scope| {
                    quickjs::into_raw(scope.string_from_bytes(value))
                })
            })
        })
    };
    match found {
        Some(Some(value)) => value,
        Some(None) => quickjs::into_raw(Value::null()),
        None => unsafe { rethrow(ctx) },
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_hasAttribute(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    let Some(node) = this_node(this_val).filter(|_| has_arg(argc, argv)) else {
        return boolean(false);
    };
    match unsafe { with_raw_text(ctx, *argv, |raw| crate::attributes::has_name(node, raw)) } {
        Some(found) => boolean(found),
        None => unsafe { rethrow(ctx) },
    }
}

fn this_node(this_val: JSValue) -> Option<Element> {
    unsafe { Node::from_ptr(ns_unwrap_element(this_val)) }
}

fn has_arg(argc: c_int, argv: *mut JSValue) -> bool {
    argc >= 1 && !argv.is_null()
}

fn arg_node(argc: c_int, argv: *mut JSValue) -> Option<Option<Element>> {
    has_arg(argc, argv).then(|| this_node(unsafe { *argv }))
}

fn int(value: i32) -> JSValue {
    quickjs::into_raw(Value::int(value))
}

fn boolean(value: bool) -> JSValue {
    quickjs::into_raw(Value::boolean(value))
}

macro_rules! node_getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                unsafe { ns_make_element(ctx, Node::ptr_or_null($f(this_node(this_val)))) }
            }
        )*
    };
}

node_getters! {
    ns_element_get_parentElement => crate::tree::parent_element,
    ns_element_get_parentNode => crate::tree::parent_node,
    ns_element_get_firstChild => crate::tree::first_child,
    ns_element_get_lastChild => crate::tree::last_child,
    ns_element_get_nextSibling => crate::tree::next_sibling,
    ns_element_get_previousSibling => crate::tree::previous_sibling,
    ns_element_get_firstElementChild => crate::tree::first_element_child,
    ns_element_get_lastElementChild => crate::tree::last_element_child,
    ns_element_get_nextElementSibling => crate::tree::next_element_sibling,
    ns_element_get_previousElementSibling => crate::tree::previous_element_sibling,
}

macro_rules! text_getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                let text = $f(this_node(this_val));
                unsafe {
                    quickjs::with_context(ctx, |scope| {
                        quickjs::into_raw(crate::names::to_value(scope, text))
                    })
                }
            }
        )*
    };
}

text_getters! {
    ns_element_get_nodeName => crate::names::node_name,
    ns_element_get_localName => crate::names::local_name_of,
    ns_element_get_prefix => crate::names::prefix_of,
    ns_element_get_namespaceURI => crate::names::namespace_uri_of,
}

macro_rules! scope_getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                unsafe {
                    quickjs::with_context(ctx, |scope| {
                        let this = quickjs::borrow_value(scope, this_val);
                        let result = $f(scope, &this);
                        quickjs::result_raw(scope, result)
                    })
                }
            }
        )*
    };
}

scope_getters! {
    ns_element_get_tagName => crate::owner::tag_name,
    ns_element_get_ownerDocument => crate::owner::owner_document,
    ns_element_get_baseURI => crate::owner::base_uri,
    ns_element_template_content => crate::owner::template_content,
    ns_element_get_children => crate::owner::children,
    ns_element_get_childNodes => crate::owner::child_nodes,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_nodeType(
    _ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    int(crate::tree::node_type(this_node(this_val)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_childElementCount(
    _ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    int(crate::tree::child_element_count(this_node(this_val)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_isConnected(
    _ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    boolean(crate::tree::is_connected(this_node(this_val)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_contains(
    _ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    let other = arg_node(argc, argv).flatten();
    boolean(crate::tree::contains(this_node(this_val), other))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_hasChildNodes(
    _ctx: *mut JSContext,
    this_val: JSValue,
    _argc: c_int,
    _argv: *mut JSValue,
) -> JSValue {
    boolean(crate::tree::has_child_nodes(this_node(this_val)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_isEqualNode(
    _ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    let equal = match (this_node(this_val), arg_node(argc, argv).flatten()) {
        (Some(a), Some(b)) => crate::tree::equal(a, b, 0),
        _ => false,
    };
    boolean(equal)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_compareDocumentPosition(
    _ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    let position = match (this_node(this_val), arg_node(argc, argv)) {
        (Some(a), Some(b)) => crate::tree::compare_document_position(a, b),
        _ => 0,
    };
    int(position)
}
