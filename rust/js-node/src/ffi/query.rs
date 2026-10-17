//! Southstar — the Node getters, comparisons and attribute methods' entry points, and the page calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use super::{Js, NsJs, imports, raw_js};
use crate::{Element, JsResult};

unsafe extern "C" {
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_valid_attr_name(s: *const c_char) -> GBoolean;
    fn ns_input_resanitize_value(el: *mut NsNode);
    fn ns_js_details_toggle_open(js: *mut NsJs, details: *mut NsNode, open: GBoolean);
    fn ns_js_schedule_iframe_load_full(js: *mut NsJs, iframe: *mut NsNode, force: GBoolean);
    fn ns_js_schedule_iframe_load(js: *mut NsJs, iframe: *mut NsNode);
}

fn c_ptr(text: Option<&CStr>) -> *const c_char {
    text.map_or(ptr::null(), CStr::as_ptr)
}

pub(crate) fn make_node(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    api::make_element(scope, Node::ptr_or_null(node)).unwrap_or_else(|_| Value::null())
}

pub(crate) fn attr_owner(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(imports::nsr_attr_owner(value)) }
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    api::current_document(js.0)
}

pub(crate) fn doc_is_xml(scope: &mut Scope<'_>, doc: &Value) -> bool {
    imports::nsr_doc_wrapper_is_xml(scope, doc)
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

pub(crate) fn reflect_str_get(scope: &mut Scope<'_>, this: &Value, attr: &CStr) -> JsResult {
    imports::nsr_element_reflect_str_get(scope, this, attr, false)
}

pub(crate) fn make_live(scope: &mut Scope<'_>, owner: &Value, kind: c_int) -> JsResult {
    Ok(imports::nsr_make_live(scope, owner, kind, None, None))
}

pub(crate) fn valid_attr_name(name: &CStr) -> bool {
    unsafe { ns_valid_attr_name(name.as_ptr()) != 0 }
}

pub(crate) fn validate_attr_ns(
    scope: &mut Scope<'_>,
    namespace: Option<&CStr>,
    name: &CStr,
) -> JsResult<()> {
    imports::nsr_validate_attr_ns(scope, namespace.map(CStr::to_bytes), name.to_bytes())
}

pub(crate) fn set_attr_recorded(js: Option<Js>, node: Element, name: &CStr, value: &CStr) {
    southstar_js_mutation::set_attr_recorded(raw_js(js), node, name, value.to_bytes());
}

pub(crate) fn remove_attr_recorded(js: Option<Js>, node: Element, name: &CStr) {
    southstar_js_mutation::remove_attr_recorded(raw_js(js), node, name);
}

pub(crate) use southstar_js_mutation::QualifiedName;

pub(crate) fn set_attr_ns_recorded(
    js: Option<Js>,
    node: Element,
    qname: QualifiedName<'_>,
    value: &CStr,
) {
    southstar_js_mutation::set_attr_ns_recorded(raw_js(js), node, &qname, value);
}

pub(crate) fn remove_attr_ns_recorded(
    js: Option<Js>,
    node: Element,
    namespace: Option<&CStr>,
    local: &CStr,
) {
    southstar_js_mutation::remove_attr_ns_recorded(raw_js(js), node, namespace, local);
}

pub(crate) fn input_resanitize_value(node: Element) {
    unsafe { ns_input_resanitize_value(node.as_mut_ptr()) };
}

pub(crate) fn details_toggle_open(js: Option<Js>, node: Element, open: bool) {
    unsafe { ns_js_details_toggle_open(raw_js(js), node.as_mut_ptr(), glib::boolean(open)) };
}

pub(crate) fn start_image_load(js: Option<Js>, node: Element, src: &CStr) {
    southstar_js_images::start_image_load(raw_js(js), node, Some(src));
}

pub(crate) fn schedule_iframe_load_full(js: Option<Js>, node: Element) {
    unsafe { ns_js_schedule_iframe_load_full(raw_js(js), node.as_mut_ptr(), glib::TRUE) };
}

pub(crate) fn schedule_iframe_load(js: Option<Js>, node: Element) {
    unsafe { ns_js_schedule_iframe_load(raw_js(js), node.as_mut_ptr()) };
}

pub(crate) fn img_src_layout_neutral(node: Element) -> bool {
    api::img_src_layout_neutral(node)
}

pub(crate) fn body_forward_content_handler(
    scope: &mut Scope<'_>,
    node: Element,
    name: &CStr,
    code: Option<&CStr>,
) {
    let code = code.map(CStr::to_string_lossy);
    southstar_js_dispatch::forward_body_content_handler(
        scope,
        node,
        &name.to_string_lossy(),
        code.as_deref(),
    );
}

pub(crate) fn attr_may_affect_style(node: Element, name: &CStr) -> bool {
    southstar_css::attr_may_affect_style(node, Some(name.to_bytes()))
}

pub(crate) fn request_repaint(js: Js) {
    api::request_repaint(js.0);
}

pub(crate) fn record_attr_change(js: Js, node: Element, name: &CStr, old: Option<&CStr>) {
    southstar_js_mutation::record_attr_change(js.0, node, name, old);
}

pub(crate) fn ce_attr_changed(
    js: Js,
    node: Element,
    name: &CStr,
    old: Option<&CStr>,
    new: Option<&CStr>,
) {
    api::ce_attr_changed(js.0, node, name, old, new);
}

pub(crate) fn with_text<R>(
    scope: &mut Scope<'_>,
    value: &Value,
    f: impl FnOnce(&mut Scope<'_>, &[u8], &CStr) -> R,
) -> JsResult<R> {
    let bytes = scope.to_bytes(value)?;
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let text = CString::new(&bytes[..end]).unwrap_or_default();
    Ok(f(scope, &bytes, &text))
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn nsr_element_isSameNode(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    crate::attributes::is_same_node(scope, this, args)
}
