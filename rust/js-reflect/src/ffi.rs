//! Southstar — the reflected attribute accessors' nsr_ entry points and C helpers, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::{Element, JsResult, numeric, strings};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

unsafe extern "C" {
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
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_node_is_submit_trigger(el: *const NsNode) -> GBoolean;
    fn ns_js_details_toggle_open(js: *mut NsJs, details: *mut NsNode, open: GBoolean);
}

unsafe extern "Rust" {
    safe fn nsr_unwrap_element(value: &Value) -> *const NsNode;
    safe fn nsr_dom_exception(
        scope: &mut Scope<'_>,
        name: &[u8],
        code: i32,
        message: &[u8],
    ) -> Value;
    safe fn nsr_make_token_list(
        scope: &mut Scope<'_>,
        element: &Value,
        attr: &'static CStr,
    ) -> Result<Value, Value>;
    safe fn nsr_element_async_method(
        scope: &mut Scope<'_>,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value>;
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque().cast())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(nsr_unwrap_element(value)) }
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
    let bytes = value.bytes();
    unsafe { ns_js_set_src_recorded(js.0, node.as_mut_ptr(), bytes.as_ptr().cast(), bytes.len()) };
}

pub(crate) fn image_natural_size(js: Js, node: Element) -> Option<(i32, i32)> {
    let (mut w, mut h) = (0, 0);
    let found = unsafe { ns_js_image_natural_size(js.0, node.as_ptr(), &mut w, &mut h) };
    (found != glib::FALSE).then_some((w, h))
}

pub(crate) fn current_url(js: Js) -> &'static [u8] {
    unsafe { CStr::from_ptr(ns_js_current_url(js.0)) }.to_bytes()
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
    nsr_dom_exception(scope, name.to_bytes(), code, message.to_bytes())
}

pub(crate) fn is_submit_trigger(node: Element) -> bool {
    unsafe { ns_node_is_submit_trigger(node.as_ptr()) != glib::FALSE }
}

pub(crate) fn form_owner(node: Element) -> Option<Element> {
    southstar_dom::controls::form_owner(node)
}

pub(crate) fn token_list(scope: &mut Scope<'_>, this: &Value, attr: &'static CStr) -> JsResult {
    nsr_make_token_list(scope, this, attr)
}

pub(crate) fn details_toggle_open(js: Js, node: Element, open: bool) {
    unsafe { ns_js_details_toggle_open(js.0, node.as_mut_ptr(), glib::boolean(open)) };
}

pub(crate) fn async_method(scope: &mut Scope<'_>) -> Value {
    scope.function("async", 2, nsr_element_async_method)
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

pub(crate) struct JsText(Vec<u8>);

impl JsText {
    pub fn bytes(&self) -> &[u8] {
        &self.0[..self.0.len() - 1]
    }

    pub fn c_str(&self) -> &CStr {
        CStr::from_bytes_until_nul(&self.0).unwrap_or(c"")
    }
}

pub(crate) fn to_text(scope: &mut Scope<'_>, value: &Value) -> JsResult<JsText> {
    let mut bytes = scope.to_bytes(value)?;
    bytes.push(0);
    Ok(JsText(bytes))
}

#[unsafe(no_mangle)]
pub fn nsr_element_attr_getter(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    strings::attr_get(scope, this, magic)
}

#[unsafe(no_mangle)]
pub fn nsr_element_attr_setter(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    magic: i32,
) -> JsResult<()> {
    strings::attr_set(scope, this, value, magic)
}

#[unsafe(no_mangle)]
pub fn nsr_element_int_attr_getter(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    numeric::int_get(scope, this, magic)
}

#[unsafe(no_mangle)]
pub fn nsr_element_int_attr_setter(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    magic: i32,
) -> JsResult<()> {
    numeric::int_set(scope, this, value, magic)
}

#[unsafe(no_mangle)]
pub fn nsr_element_reflect_str_get(
    scope: &mut Scope<'_>,
    this: &Value,
    attr: &CStr,
    null_if_absent: bool,
) -> JsResult {
    Ok(strings::reflect_get(scope, this, attr, null_if_absent))
}

#[unsafe(no_mangle)]
pub fn nsr_element_reflect_str_set(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    attr: &CStr,
) -> JsResult<()> {
    strings::reflect_set(scope, this, value, attr)
}

pub(crate) fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
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
