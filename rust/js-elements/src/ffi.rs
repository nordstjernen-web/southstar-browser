//! Southstar — the C ABI of the element-specific bindings as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_image::ImageRef;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::{Element, JsResult};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

const LIVE_LABELS: c_int = 13;

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_make_live(
        ctx: *mut JSContext,
        owner: JSValue,
        kind: c_int,
        param: *const c_char,
    ) -> JSValue;
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
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_label_associated_control(label: *const NsNode) -> *const NsNode;
    fn ns_js_node_is_labelable(n: *const NsNode) -> GBoolean;
    fn ns_enum_normalize(attr: *const c_char, v: *const c_char) -> *const c_char;
    fn ns_node_is_custom_element(n: *const NsNode) -> GBoolean;
    fn ns_element_reflect_str_set(
        ctx: *mut JSContext,
        this_val: JSValue,
        val: JSValue,
        attr: *const c_char,
    ) -> JSValue;
    fn ns_element_int_attr_getter(ctx: *mut JSContext, this_val: JSValue, magic: c_int) -> JSValue;
    fn ns_element_int_attr_setter(
        ctx: *mut JSContext,
        this_val: JSValue,
        val: JSValue,
        magic: c_int,
    ) -> JSValue;
    fn ns_int_attr_index(attr: *const c_char) -> c_int;
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
    let js: *mut NsJs = quickjs::context_opaque(scope).cast();
    (!js.is_null()).then_some(Js(js))
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn make_node(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), Node::ptr_or_null(node)) };
    unsafe { quickjs::take_value(scope, raw) }
}

fn checked_raw(scope: &mut Scope<'_>, raw: JSValue) -> JsResult {
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) fn make_labels(scope: &mut Scope<'_>, owner: &Value) -> JsResult {
    let raw = unsafe {
        ns_make_live(
            quickjs::raw_context(scope),
            quickjs::raw(owner),
            LIVE_LABELS,
            ptr::null(),
        )
    };
    checked_raw(scope, raw)
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
    let mut href = href.to_vec();
    href.push(0);
    owned_text(unsafe { ns_url_resolve(base, href.as_ptr().cast()) })
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
    let raw = unsafe {
        ns_element_reflect_str_set(
            quickjs::raw_context(scope),
            quickjs::raw(this),
            quickjs::raw(value),
            attr.as_ptr(),
        )
    };
    checked_raw(scope, raw)
}

pub(crate) fn int_attr_get(scope: &mut Scope<'_>, this: &Value, attr: &CStr) -> JsResult {
    let raw = unsafe {
        ns_element_int_attr_getter(
            quickjs::raw_context(scope),
            quickjs::raw(this),
            ns_int_attr_index(attr.as_ptr()),
        )
    };
    checked_raw(scope, raw)
}

pub(crate) fn int_attr_set(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    attr: &CStr,
) -> JsResult {
    let raw = unsafe {
        ns_element_int_attr_setter(
            quickjs::raw_context(scope),
            quickjs::raw(this),
            quickjs::raw(value),
            ns_int_attr_index(attr.as_ptr()),
        )
    };
    checked_raw(scope, raw)
}

unsafe fn getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    f: fn(&mut Scope<'_>, &Value) -> JsResult,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let this = quickjs::borrow_value(scope, this_val);
            let result = f(scope, &this);
            quickjs::result_raw(scope, result)
        })
    }
}

unsafe fn setter(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    f: fn(&mut Scope<'_>, &Value, &Value) -> JsResult<()>,
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

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                unsafe { getter(ctx, this_val, $f) }
            }
        )*
    };
}

macro_rules! setters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                val: JSValue,
            ) -> JSValue {
                unsafe { setter(ctx, this_val, val, $f) }
            }
        )*
    };
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
                let f: NativeFn = $f;
                unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
            }
        )*
    };
}

getters! {
    ns_element_get_zero_int => crate::element::zero,
    ns_element_get_null => crate::element::null,
    ns_element_get_empty_array_prop => crate::element::empty_array,
    ns_element_get_isContentEditable => crate::element::is_content_editable,
    ns_element_get_labels => crate::element::labels,
    ns_element_get_form_enctype => crate::element::form_enctype,
    ns_element_get_hidden => crate::element::hidden,
    ns_element_get_disabled => crate::element::disabled,
    ns_element_get_option_index => crate::element::option_index,
    ns_element_get_select_length => crate::element::select_length,
    ns_element_get_list_ref => crate::element::list,
    ns_element_get_form => crate::element::form,
    ns_element_img_complete => crate::image::complete,
    ns_element_img_natural_width => crate::image::natural_width,
    ns_element_img_natural_height => crate::image::natural_height,
    ns_element_img_current_src => crate::image::current_src,
    ns_element_table_rows => crate::table::rows,
    ns_element_table_caption => crate::table::caption,
    ns_element_table_thead => crate::table::thead,
    ns_element_table_tfoot => crate::table::tfoot,
    ns_element_table_tbodies => crate::table::tbodies,
    ns_element_tr_cells => crate::table::cells,
}

setters! {
    ns_element_noop_set => crate::element::ignore,
    ns_element_set_form_enctype => crate::element::set_form_enctype,
    ns_element_set_hidden => crate::element::set_hidden,
    ns_element_set_disabled => crate::element::set_disabled,
    ns_element_list_set => crate::element::set_list,
    ns_element_set_content => crate::element::set_content,
    ns_element_set_rows => crate::table::set_rows,
}

methods! {
    ns_table_createCaption => crate::table::create_caption,
    ns_table_createTHead => crate::table::create_thead,
    ns_table_createTBody => crate::table::create_tbody,
    ns_table_createTFoot => crate::table::create_tfoot,
    ns_table_deleteCaption => crate::table::delete_caption,
    ns_table_deleteTHead => crate::table::delete_thead,
    ns_table_deleteTFoot => crate::table::delete_tfoot,
    ns_table_insertRow => crate::table::insert_row,
    ns_table_deleteRow => crate::table::delete_row,
    ns_tr_insertCell => crate::table::insert_cell,
    ns_tr_deleteCell => crate::table::delete_cell,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_live_build_labels(ctx: *mut JSContext, n: *const NsNode) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let result = crate::element::build_labels(scope, Node::from_ptr(n));
            quickjs::result_raw(scope, result)
        })
    }
}
