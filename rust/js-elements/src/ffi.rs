//! Southstar — the element-specific bindings' nsr_ entry points, and the js.c and binding calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_image::ImageRef;
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, JsResult};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

const LIVE_LABELS: c_int = 13;

unsafe extern "C" {
    fn ns_label_associated_control(label: *const NsNode) -> *const NsNode;
    fn ns_js_node_is_labelable(n: *const NsNode) -> GBoolean;
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
}

unsafe extern "Rust" {
    safe fn nsr_make_live(
        scope: &mut Scope<'_>,
        owner: &Value,
        kind: c_int,
        param: Option<&[u8]>,
        param2: Option<&[u8]>,
    ) -> Value;
    safe fn nsr_form_controls_snapshot(scope: &mut Scope<'_>, form: &Value) -> JsResult;
    safe fn nsr_element_reflect_str_set(
        scope: &mut Scope<'_>,
        this: &Value,
        value: &Value,
        attr: &CStr,
    ) -> JsResult<()>;
    safe fn nsr_element_int_attr_getter(
        scope: &mut Scope<'_>,
        this: &Value,
        magic: i32,
    ) -> JsResult;
    safe fn nsr_element_int_attr_setter(
        scope: &mut Scope<'_>,
        this: &Value,
        value: &Value,
        magic: i32,
    ) -> JsResult<()>;
}

fn raw_js(js: Option<Js>) -> *mut NsJs {
    js.map_or(ptr::null_mut(), |js| js.0)
}

fn owned_text(text: *mut c_char) -> Option<Vec<u8>> {
    if text.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(text) }.to_bytes().to_vec();
    unsafe { glib::g_free(text.cast()) };
    Some(bytes)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Option<Js> {
    let js: *mut NsJs = scope.context_opaque().cast();
    (!js.is_null()).then_some(Js(js))
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn make_node(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    api::make_element(scope, Node::ptr_or_null(node)).unwrap_or_else(|_| Value::null())
}

pub(crate) fn make_labels(scope: &mut Scope<'_>, owner: &Value) -> JsResult {
    Ok(nsr_make_live(scope, owner, LIVE_LABELS, None, None))
}

pub(crate) fn mark_mutated(js: Js) {
    api::mark_mutated(js.0);
}

pub(crate) fn orphan(js: Option<Js>, node: Element) {
    southstar_js_mutation::orphan_node(raw_js(js), node);
}

pub(crate) fn qcache_invalidate(js: Js) {
    southstar_js_collections::qcache_invalidate(js.0);
}

pub(crate) fn record_child_change(
    js: Js,
    parent: Option<Element>,
    added: Option<Element>,
    removed: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    match parent {
        Some(parent) => {
            southstar_js_mutation::record_child_change(js.0, parent, added, removed, previous, next)
        }
        None => qcache_invalidate(js),
    }
}

pub(crate) fn insert_before_single(js: Option<Js>, node: Element, reference: Element) {
    api::insert_before_single(raw_js(js), node, reference);
}

pub(crate) fn set_attr_recorded(js: Option<Js>, node: Element, name: &CStr, value: &CStr) {
    southstar_js_mutation::set_attr_recorded(raw_js(js), node, name, value.to_bytes());
}

pub(crate) fn set_attr_recorded_bytes(js: Option<Js>, node: Element, name: &CStr, value: &[u8]) {
    southstar_js_mutation::set_attr_recorded(raw_js(js), node, name, value);
}

pub(crate) fn remove_attr_recorded(js: Option<Js>, node: Element, name: &CStr) {
    southstar_js_mutation::remove_attr_recorded(raw_js(js), node, name);
}

pub(crate) fn form_controls_snapshot(scope: &mut Scope<'_>, form: &Value) -> JsResult {
    nsr_form_controls_snapshot(scope, form)
}

pub(crate) fn replace_text(node: Element, text: &[u8]) {
    node.set_text(text);
}

pub(crate) fn record_character_data(js: Js, node: Element, old: &[u8]) {
    southstar_js_mutation::record_character_data(js.0, node, old);
}

pub(crate) fn schedule_iframe_load(js: Option<Js>, node: Element) {
    southstar_js_frames::schedule_iframe_load(raw_js(js), node, false);
}

pub(crate) fn resolve_against_document_base(js: Option<Js>, href: &CStr) -> Option<Vec<u8>> {
    let base = owned_text(unsafe { ns_js_doc_base_url(raw_js(js)) })?;
    if base.is_empty() {
        return None;
    }
    southstar_url::bytes::resolve(Some(&base), href.to_bytes())
}

pub(crate) fn is_shadow_root(node: Element) -> bool {
    southstar_js_shadow::is_shadow_root(node)
}

pub(crate) fn has_shadow_child(host: Element) -> bool {
    southstar_js_shadow::shadow_child(host).is_some()
}

pub(crate) fn has_assigned_slot(node: Element) -> bool {
    southstar_js_shadow::assigned_slot_node(node).is_some()
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    api::current_document(js.0)
}

pub(crate) fn image_for_node(js: Option<Js>, node: Element) -> Option<ImageRef<'static>> {
    southstar_js_images::image_for_node(raw_js(js), node)
}

pub(crate) use southstar_layout::{
    img_chosen_density as chosen_density, img_chosen_url as chosen_url,
};

pub(crate) fn resolve_against_doc_base(
    js: Option<Js>,
    node: Element,
    href: &[u8],
) -> Option<Vec<u8>> {
    let js = js?;
    let base = api::node_doc_base(js.0, Some(node))?;
    let document_url = base.to_bytes();
    let base = southstar_dom::base_url::owning_document(node)
        .and_then(|doc| southstar_dom::base_url::document_base(doc, Some(document_url)))
        .unwrap_or_else(|| document_url.to_vec());
    let href = href.split(|&b| b == 0).next().unwrap_or_default();
    southstar_url::bytes::resolve(Some(&base), href)
}

pub(crate) fn label_associated_control(label: Element) -> Option<Element> {
    unsafe { Node::from_ptr(ns_label_associated_control(label.as_ptr())) }
}

pub(crate) fn is_labelable(node: Option<Element>) -> bool {
    unsafe { ns_js_node_is_labelable(Node::ptr_or_null(node)) != 0 }
}

pub(crate) fn enum_normalize(attr: &CStr, value: Option<&CStr>) -> &'static CStr {
    southstar_js_reflect::normalize(attr.to_bytes(), value.map(CStr::to_bytes)).unwrap_or_default()
}

pub(crate) fn is_custom_element(node: Option<Element>) -> bool {
    node.is_some_and(southstar_js_reflect::is_custom_element)
}

pub(crate) fn reflect_str_set(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    attr: &CStr,
) -> JsResult {
    nsr_element_reflect_str_set(scope, this, value, attr).map(|()| Value::undefined())
}

pub(crate) fn int_attr_get(scope: &mut Scope<'_>, this: &Value, attr: &CStr) -> JsResult {
    nsr_element_int_attr_getter(
        scope,
        this,
        southstar_js_reflect::numeric::int_attr_index(attr.to_bytes()),
    )
}

pub(crate) fn int_attr_set(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    attr: &CStr,
) -> JsResult {
    let index = southstar_js_reflect::numeric::int_attr_index(attr.to_bytes());
    nsr_element_int_attr_setter(scope, this, value, index).map(|()| Value::undefined())
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_live_build_labels(scope: &mut Scope<'_>, node: *const NsNode) -> JsResult {
    crate::element::build_labels(scope, unsafe { Node::from_ptr(node) })
}

#[unsafe(no_mangle)]
pub fn nsr_js_element_is_rendered(scope: &mut Scope<'_>, value: &Value) -> bool {
    crate::node_value::is_rendered(scope, value)
}

#[unsafe(no_mangle)]
pub fn nsr_element_img_natural_width(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    crate::image::natural_width(scope, this)
}

#[unsafe(no_mangle)]
pub fn nsr_element_img_natural_height(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    crate::image::natural_height(scope, this)
}

#[unsafe(no_mangle)]
pub fn nsr_element_set_node_value(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
) -> JsResult<()> {
    crate::node_value::set_node_value(scope, this, value)
}
