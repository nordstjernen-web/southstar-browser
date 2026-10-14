//! Southstar — the element-specific bindings' nsr_ entry points, and the js.c and binding calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_image::ImageRef;
use southstar_js_engine::{Scope, Value};

use crate::{Element, JsResult};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

const LIVE_LABELS: c_int = 13;

unsafe extern "C" {
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_orphan_node(js: *mut NsJs, node: *mut NsNode);
    fn ns_qcache_invalidate(js: *const NsJs);
    fn ns_js_record_child_change(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut NsNode,
        removed: *mut NsNode,
        previous_sibling: *mut NsNode,
        next_sibling: *mut NsNode,
    );
    fn ns_element_insert_before_single(
        js: *mut NsJs,
        parent: *mut NsNode,
        node: *mut NsNode,
        reference: *mut NsNode,
    );
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        n: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_js_set_attr_recorded_len(
        js: *mut NsJs,
        n: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
        len: isize,
    );
    fn ns_js_remove_attr_recorded(js: *mut NsJs, n: *mut NsNode, name: *const c_char);
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_image_for_node(js: *mut NsJs, el: *const NsNode) -> *const c_void;
    fn ns_img_chosen_url(n: *const NsNode) -> *mut c_char;
    fn ns_img_chosen_density(n: *const NsNode) -> f64;
    fn ns_js_node_doc_base(js: *mut NsJs, el: *const NsNode) -> *const c_char;
    fn ns_label_associated_control(label: *const NsNode) -> *const NsNode;
    fn ns_js_node_is_labelable(n: *const NsNode) -> GBoolean;
    fn ns_enum_normalize(attr: *const c_char, v: *const c_char) -> *const c_char;
    fn ns_node_is_custom_element(n: *const NsNode) -> GBoolean;
    fn ns_int_attr_index(attr: *const c_char) -> c_int;
    fn ns_js_record_character_data(js: *mut NsJs, node: *mut NsNode, old_value: *const c_char);
    fn ns_js_schedule_iframe_load(js: *mut NsJs, iframe: *mut NsNode);
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_node_is_shadow_root(n: *const NsNode) -> GBoolean;
    fn ns_element_find_shadow_child(host: *const NsNode) -> *mut NsNode;
    fn ns_node_assigned_slot_node(n: *const NsNode) -> *mut NsNode;
}

unsafe extern "Rust" {
    safe fn nsr_make_element(scope: &mut Scope<'_>, node: *const NsNode) -> Result<Value, Value>;
    safe fn nsr_unwrap_element(value: &Value) -> *const NsNode;
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

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
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
    unsafe { Node::from_ptr(nsr_unwrap_element(value)) }
}

pub(crate) fn make_node(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    nsr_make_element(scope, Node::ptr_or_null(node)).unwrap_or_else(|_| Value::null())
}

pub(crate) fn make_labels(scope: &mut Scope<'_>, owner: &Value) -> JsResult {
    Ok(nsr_make_live(scope, owner, LIVE_LABELS, None, None))
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.0) };
}

pub(crate) fn orphan(js: Option<Js>, node: Element) {
    unsafe { ns_js_orphan_node(raw_js(js), node.as_mut_ptr()) };
}

pub(crate) fn qcache_invalidate(js: Js) {
    unsafe { ns_qcache_invalidate(js.0) };
}

pub(crate) fn record_child_change(
    js: Js,
    parent: Option<Element>,
    added: Option<Element>,
    removed: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    unsafe {
        ns_js_record_child_change(
            js.0,
            raw(parent),
            raw(added),
            raw(removed),
            raw(previous),
            raw(next),
        )
    };
}

pub(crate) fn insert_before_single(
    js: Option<Js>,
    parent: Element,
    node: Element,
    reference: Element,
) {
    unsafe {
        ns_element_insert_before_single(
            raw_js(js),
            parent.as_mut_ptr(),
            node.as_mut_ptr(),
            reference.as_mut_ptr(),
        )
    };
}

pub(crate) fn set_attr_recorded(js: Option<Js>, node: Element, name: &CStr, value: &CStr) {
    unsafe {
        ns_js_set_attr_recorded(raw_js(js), node.as_mut_ptr(), name.as_ptr(), value.as_ptr())
    };
}

pub(crate) fn set_attr_recorded_bytes(js: Option<Js>, node: Element, name: &CStr, value: &[u8]) {
    let mut terminated = Vec::with_capacity(value.len() + 1);
    terminated.extend_from_slice(value);
    terminated.push(0);
    unsafe {
        ns_js_set_attr_recorded_len(
            raw_js(js),
            node.as_mut_ptr(),
            name.as_ptr(),
            terminated.as_ptr().cast(),
            value.len() as isize,
        )
    };
}

pub(crate) fn remove_attr_recorded(js: Option<Js>, node: Element, name: &CStr) {
    unsafe { ns_js_remove_attr_recorded(raw_js(js), node.as_mut_ptr(), name.as_ptr()) };
}

pub(crate) fn form_controls_snapshot(scope: &mut Scope<'_>, form: &Value) -> JsResult {
    nsr_form_controls_snapshot(scope, form)
}

pub(crate) fn replace_text(node: Element, text: &[u8]) {
    node.set_text(glib::strdup(text), text.len() as u32, true);
}

pub(crate) fn record_character_data(js: Js, node: Element, old: &[u8]) {
    let mut old = old.to_vec();
    old.push(0);
    unsafe { ns_js_record_character_data(js.0, node.as_mut_ptr(), old.as_ptr().cast()) };
}

pub(crate) fn schedule_iframe_load(js: Option<Js>, node: Element) {
    unsafe { ns_js_schedule_iframe_load(raw_js(js), node.as_mut_ptr()) };
}

pub(crate) fn resolve_against_document_base(js: Option<Js>, href: &CStr) -> Option<Vec<u8>> {
    let base = owned_text(unsafe { ns_js_doc_base_url(raw_js(js)) })?;
    if base.is_empty() {
        return None;
    }
    southstar_url::bytes::resolve(Some(&base), href.to_bytes())
}

pub(crate) fn is_shadow_root(node: Element) -> bool {
    unsafe { ns_node_is_shadow_root(node.as_ptr()) != 0 }
}

pub(crate) fn has_shadow_child(host: Element) -> bool {
    !unsafe { ns_element_find_shadow_child(host.as_ptr()) }.is_null()
}

pub(crate) fn has_assigned_slot(node: Element) -> bool {
    !unsafe { ns_node_assigned_slot_node(node.as_ptr()) }.is_null()
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    unsafe { Node::from_ptr(ns_js_current_document(js.0)) }
}

pub(crate) fn image_for_node(js: Option<Js>, node: Element) -> Option<ImageRef<'static>> {
    unsafe { ImageRef::from_ptr(ns_js_image_for_node(raw_js(js), node.as_ptr())) }
}

pub(crate) fn chosen_url(node: Element) -> Option<Vec<u8>> {
    owned_text(unsafe { ns_img_chosen_url(node.as_ptr()) })
}

pub(crate) fn chosen_density(node: Option<Element>) -> f64 {
    unsafe { ns_img_chosen_density(Node::ptr_or_null(node)) }
}

pub(crate) fn resolve_against_doc_base(
    js: Option<Js>,
    node: Element,
    href: &[u8],
) -> Option<Vec<u8>> {
    let js = js?;
    let base = unsafe { ns_js_node_doc_base(js.0, node.as_ptr()) };
    if base.is_null() {
        return None;
    }
    let document_url = unsafe { CStr::from_ptr(base) }.to_bytes();
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
    let value = value.map_or(ptr::null(), CStr::as_ptr);
    let normalized = unsafe { ns_enum_normalize(attr.as_ptr(), value) };
    if normalized.is_null() {
        c""
    } else {
        unsafe { CStr::from_ptr(normalized) }
    }
}

pub(crate) fn is_custom_element(node: Option<Element>) -> bool {
    unsafe { ns_node_is_custom_element(Node::ptr_or_null(node)) != 0 }
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
    nsr_element_int_attr_getter(scope, this, unsafe { ns_int_attr_index(attr.as_ptr()) })
}

pub(crate) fn int_attr_set(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    attr: &CStr,
) -> JsResult {
    let index = unsafe { ns_int_attr_index(attr.as_ptr()) };
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

#[allow(non_snake_case)]
#[unsafe(no_mangle)]
pub fn nsr_element_set_nodeValue(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
) -> JsResult<()> {
    crate::node_value::set_node_value(scope, this, value)
}

#[unsafe(no_mangle)]
pub fn nsr_element_set_node_value(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
) -> JsResult<()> {
    crate::node_value::set_node_value(scope, this, value)
}
