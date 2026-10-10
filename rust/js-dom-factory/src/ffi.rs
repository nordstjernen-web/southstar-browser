//! Southstar — the C ABI of the document factories, namespace lookups and hyperlink accessors as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::names::{self, NameError};
use crate::{Element, JsResult, adopt, factories, hyperlink, implementation, namespaces};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

#[repr(C)]
struct RawAttr {
    name: *const c_char,
    value: *const c_char,
    namespace_uri: *const c_char,
    prefix: *const c_char,
    local_name: *const c_char,
    next: *mut RawAttr,
    value_len: c_uint,
    flags: u8,
}

#[repr(C)]
struct UrlParts {
    href: *mut c_char,
    protocol: *mut c_char,
    origin: *mut c_char,
    host: *mut c_char,
    hostname: *mut c_char,
    port: *mut c_char,
    pathname: *mut c_char,
    search: *mut c_char,
    hash: *mut c_char,
    username: *mut c_char,
    password: *mut c_char,
}

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_record_child_change(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut NsNode,
        removed: *mut NsNode,
        previous_sibling: *mut NsNode,
        next_sibling: *mut NsNode,
    );
    fn ns_ce_upgrade_element(js: *mut NsJs, node: *mut NsNode);
    fn ns_ce_upgrade_subtree_detached(js: *mut NsJs, root: *mut NsNode);
    fn ns_node_in_template_content(node: *const NsNode) -> GBoolean;
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_element_get_attr_len(
        node: *const NsNode,
        name: *const c_char,
        len: *mut usize,
    ) -> *const c_char;
    fn ns_attr_to_js(
        ctx: *mut JSContext,
        owner: JSValue,
        attr: *const RawAttr,
        include_base: GBoolean,
    ) -> JSValue;
    fn ns_make_realm_document(
        ctx: *mut JSContext,
        doc: *mut NsNode,
        url: *const c_char,
        charset: *const c_char,
        content_type: *const c_char,
        is_xml: GBoolean,
        inert: GBoolean,
    ) -> JSValue;
    fn ns_make_synth_xml_document(ctx: *mut JSContext) -> JSValue;
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_url_resolve_len(base: *const c_char, href: *const c_char, len: usize) -> *mut c_char;
    fn ns_url_parts_new(url: *const c_char) -> *mut UrlParts;
    fn ns_url_parts_free(parts: *mut UrlParts);
    fn ns_url_set_component_len(
        href: *const c_char,
        component: *const c_char,
        value: *const c_char,
        len: usize,
    ) -> *mut c_char;
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_element_reflect_str_get(
        ctx: *mut JSContext,
        this_val: JSValue,
        attr: *const c_char,
        null_if_absent: GBoolean,
    ) -> JSValue;
    fn ns_element_reflect_str_set(
        ctx: *mut JSContext,
        this_val: JSValue,
        val: JSValue,
        attr: *const c_char,
    ) -> JSValue;
}

fn raw_js(js: Option<Js>) -> *mut NsJs {
    js.map_or(ptr::null_mut(), |js| js.0)
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    let js: *mut NsJs = quickjs::context_opaque(scope).cast();
    (!js.is_null()).then_some(Js(js))
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn wrap(scope: &mut Scope<'_>, node: Element) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), node.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: i32,
    message: &CStr,
) -> Value {
    unsafe {
        ns_throw_dom_exception(
            quickjs::raw_context(scope),
            name.as_ptr(),
            code,
            message.as_ptr(),
        )
    };
    quickjs::take_exception(scope)
}

pub(crate) fn name_error(scope: &mut Scope<'_>, error: NameError) -> Value {
    dom_exception(scope, error.name, error.code, error.message)
}

pub(crate) fn track_orphan(js: Option<Js>, node: Element) {
    if let Some(js) = js {
        unsafe { ns_js_track_orphan(js.0, node.as_mut_ptr()) };
    }
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    unsafe { Node::from_ptr(ns_js_current_document(js.0)) }
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.0) };
}

pub(crate) fn record_removal(
    js: Js,
    parent: Element,
    node: Element,
    previous: Option<Element>,
    next: Option<Element>,
) {
    unsafe {
        ns_js_record_child_change(
            js.0,
            parent.as_mut_ptr(),
            ptr::null_mut(),
            node.as_mut_ptr(),
            raw(previous),
            raw(next),
        )
    };
}

pub(crate) fn ce_upgrade_element(js: Option<Js>, node: Element) {
    unsafe { ns_ce_upgrade_element(raw_js(js), node.as_mut_ptr()) };
}

pub(crate) fn ce_upgrade_subtree_detached(js: Js, root: Element) {
    unsafe { ns_ce_upgrade_subtree_detached(js.0, root.as_mut_ptr()) };
}

pub(crate) fn in_template_content(node: Element) -> bool {
    unsafe { ns_node_in_template_content(node.as_ptr()) != glib::FALSE }
}

pub(crate) fn cstring(bytes: &[u8]) -> CString {
    CString::new(c_prefix(bytes)).unwrap_or_default()
}

pub(crate) fn c_prefix(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end])
}

pub(crate) fn set_attr(node: Element, name: &CStr, value: &[u8]) {
    let value = cstring(value);
    unsafe { ns_element_set_attr(node.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub(crate) fn attr(node: Element, name: &CStr) -> Option<&'static [u8]> {
    let mut len = 0usize;
    let v = unsafe { ns_element_get_attr_len(node.as_ptr(), name.as_ptr(), &mut len) };
    (!v.is_null()).then(|| unsafe { core::slice::from_raw_parts(v.cast::<u8>(), len) })
}

pub(crate) struct AttrName<'a> {
    pub namespace_uri: Option<&'a [u8]>,
    pub prefix: Option<&'a [u8]>,
    pub local_name: &'a [u8],
    pub name: &'a [u8],
}

pub(crate) fn attr_object(scope: &mut Scope<'_>, qname: AttrName<'_>) -> JsResult {
    let name = cstring(qname.name);
    let local = cstring(qname.local_name);
    let namespace_uri = qname.namespace_uri.map(cstring);
    let prefix = qname.prefix.map(cstring);
    let attr = RawAttr {
        name: name.as_ptr(),
        value: c"".as_ptr(),
        namespace_uri: namespace_uri.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
        prefix: prefix.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
        local_name: local.as_ptr(),
        next: ptr::null_mut(),
        value_len: 0,
        flags: 0,
    };
    let raw = unsafe {
        ns_attr_to_js(
            quickjs::raw_context(scope),
            quickjs::raw(&Value::null()),
            &attr,
            glib::TRUE,
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) struct RealmDocument<'a> {
    pub url: Option<&'a [u8]>,
    pub charset: Option<&'a [u8]>,
    pub content_type: Option<&'a [u8]>,
    pub is_xml: bool,
}

pub(crate) fn realm_document(
    scope: &mut Scope<'_>,
    doc: Element,
    info: RealmDocument<'_>,
) -> Value {
    let url = info.url.map(cstring);
    let charset = info.charset.map(cstring);
    let content_type = info.content_type.map(cstring);
    let p = |s: &Option<CString>| s.as_ref().map_or(ptr::null(), |s| s.as_ptr());
    let raw = unsafe {
        ns_make_realm_document(
            quickjs::raw_context(scope),
            doc.as_mut_ptr(),
            p(&url),
            p(&charset),
            p(&content_type),
            glib::boolean(info.is_xml),
            glib::TRUE,
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn synth_xml_document(scope: &mut Scope<'_>) -> Value {
    let raw = unsafe { ns_make_synth_xml_document(quickjs::raw_context(scope)) };
    unsafe { quickjs::take_value(scope, raw) }
}

fn take_gstring(p: *mut c_char) -> Option<Vec<u8>> {
    if p.is_null() {
        return None;
    }
    let out = unsafe { CStr::from_ptr(p) }.to_bytes().to_vec();
    unsafe { glib::g_free(p.cast::<c_void>()) };
    Some(out)
}

pub(crate) fn doc_base_url(js: Option<Js>) -> Option<Vec<u8>> {
    let js = js?;
    take_gstring(unsafe { ns_js_doc_base_url(js.0) })
}

pub(crate) fn resolve_url(base: Option<&[u8]>, href: &[u8]) -> Option<Vec<u8>> {
    let base = base.map(cstring);
    take_gstring(unsafe {
        ns_url_resolve_len(
            base.as_ref().map_or(ptr::null(), |b| b.as_ptr()),
            href.as_ptr().cast(),
            href.len(),
        )
    })
}

pub(crate) fn url_part(href: &[u8], magic: c_int) -> Option<Vec<u8>> {
    let href = cstring(href);
    let parts = unsafe { ns_url_parts_new(href.as_ptr()) };
    if parts.is_null() {
        return None;
    }
    let p = unsafe { &*parts };
    let field = match magic {
        hyperlink::PROTOCOL => p.protocol,
        hyperlink::HOST => p.host,
        hyperlink::HOSTNAME => p.hostname,
        hyperlink::PORT => p.port,
        hyperlink::PATHNAME => p.pathname,
        hyperlink::SEARCH => p.search,
        hyperlink::HASH => p.hash,
        hyperlink::ORIGIN => p.origin,
        hyperlink::USERNAME => p.username,
        hyperlink::PASSWORD => p.password,
        _ => ptr::null_mut(),
    };
    let out = if field.is_null() {
        Vec::new()
    } else {
        unsafe { CStr::from_ptr(field) }.to_bytes().to_vec()
    };
    unsafe { ns_url_parts_free(parts) };
    Some(out)
}

pub(crate) fn set_url_component(href: &[u8], component: &CStr, value: &[u8]) -> Option<Vec<u8>> {
    let href = cstring(href);
    take_gstring(unsafe {
        ns_url_set_component_len(
            href.as_ptr(),
            component.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
        )
    })
}

pub(crate) fn set_attr_recorded(js: Option<Js>, node: Element, name: &CStr, value: &[u8]) {
    let value = cstring(value);
    unsafe {
        ns_js_set_attr_recorded(raw_js(js), node.as_mut_ptr(), name.as_ptr(), value.as_ptr())
    };
}

pub(crate) fn reflect_href_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let raw = unsafe {
        ns_element_reflect_str_get(
            quickjs::raw_context(scope),
            quickjs::raw(this),
            c"href".as_ptr(),
            glib::FALSE,
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) fn reflect_href_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    let raw = unsafe {
        ns_element_reflect_str_set(
            quickjs::raw_context(scope),
            quickjs::raw(this),
            quickjs::raw(val),
            c"href".as_ptr(),
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value).map(|_| ())
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

macro_rules! methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                argc: c_int,
                argv: *mut JSValue,
            ) -> JSValue {
                unsafe { native(ctx, this_val, argc, argv, $f) }
            }
        )*
    };
}

methods! {
    ns_document_createElement => factories::create_element,
    ns_document_createElementNS => factories::create_element_ns,
    ns_document_createTextNode => factories::create_text_node,
    ns_document_createComment => factories::create_comment,
    ns_document_createCDATASection => factories::create_cdata_section,
    ns_document_createProcessingInstruction => factories::create_processing_instruction,
    ns_document_createAttribute => factories::create_attribute,
    ns_document_createAttributeNS => factories::create_attribute_ns,
    ns_document_createEvent => factories::create_event,
    ns_document_createDocumentFragment => factories::create_document_fragment,
    ns_document_import_node => adopt::import_node,
    ns_document_adopt_node => adopt::adopt_node,
    ns_element_cloneNode => adopt::clone_node,
    ns_attr_cloneNode => adopt::clone_attr,
    ns_element_lookupNamespaceURI => namespaces::lookup_namespace_uri,
    ns_element_lookupPrefix => namespaces::lookup_prefix,
    ns_element_isDefaultNamespace => namespaces::is_default_namespace,
    ns_impl_create_html_document => implementation::create_html_document,
    ns_impl_create_document => implementation::create_document,
    ns_impl_create_document_type => implementation::create_document_type,
}

unsafe fn with_this(
    ctx: *mut JSContext,
    this_val: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value) -> JsResult,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let result = f(scope, &this);
            quickjs::result_raw(scope, result)
        })
    }
}

unsafe fn with_this_val(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value, &Value) -> JsResult<()>,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let val = quickjs::borrow_value(scope, val);
            let result = f(scope, &this, &val).map(|()| Value::undefined());
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_implementation(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { with_this(ctx, this_val, implementation::of_document) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_anchor_part_get(
    ctx: *mut JSContext,
    this_val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        with_this(ctx, this_val, |scope, this| {
            hyperlink::part_get(scope, this, magic)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_anchor_href_set(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    _magic: c_int,
) -> JSValue {
    unsafe { with_this_val(ctx, this_val, val, reflect_href_set) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_url_part_set(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        with_this_val(ctx, this_val, val, |scope, this, val| {
            hyperlink::part_set(scope, this, val, magic)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_anchor_resolved_href(
    node: *const NsNode,
    js: *mut NsJs,
) -> *mut c_char {
    let js = (!js.is_null()).then_some(Js(js));
    unsafe { Node::from_ptr(node) }
        .and_then(|node| hyperlink::resolved_href(node, js))
        .map_or(ptr::null_mut(), |href| glib::strdup(&href))
}

fn c_bytes<'a>(p: *const c_char) -> Option<&'a [u8]> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_valid_element_local_name(s: *const c_char) -> GBoolean {
    glib::boolean(c_bytes(s).is_some_and(names::valid_element_local_name))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_valid_attr_name(s: *const c_char) -> GBoolean {
    glib::boolean(c_bytes(s).is_some_and(names::valid_attr_name))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_validate_attr_ns(
    ctx: *mut JSContext,
    ns_uri: *const c_char,
    qname: *const c_char,
) -> JSValue {
    match names::validate_attr_ns(c_bytes(ns_uri), c_bytes(qname).unwrap_or_default()) {
        Ok(()) => quickjs::UNDEFINED,
        Err(error) => unsafe {
            ns_throw_dom_exception(ctx, error.name.as_ptr(), error.code, error.message.as_ptr())
        },
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_wrapper_is_xml(ctx: *mut JSContext, doc_val: JSValue) -> GBoolean {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let doc = quickjs::borrow_value(scope, doc_val);
            glib::boolean(adopt::doc_is_xml(scope, &doc))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tag_owner_document(
    ctx: *mut JSContext,
    doc_val: JSValue,
    node_val: JSValue,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let doc = quickjs::borrow_value(scope, doc_val);
            let node = quickjs::borrow_value(scope, node_val);
            adopt::tag_owner_document(scope, &doc, &node);
        })
    }
}
