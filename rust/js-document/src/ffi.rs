//! Southstar — the document bindings' calls into the page record, net, GLib and the other binding crates, and the entry points those call back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode, Tree};
use southstar_glib as glib;
use southstar_js_engine::{NativeFn, Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, JsResult};

mod imports;

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn key(self) -> usize {
        self.0 as usize
    }
}

unsafe extern "C" {
    fn ns_js_focused_node(js: *const NsJs) -> *const NsNode;
    fn ns_js_focused_doc(js: *const NsJs) -> *const NsNode;
    fn ns_js_clear_focused_node(js: *mut NsJs);
    fn ns_net_http_date(date: *const c_char) -> i64;
    fn g_date_time_new_now_local() -> *mut c_void;
    fn g_date_time_format(datetime: *mut c_void, format: *const c_char) -> *mut c_char;
    fn g_date_time_unref(datetime: *mut c_void);
}

fn node_of(raw: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(raw) }
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

fn borrowed_text(text: *const c_char) -> Option<Vec<u8>> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_bytes().to_vec())
}

fn take_glib_string(text: *mut c_char) -> Option<Vec<u8>> {
    let bytes = borrowed_text(text)?;
    unsafe { glib::g_free(text.cast()) };
    Some(bytes)
}

fn c_bytes(bytes: &[u8]) -> CString {
    CString::new(crate::until_nul(bytes)).unwrap_or_default()
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque().cast())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    api::make_element(scope, raw(node)).unwrap_or_else(|_| Value::null())
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    let message = c_bytes(message.as_bytes());
    imports::nsr_dom_exception(scope, name.as_bytes(), code, message.to_bytes())
}

pub(crate) fn is_realm_document(scope: &mut Scope<'_>, doc: &Value) -> bool {
    crate::realm::is_realm_document(scope, doc)
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    api::current_document(js.0)
}

pub(crate) fn current_url_c(js: Js) -> Option<CString> {
    Some(api::current_url(js.0))
}

pub(crate) fn node_doc_base(js: Js, node: Option<Element>) -> Option<CString> {
    api::node_doc_base(js.0, node)
}

pub(crate) fn url_resolve(base: &CStr, href: &CStr) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(Some(base.to_bytes()), href.to_bytes())
}

pub(crate) fn base_uri_allowed(js: Js, url: &[u8], document_url: Option<&CStr>) -> bool {
    if js.is_null() {
        return true;
    }
    let url = c_bytes(url);
    api::with_page_csp(js.0, |csp| {
        csp.is_none_or(|csp| csp.allows(southstar_csp::BASE_URI, &url, document_url, None, true))
    })
}

pub(crate) fn iframe_realm_window(scope: &mut Scope<'_>, element: &Value, frame: Element) -> Value {
    southstar_js_frames::iframe_realm_window(scope, element, frame)
}

fn glib_bytes(bytes: Option<Vec<u8>>) -> *mut c_char {
    bytes.map_or(ptr::null_mut(), |bytes| glib::strdup(&bytes))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char {
    glib_bytes(crate::base_url::page_base_url(Js(js)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_node_document_base_url(
    js: *mut NsJs,
    node: *const NsNode,
) -> *mut c_char {
    glib_bytes(crate::base_url::node_base_url(Js(js), node_of(node)))
}

pub(crate) fn current_url(js: Js) -> Option<Vec<u8>> {
    Some(api::current_url(js.0).into_bytes()).filter(|url| !url.is_empty())
}

pub(crate) fn cookie_value(js: Js) -> Option<Vec<u8>> {
    api::cookie_value(js.0).map(CString::into_bytes)
}

pub(crate) fn set_cookie_value(js: Js, value: &[u8]) {
    api::set_cookie_value(js.0, Some(&c_bytes(value)));
}

pub(crate) fn cookie_version(js: Js) -> u64 {
    api::cookie_version(js.0)
}

pub(crate) fn set_cookie_version(js: Js, version: u64) {
    api::set_cookie_version(js.0, version);
}

pub(crate) fn partition_key(js: Js) -> Option<Vec<u8>> {
    api::partition_key(js.0).map(CString::into_bytes)
}

pub(crate) fn frame_referrer(js: Js, frame: Element) -> Option<Vec<u8>> {
    southstar_js_frames::frame_referrer(js.0, frame)
}

pub(crate) fn ready_state(js: Js, doc: Option<Element>) -> i32 {
    if let Some(doc) = doc {
        let state = api::doc_ready_state(js.0, doc);
        if state >= 0 {
            return state;
        }
    }
    api::ready_state(js.0)
}

pub(crate) fn current_script(js: Js) -> Option<Element> {
    api::current_script(js.0)
}

pub(crate) fn focused_node(js: Js) -> Option<Element> {
    node_of(unsafe { ns_js_focused_node(js.0) })
}

pub(crate) fn focused_doc(js: Js) -> Option<Element> {
    node_of(unsafe { ns_js_focused_doc(js.0) })
}

pub(crate) fn clear_focused_node(js: Js) {
    unsafe { ns_js_clear_focused_node(js.0) };
}

pub(crate) fn unorphan_node(js: Js, node: Element) {
    southstar_js_mutation::unorphan_node(js.0, node);
}

pub(crate) fn orphan_node(js: Js, node: Element) {
    southstar_js_mutation::orphan_node(js.0, node);
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.0);
}

pub(crate) fn record_child_change(
    js: Js,
    parent: Element,
    added: Option<Element>,
    removed: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    southstar_js_mutation::record_child_change(js.0, parent, added, removed, previous, next);
}

pub(crate) fn iters_pre_remove(js: Js, node: Element) {
    southstar_js_traversal::iters_pre_remove(js.0, node);
}

pub(crate) fn ce_disconnect_subtree(js: Js, node: Element) {
    southstar_js_custom_elements::disconnect_subtree(js.0, node);
}

pub(crate) fn insert_before_single(js: Js, node: Element, reference: Element) {
    api::insert_before_single(js.0, node, reference);
}

pub(crate) fn replace_all_recorded(js: Js, node: Element, added: Option<Element>) {
    southstar_js_mutation::replace_all_recorded(js.0, node, added);
}

pub(crate) fn set_attr_recorded(js: Js, node: Element, name: &CStr, value: &[u8]) {
    southstar_js_mutation::set_attr_recorded(js.0, node, name, value);
}

pub(crate) fn append_child(parent: Element, child: Element) {
    parent.append(child);
}

pub(crate) fn remove_node(node: Element) {
    node.detach();
}

pub(crate) fn new_element(name: &[u8]) -> Element {
    southstar_dom::node::new_element(name)
}

pub(crate) fn new_text(text: &[u8]) -> Element {
    southstar_dom::node::new_text(southstar_dom::Kind::Text, text)
}

pub(crate) fn attr_bytes(node: Element, name: &CStr) -> Option<&'static [u8]> {
    let attr = southstar_dom::attrs::find(node, name)?;
    let value = attr.value_ptr();
    if value.is_null() {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(value.cast(), attr.value_len() as usize) })
}

pub(crate) fn cookies_for_js(url: &[u8]) -> Option<Vec<u8>> {
    southstar_net::cookies_for_js(url)
}

pub(crate) fn cookie_jar_version(url: &[u8]) -> Option<u64> {
    southstar_net::cookie_jar_version(url)
}

pub(crate) fn cookie_store_from_js(url: &[u8], cookie: &[u8]) {
    southstar_net::cookie_store_from_js(url, cookie);
}

pub(crate) fn http_date(date: &[u8]) -> i64 {
    let date = c_bytes(date);
    unsafe { ns_net_http_date(date.as_ptr()) }
}

pub(crate) fn local_now_formatted(format: &CStr) -> Option<Vec<u8>> {
    let now = unsafe { g_date_time_new_now_local() };
    if now.is_null() {
        return None;
    }
    let text = take_glib_string(unsafe { g_date_time_format(now, format.as_ptr()) });
    unsafe { g_date_time_unref(now) };
    Some(text.unwrap_or_default())
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
    nsr_document_open => crate::write::open,
    nsr_document_close => crate::write::close,
    nsr_document_write => crate::write::write,
    nsr_document_writeln => crate::write::writeln,
    nsr_document_ctor => crate::realm::document_ctor,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_seed_cookies_from_jar(js: *mut NsJs) {
    if !js.is_null() {
        crate::cookie::seed_from_jar(Js(js));
    }
}

unsafe extern "C" {}

pub(crate) const REALM_DOCUMENT_METHODS: [(&str, u32, NativeFn); 13] = [
    ("createElement", 1, imports::nsr_document_createElement),
    ("createElementNS", 2, imports::nsr_document_createElementNS),
    ("createTextNode", 1, imports::nsr_document_createTextNode),
    ("createComment", 1, imports::nsr_document_createComment),
    (
        "createDocumentFragment",
        0,
        imports::nsr_document_createDocumentFragment,
    ),
    (
        "createCDATASection",
        1,
        imports::nsr_document_createCDATASection,
    ),
    (
        "createProcessingInstruction",
        2,
        imports::nsr_document_createProcessingInstruction,
    ),
    ("createAttribute", 1, imports::nsr_document_createAttribute),
    (
        "createAttributeNS",
        2,
        imports::nsr_document_createAttributeNS,
    ),
    ("createRange", 0, imports::nsr_document_create_range),
    ("createEvent", 1, imports::nsr_document_createEvent),
    ("importNode", 2, imports::nsr_document_import_node),
    ("adoptNode", 1, imports::nsr_document_adopt_node),
];

pub(crate) const REALM_QUERY_METHODS: [(&str, u32, NativeFn); 8] = [
    ("querySelector", 1, imports::nsr_element_querySelector),
    ("querySelectorAll", 1, imports::nsr_element_querySelectorAll),
    (
        "getElementsByTagName",
        1,
        imports::nsr_element_getElementsByTagName,
    ),
    (
        "getElementsByClassName",
        1,
        imports::nsr_element_getElementsByClassName,
    ),
    (
        "createTreeWalker",
        3,
        imports::nsr_document_create_tree_walker,
    ),
    (
        "createNodeIterator",
        3,
        imports::nsr_document_create_node_iterator,
    ),
    ("getSelection", 0, imports::nsr_window_get_selection),
    ("hasFocus", 0, imports::nsr_document_has_focus),
];

pub(crate) fn bind_c_function(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    arity: u32,
    f: NativeFn,
) {
    let function = scope.function(name, arity, f);
    let _ = scope.set(object, name, function);
}

pub(crate) fn with_main_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    api::main_context(js.0).with(f)
}

pub(crate) fn orphan_children(js: Js, node: Element) {
    southstar_js_mutation::orphan_children(js.0, node);
}

pub(crate) fn clear_children(js: Js, node: Element) {
    southstar_js_mutation::clear_children(js.0, node);
}

pub(crate) fn ignore_destructive_writes(js: Js) -> bool {
    api::ignore_destructive_writes(js.0) > 0
}

pub(crate) fn index_child_change(js: Js, parent: Element, added: Element) {
    southstar_js_mutation::index_child_change(js.0, parent, Some(added), None);
}

pub(crate) fn pending_iframe_add(js: Js, frame: Element) {
    southstar_js_frames::add_pending_iframe(js.0, frame);
}

pub(crate) fn run_inserted_scripts(js: Js, root: Element) {
    southstar_js_loader::run_inserted_scripts(js.0, root);
}

pub(crate) fn ce_upgrade_subtree_all(js: Js, root: Element) {
    southstar_js_custom_elements::upgrade_subtree_all(js.0, root);
}

pub(crate) fn expose_legacy_named(scope: &mut Scope<'_>, root: Element, document: &Value) {
    unsafe { imports::nsr_document_expose_legacy_named(scope, root.as_ptr(), document) };
}

pub(crate) fn arm_js_invalidate(node: Element) {
    api::arm_js_invalidate(node);
}

pub(crate) fn new_document() -> Option<Element> {
    Some(Node::alloc(southstar_dom::Kind::Document))
}

pub(crate) fn set_attr(node: Element, name: &CStr, value: &CStr) {
    southstar_dom::attrs::set_attribute(node, name, Some(value.to_bytes()));
}

pub(crate) fn parse_html(markup: &[u8]) -> Tree {
    southstar_html_parser::parse_document(markup, true)
}

pub(crate) fn parse_fragment(context: Option<&CStr>, markup: &[u8], scripting: bool) -> Tree {
    let context = context.unwrap_or(c"");
    southstar_html_parser::parse_fragment_for_tag(context, markup, scripting)
}

pub(crate) fn url_host(url: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::host_from(crate::until_nul(url))
}

const LIVE_DOC_TAG: c_int = 3;

pub(crate) fn live_doc_tag(scope: &mut Scope<'_>, owner: &Value, tag: &CStr) -> Value {
    imports::nsr_make_live(scope, owner, LIVE_DOC_TAG, Some(tag.to_bytes()), None)
}

pub(crate) fn implementation(scope: &mut Scope<'_>, this: &Value) -> Value {
    imports::nsr_document_implementation(scope, this).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn install_document_funcs(scope: &mut Scope<'_>, doc: &Value) {
    imports::nsr_document_install_funcs(scope, doc);
}

pub(crate) fn window_open(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    imports::nsr_window_open_method(scope, this, args)
}

fn text_arg(text: Option<&[u8]>) -> Option<&[u8]> {
    text.filter(|t| !t.is_empty())
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_make_realm_document(
    scope: &mut Scope<'_>,
    doc_node: *mut NsNode,
    url: Option<&[u8]>,
    charset: Option<&[u8]>,
    content_type: Option<&[u8]>,
    is_xml: bool,
    inert: bool,
) -> Value {
    let Some(doc) = node_of(doc_node) else {
        return Value::null();
    };
    let options = crate::realm::RealmDocument {
        url: text_arg(url).unwrap_or(b"about:blank"),
        charset: text_arg(charset).unwrap_or(b"UTF-8"),
        content_type: text_arg(content_type),
        is_xml,
        inert,
    };
    crate::realm::make(scope, doc, &options)
}

#[unsafe(no_mangle)]
pub fn nsr_make_synth_xml_document(scope: &mut Scope<'_>) -> Value {
    crate::realm::synth_xml(scope)
}

#[unsafe(no_mangle)]
pub fn nsr_realmdoc_deny_cookie(scope: &mut Scope<'_>, doc: &Value) {
    crate::realm::deny_cookie(scope, doc);
}

#[unsafe(no_mangle)]
pub fn nsr_document_lift_methods_to_proto(scope: &mut Scope<'_>, document: &Value) {
    crate::realm::lift_methods_to_proto(scope, document);
}

#[unsafe(no_mangle)]
pub fn nsr_document_define_doctype_getter(scope: &mut Scope<'_>, document: &Value) {
    crate::realm::define_getter(scope, document, "doctype", crate::realm::get_doctype);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_flush_document_write(js: *mut NsJs) {
    if !js.is_null() {
        crate::write::flush(Js(js));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_write_teardown(js: *mut NsJs) {
    crate::write::teardown(Js(js));
}
