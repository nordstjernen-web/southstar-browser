//! Southstar — the document factories' calls into the page record, the parsers and the other binding crates, and the entry points those call back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod imports;

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{AttrView, Node, NsNode, Tree};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{NativeFn, Scope, Value};
use southstar_js_runtime::api;

use crate::names::{self, NameError};
use crate::{Element, JsResult, adopt, constructors, factories, hyperlink, implementation};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

#[repr(C)]
pub(crate) enum TargetMethod {
    AddEventListener,
    RemoveEventListener,
    DispatchEvent,
}

unsafe extern "C" {
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
}

fn raw_js(js: Option<Js>) -> *mut NsJs {
    js.map_or(ptr::null_mut(), |js| js.0)
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
        southstar_js_mutation::track_orphan(js.0, node);
    }
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    api::current_document(js.0)
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.0);
}

pub(crate) fn record_removal(
    js: Js,
    parent: Element,
    node: Element,
    previous: Option<Element>,
    next: Option<Element>,
) {
    southstar_js_mutation::record_child_change(js.0, parent, None, Some(node), previous, next);
}

pub(crate) fn ce_upgrade_element(js: Option<Js>, node: Element) {
    southstar_js_custom_elements::upgrade_element(raw_js(js), node);
}

pub(crate) fn ce_upgrade_subtree_detached(js: Js, root: Element) {
    southstar_js_custom_elements::upgrade_subtree_detached(js.0, root);
}

pub(crate) fn in_template_content(node: Element) -> bool {
    api::in_template_content(node)
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
    southstar_dom::attrs::set_attribute(node, name, Some(value.to_bytes()));
}

pub(crate) fn attr(node: Element, name: &CStr) -> Option<&'static [u8]> {
    southstar_dom::attrs::find(node, name).and_then(southstar_dom::Attr::value_bytes)
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
    let attr = AttrView {
        name: &name,
        value: c"",
        namespace_uri: namespace_uri.as_deref(),
        prefix: prefix.as_deref(),
        local_name: Some(&local),
    };
    imports::nsr_attr_to_js(scope, &Value::null(), attr, true)
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
    southstar_url::bytes::resolve(base.map(c_prefix), href)
}

pub(crate) fn url_part(href: &[u8], magic: c_int) -> Option<Vec<u8>> {
    let p = southstar_url::bytes::parts(c_prefix(href))?;
    Some(match magic {
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
        _ => Vec::new(),
    })
}

pub(crate) fn set_url_component(href: &[u8], component: &CStr, value: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::set_component(c_prefix(href), component.to_bytes(), value)
}

pub(crate) fn set_attr_recorded(js: Option<Js>, node: Element, name: &CStr, value: &[u8]) {
    southstar_js_mutation::set_attr_recorded(raw_js(js), node, name, c_prefix(value));
}

pub(crate) fn reflect_href_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    imports::nsr_element_reflect_str_get(scope, this, c"href", false)
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

pub(crate) fn parse_xml_reporting(source: &[u8]) -> (Option<Tree>, c_int, c_int) {
    match southstar_xml::parse_document(c_prefix(source)) {
        Ok(doc) => (Some(doc), 1, 1),
        Err((line, column)) => (None, line, column),
    }
}

pub(crate) fn parse_xml(source: &[u8]) -> Option<Tree> {
    southstar_xml::parse_document(c_prefix(source)).ok()
}

pub(crate) fn parse_html(source: &[u8]) -> Tree {
    southstar_html_parser::parse_document(c_prefix(source), false)
}

pub(crate) fn parse_html_fragment(source: &[u8]) -> Tree {
    southstar_html_parser::parse_fragment_for_tag(c"", c_prefix(source), false)
}

pub(crate) fn mark_scripts_already_started(root: Element) {
    southstar_js_mutation::mark_scripts_already_started(root);
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
    nsr_attr_cloneNode => adopt::clone_attr,
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
