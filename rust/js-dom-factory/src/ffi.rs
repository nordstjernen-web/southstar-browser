//! Southstar — the document factories' calls into the page record, the parsers and the other binding crates, and the entry points those call back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod imports;

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsAttr, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{NativeFn, Scope, Value};
use southstar_js_runtime::api;

use crate::names::{self, NameError};
use crate::{
    Element, JsResult, adopt, constructors, factories, hyperlink, implementation, namespaces,
};

pub(crate) use southstar_js_runtime::api::NsJs;

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

pub(crate) enum TargetMethod {
    AddEventListener,
    RemoveEventListener,
    DispatchEvent,
}

unsafe extern "C" {
    fn ns_xml_parse_reporting(
        input: *const c_char,
        len: isize,
        error_line: *mut c_int,
        error_column: *mut c_int,
    ) -> *mut NsNode;
    fn ns_xml_parse(input: *const c_char, len: isize) -> *mut NsNode;
    fn ns_html_parse_with_scripting(
        input: *const c_char,
        len: isize,
        scripting: GBoolean,
    ) -> *mut NsNode;
    fn ns_html_parse_fragment_with_scripting(
        context_tag: *const c_char,
        input: *const c_char,
        len: isize,
        scripting: GBoolean,
    ) -> *mut NsNode;
    fn ns_node_free(node: *mut NsNode);
    fn ns_mark_scripts_already_started(root: *mut NsNode);
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
}

fn raw_js(js: Option<Js>) -> *mut NsJs {
    js.map_or(ptr::null_mut(), |js| js.0)
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    let js: *mut NsJs = scope.context_opaque().cast();
    (!js.is_null()).then_some(Js(js))
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap(scope: &mut Scope<'_>, node: Element) -> Value {
    api::make_element(scope, node.as_ptr()).unwrap_or_else(|_| Value::null())
}

pub(crate) fn dom_exception(
    scope: &mut Scope<'_>,
    name: &CStr,
    code: i32,
    message: &CStr,
) -> Value {
    imports::nsr_dom_exception(scope, name.to_bytes(), code, message.to_bytes())
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
    let attr: *const RawAttr = &attr;
    unsafe { imports::nsr_attr_to_js(scope, &Value::null(), attr.cast::<NsAttr>(), true) }
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
    unsafe {
        imports::nsr_make_realm_document(
            scope,
            doc.as_mut_ptr(),
            info.url.map(c_prefix),
            info.charset.map(c_prefix),
            info.content_type.map(c_prefix),
            info.is_xml,
            true,
        )
    }
}

pub(crate) fn synth_xml_document(scope: &mut Scope<'_>) -> Value {
    imports::nsr_make_synth_xml_document(scope)
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
    imports::nsr_element_reflect_str_get(scope, this, c"href", false)
}

pub(crate) fn reflect_href_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    imports::nsr_element_reflect_str_set(scope, this, val, c"href")
}

pub(crate) fn tag_caller_document(scope: &mut Scope<'_>, node: &Value) {
    api::tag_caller_document(scope, node);
}

pub(crate) fn new_event(scope: &mut Scope<'_>) -> Value {
    imports::nsr_event_new(scope).unwrap_or_else(|_| Value::null())
}

pub(crate) fn target_method(
    scope: &mut Scope<'_>,
    name: &str,
    arity: u32,
    method: TargetMethod,
) -> Value {
    let f: NativeFn = match method {
        TargetMethod::AddEventListener => imports::nsr_target_addEventListener,
        TargetMethod::RemoveEventListener => imports::nsr_target_removeEventListener,
        TargetMethod::DispatchEvent => imports::nsr_target_dispatchEvent,
    };
    scope.function(name, arity, f)
}

pub(crate) fn parse_xml_reporting(source: &[u8]) -> (Option<Element>, c_int, c_int) {
    let source = cstring(source);
    let (mut line, mut column) = (1, 1);
    let doc = unsafe { ns_xml_parse_reporting(source.as_ptr(), -1, &mut line, &mut column) };
    (unsafe { Node::from_ptr(doc) }, line, column)
}

pub(crate) fn parse_xml(source: &[u8]) -> Option<Element> {
    let source = cstring(source);
    unsafe { Node::from_ptr(ns_xml_parse(source.as_ptr(), -1)) }
}

pub(crate) fn parse_html(source: &[u8]) -> Option<Element> {
    let source = cstring(source);
    unsafe {
        Node::from_ptr(ns_html_parse_with_scripting(
            source.as_ptr(),
            -1,
            glib::FALSE,
        ))
    }
}

pub(crate) fn parse_html_fragment(source: &[u8]) -> Option<Element> {
    let source = cstring(source);
    unsafe {
        Node::from_ptr(ns_html_parse_fragment_with_scripting(
            ptr::null(),
            source.as_ptr(),
            -1,
            glib::FALSE,
        ))
    }
}

pub(crate) fn free_node(node: Element) {
    unsafe { ns_node_free(node.as_mut_ptr()) };
}

pub(crate) fn mark_scripts_already_started(root: Element) {
    unsafe { ns_mark_scripts_already_started(root.as_mut_ptr()) };
}

macro_rules! rust_methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
                $f(scope, this, args)
            }
        )*
    };
}

rust_methods! {
    nsr_document_createElement => factories::create_element,
    nsr_document_createElementNS => factories::create_element_ns,
    nsr_document_createTextNode => factories::create_text_node,
    nsr_document_createComment => factories::create_comment,
    nsr_document_createCDATASection => factories::create_cdata_section,
    nsr_document_createProcessingInstruction => factories::create_processing_instruction,
    nsr_document_createAttribute => factories::create_attribute,
    nsr_document_createAttributeNS => factories::create_attribute_ns,
    nsr_document_createEvent => factories::create_event,
    nsr_document_createDocumentFragment => factories::create_document_fragment,
    nsr_document_import_node => adopt::import_node,
    nsr_document_adopt_node => adopt::adopt_node,
    nsr_element_cloneNode => adopt::clone_node,
    nsr_attr_cloneNode => adopt::clone_attr,
    nsr_element_lookupNamespaceURI => namespaces::lookup_namespace_uri,
    nsr_element_lookupPrefix => namespaces::lookup_prefix,
    nsr_element_isDefaultNamespace => namespaces::is_default_namespace,
    nsr_impl_create_html_document => implementation::create_html_document,
    nsr_impl_create_document => implementation::create_document,
    nsr_impl_create_document_type => implementation::create_document_type,
    nsr_window_document_fragment_ctor => constructors::document_fragment,
    nsr_window_text_ctor => constructors::text,
    nsr_window_comment_ctor => constructors::comment,
    nsr_window_image_ctor => constructors::image,
    nsr_window_audio_ctor => constructors::audio,
    nsr_window_option_ctor => constructors::option,
    nsr_window_dom_parser_ctor => constructors::dom_parser,
    nsr_dom_parser_parseFromString => constructors::parse_from_string,
    nsr_window_close_watcher_ctor => constructors::close_watcher,
}

#[unsafe(no_mangle)]
pub fn nsr_document_implementation(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    implementation::of_document(scope, this)
}

#[unsafe(no_mangle)]
pub fn nsr_element_anchor_part_get(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    hyperlink::part_get(scope, this, magic)
}

#[unsafe(no_mangle)]
pub fn nsr_element_anchor_href_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    _magic: i32,
) -> JsResult<()> {
    reflect_href_set(scope, this, val)
}

#[unsafe(no_mangle)]
pub fn nsr_element_url_part_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    magic: i32,
) -> JsResult<()> {
    hyperlink::part_set(scope, this, val, magic)
}

#[unsafe(no_mangle)]
pub fn nsr_doc_wrapper_is_xml(scope: &mut Scope<'_>, doc: &Value) -> bool {
    adopt::doc_is_xml(scope, doc)
}

#[unsafe(no_mangle)]
pub fn nsr_validate_attr_ns(
    scope: &mut Scope<'_>,
    namespace: Option<&[u8]>,
    qname: &[u8],
) -> JsResult<()> {
    names::validate_attr_ns(namespace, qname).map_err(|error| name_error(scope, error))
}

#[unsafe(no_mangle)]
pub fn nsr_tag_owner_document(scope: &mut Scope<'_>, document: &Value, node: &Value) {
    adopt::tag_owner_document(scope, document, node);
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
