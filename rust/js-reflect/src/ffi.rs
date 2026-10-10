//! Southstar — the C ABI of the reflected attribute accessors as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::{Element, JsResult, numeric, strings};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

unsafe extern "C" {
    fn JS_ToCStringLen2(
        ctx: *mut JSContext,
        plen: *mut usize,
        val: JSValue,
        cesu8: c_int,
    ) -> *const c_char;
    fn JS_FreeCString(ctx: *mut JSContext, ptr: *const c_char);
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_element_get_attr_len(
        node: *const NsNode,
        name: *const c_char,
        len: *mut usize,
    ) -> *const c_char;
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_js_set_attr_recorded_len(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
        len: isize,
    );
    fn ns_js_remove_attr_recorded(js: *mut NsJs, node: *mut NsNode, name: *const c_char);
    fn ns_js_set_src_recorded(js: *mut NsJs, node: *mut NsNode, value: *const c_char, len: usize);
    fn ns_js_image_natural_size(
        js: *mut NsJs,
        node: *const NsNode,
        width: *mut c_int,
        height: *mut c_int,
    ) -> GBoolean;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_node_is_submit_trigger(el: *const NsNode) -> GBoolean;
    fn ns_form_owner(control: *const NsNode, doc: *const NsNode) -> *const NsNode;
    fn ns_make_token_list(ctx: *mut JSContext, element: JSValue, attr: *const c_char) -> JSValue;
    fn ns_js_details_toggle_open(js: *mut NsJs, details: *mut NsNode, open: GBoolean);
    fn ns_element_async_method(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope).cast())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn attr(node: Element, name: &CStr) -> Option<&'static [u8]> {
    let mut len = 0usize;
    let v = unsafe { ns_element_get_attr_len(node.as_ptr(), name.as_ptr(), &mut len) };
    (!v.is_null()).then(|| unsafe { core::slice::from_raw_parts(v.cast::<u8>(), len) })
}

pub(crate) fn attr_c(node: Element, name: &CStr) -> Option<&'static CStr> {
    let mut len = 0usize;
    let v = unsafe { ns_element_get_attr_len(node.as_ptr(), name.as_ptr(), &mut len) };
    (!v.is_null()).then(|| unsafe { CStr::from_ptr(v) })
}

pub(crate) fn set_attr_len(js: Js, node: Element, name: &CStr, value: &[u8]) {
    if value.is_empty() {
        return set_attr(js, node, name, c"");
    }
    unsafe {
        ns_js_set_attr_recorded_len(
            js.0,
            node.as_mut_ptr(),
            name.as_ptr(),
            value.as_ptr().cast(),
            value.len() as isize,
        )
    };
}

pub(crate) fn set_attr(js: Js, node: Element, name: &CStr, value: &CStr) {
    unsafe { ns_js_set_attr_recorded(js.0, node.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub(crate) fn remove_attr(js: Js, node: Element, name: &CStr) {
    unsafe { ns_js_remove_attr_recorded(js.0, node.as_mut_ptr(), name.as_ptr()) };
}

pub(crate) fn set_src(js: Js, node: Element, value: &JsText) {
    unsafe { ns_js_set_src_recorded(js.0, node.as_mut_ptr(), value.ptr, value.len) };
}

pub(crate) fn image_natural_size(js: Js, node: Element) -> Option<(i32, i32)> {
    let (mut w, mut h) = (0, 0);
    let found = unsafe { ns_js_image_natural_size(js.0, node.as_ptr(), &mut w, &mut h) };
    (found != glib::FALSE).then_some((w, h))
}

pub(crate) fn current_url(js: Js) -> &'static [u8] {
    unsafe { CStr::from_ptr(ns_js_current_url(js.0)) }.to_bytes()
}

pub(crate) fn current_document(js: Js) -> *const NsNode {
    if js.0.is_null() {
        ptr::null()
    } else {
        unsafe { ns_js_current_document(js.0) }
    }
}

pub(crate) fn resolved_url(scope: &mut Scope<'_>, js: Js, href: &'static [u8]) -> Value {
    let base = unsafe { ns_js_doc_base_url(js.0) };
    let mut out = None;
    if !base.is_null() && unsafe { *base } != 0 {
        let resolved = unsafe { ns_url_resolve(base, href.as_ptr().cast()) };
        if !resolved.is_null() {
            out = Some(scope.string_from_bytes(unsafe { CStr::from_ptr(resolved) }.to_bytes()));
            unsafe { glib::g_free(resolved.cast::<c_void>()) };
        }
    }
    unsafe { glib::g_free(base.cast::<c_void>()) };
    out.unwrap_or_else(|| scope.string_from_bytes(c_prefix(href)))
}

fn c_prefix(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&b| b == 0)
        .map_or(bytes, |end| &bytes[..end])
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

pub(crate) fn is_submit_trigger(node: Element) -> bool {
    unsafe { ns_node_is_submit_trigger(node.as_ptr()) != glib::FALSE }
}

pub(crate) fn form_owner(js: Js, node: Element) -> Option<Element> {
    unsafe { Node::from_ptr(ns_form_owner(node.as_ptr(), current_document(js))) }
}

pub(crate) fn token_list(scope: &mut Scope<'_>, this: &Value, attr: &'static CStr) -> Value {
    let raw = unsafe {
        ns_make_token_list(
            quickjs::raw_context(scope),
            quickjs::raw(this),
            attr.as_ptr(),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn details_toggle_open(js: Js, node: Element, open: bool) {
    unsafe { ns_js_details_toggle_open(js.0, node.as_mut_ptr(), glib::boolean(open)) };
}

pub(crate) fn async_method(scope: &mut Scope<'_>) -> Value {
    quickjs::c_function(scope, "async", 2, ns_element_async_method)
}

pub(crate) fn define_own(
    scope: &mut Scope<'_>,
    this: &Value,
    name: &CStr,
    value: &Value,
    attributes: Attributes,
) {
    let _ = scope.define(
        this,
        name.to_str().unwrap_or_default(),
        value.clone(),
        attributes,
    );
}

pub(crate) struct JsText {
    ctx: *mut JSContext,
    ptr: *const c_char,
    len: usize,
}

impl JsText {
    pub fn bytes(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.ptr.cast::<u8>(), self.len) }
    }

    pub fn c_str(&self) -> &CStr {
        unsafe { CStr::from_ptr(self.ptr) }
    }
}

impl Drop for JsText {
    fn drop(&mut self) {
        unsafe { JS_FreeCString(self.ctx, self.ptr) };
    }
}

pub(crate) fn to_text(scope: &mut Scope<'_>, value: &Value) -> JsResult<JsText> {
    let ctx = quickjs::raw_context(scope);
    let mut len = 0usize;
    let ptr = unsafe { JS_ToCStringLen2(ctx, &mut len, quickjs::raw(value), 0) };
    if ptr.is_null() {
        return Err(quickjs::take_exception(scope));
    }
    Ok(JsText { ctx, ptr, len })
}

unsafe fn getter(
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

unsafe fn setter(
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

macro_rules! magic_accessors {
    ($($get:ident, $set:ident => $g:path, $s:path);* $(;)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $get(
                ctx: *mut JSContext,
                this_val: JSValue,
                magic: c_int,
            ) -> JSValue {
                unsafe { getter(ctx, this_val, |scope, this| $g(scope, this, magic)) }
            }

            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $set(
                ctx: *mut JSContext,
                this_val: JSValue,
                val: JSValue,
                magic: c_int,
            ) -> JSValue {
                unsafe { setter(ctx, this_val, val, |scope, this, val| $s(scope, this, val, magic)) }
            }
        )*
    };
}

magic_accessors! {
    ns_element_attr_getter, ns_element_attr_setter => strings::attr_get, strings::attr_set;
    ns_element_enum_getter, ns_element_enum_setter => strings::enum_get, strings::enum_set;
    ns_element_aria_string_getter, ns_element_aria_string_setter => strings::aria_get, strings::aria_set;
    ns_element_int_attr_getter, ns_element_int_attr_setter => numeric::int_get, numeric::int_set;
    ns_element_bool_attr_getter, ns_element_bool_attr_setter => numeric::plain_bool_get, numeric::plain_bool_set;
    ns_element_boolattr_getter, ns_element_boolattr_setter => numeric::bool_get, numeric::bool_set;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_dimension_setter(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        setter(ctx, this_val, val, |scope, this, val| {
            numeric::dimension_set(scope, this, val, magic)
        })
    }
}

getters! {
    ns_element_get_autocomplete => strings::autocomplete_get,
    ns_element_get_dir => strings::dir_get,
    ns_element_get_translate => strings::translate_get,
    ns_element_get_type => strings::type_get,
    ns_element_get_autocapitalize => strings::autocapitalize_get,
    ns_element_get_spellcheck => strings::spellcheck_get,
    ns_element_get_htmlFor => strings::html_for_get,
    ns_element_get_draggable => strings::draggable_get,
    ns_element_get_contentEditable => strings::content_editable_get,
    ns_element_get_id => strings::id_get,
    ns_element_get_className => strings::class_name_get,
    ns_element_get_tabIndex => numeric::tab_index_get,
}

setters! {
    ns_element_set_autocomplete => strings::autocomplete_set,
    ns_element_set_dir => strings::dir_set,
    ns_element_set_translate => strings::translate_set,
    ns_element_set_autocapitalize => strings::autocapitalize_set,
    ns_element_set_spellcheck => strings::spellcheck_set,
    ns_element_set_htmlFor => strings::html_for_set,
    ns_element_set_draggable => strings::draggable_set,
    ns_element_set_contentEditable => strings::content_editable_set,
    ns_element_set_id => strings::id_set,
    ns_element_set_className => strings::class_name_set,
    ns_element_attr_setter_sizes => strings::sizes_set,
    ns_element_attr_setter_sandbox => strings::sandbox_set,
    ns_element_set_tabIndex => numeric::tab_index_set,
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_reflect_str_get(
    ctx: *mut JSContext,
    this_val: JSValue,
    attr: *const c_char,
    null_if_absent: GBoolean,
) -> JSValue {
    let attr = c_str(attr).unwrap_or(c"");
    unsafe {
        getter(ctx, this_val, |scope, this| {
            Ok(strings::reflect_get(
                scope,
                this,
                attr,
                null_if_absent != glib::FALSE,
            ))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_reflect_str_set(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    attr: *const c_char,
) -> JSValue {
    let attr = c_str(attr).unwrap_or(c"");
    unsafe {
        setter(ctx, this_val, val, |scope, this, val| {
            strings::reflect_set(scope, this, val, attr)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_enum_normalize(attr: *const c_char, v: *const c_char) -> *const c_char {
    let Some(attr) = c_str(attr) else {
        return ptr::null();
    };
    crate::normalize(attr.to_bytes(), c_str(v).map(CStr::to_bytes))
        .map_or(ptr::null(), CStr::as_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_custom_element(n: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { Node::from_ptr(n) }.is_some_and(crate::is_custom_element))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse_int(s: *const c_char, out: *mut i64) -> GBoolean {
    let Some(v) = c_str(s).and_then(|s| numeric::parse_int(s.to_bytes())) else {
        return glib::FALSE;
    };
    unsafe { *out = v };
    glib::TRUE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_int_attr_index(attr: *const c_char) -> c_int {
    c_str(attr).map_or(-1, |attr| numeric::int_attr_index(attr.to_bytes()))
}
