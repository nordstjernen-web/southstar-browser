//! Southstar — the reflected attribute accessors' nsr_ entry points and C helpers, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_engine::{Attributes, Scope, Value};
use southstar_js_runtime::api;

use crate::{Element, JsResult, numeric, strings};

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

unsafe extern "Rust" {
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
    api::unwrap_element(value)
}

pub(crate) fn attr(node: Element, name: &CStr) -> Option<&'static [u8]> {
    southstar_dom::attrs::find(node, name).and_then(southstar_dom::Attr::value_bytes)
}

pub(crate) fn attr_c(node: Element, name: &CStr) -> Option<&'static CStr> {
    node.attr(name)
}

pub(crate) fn set_attr_len(js: Js, node: Element, name: &CStr, value: &[u8]) {
    southstar_js_mutation::set_attr_recorded(js.0, node, name, value);
}

pub(crate) fn set_attr(js: Js, node: Element, name: &CStr, value: &CStr) {
    set_attr_len(js, node, name, value.to_bytes());
}

pub(crate) fn remove_attr(js: Js, node: Element, name: &CStr) {
    southstar_js_mutation::remove_attr_recorded(js.0, node, name);
}

pub(crate) fn set_src(js: Js, node: Element, value: &JsText) {
    api::set_src_recorded(js.0, node, value.bytes());
}

pub(crate) fn image_natural_size(js: Js, node: Element) -> Option<(i32, i32)> {
    southstar_js_images::image_natural_size(js.0, node)
}

pub(crate) fn current_url(js: Js) -> Vec<u8> {
    api::current_url(js.0).into_bytes()
}

pub(crate) fn resolved_url(scope: &mut Scope<'_>, js: Js, href: &'static [u8]) -> Value {
    let resolved = southstar_js_document::doc_base_url(js.0)
        .filter(|base| !base.is_empty())
        .and_then(|base| southstar_url::bytes::resolve(Some(&base), c_prefix(href)));
    match resolved {
        Some(url) => scope.string_from_bytes(&url),
        None => scope.string_from_bytes(c_prefix(href)),
    }
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
    southstar_js_forms::is_submit_trigger(node)
}

pub(crate) fn form_owner(node: Element) -> Option<Element> {
    southstar_dom::controls::form_owner(node)
}

pub(crate) fn token_list(scope: &mut Scope<'_>, this: &Value, attr: &'static CStr) -> JsResult {
    nsr_make_token_list(scope, this, attr)
}

pub(crate) fn details_toggle_open(js: Js, node: Element, open: bool) {
    southstar_js_top_layer::details_toggle_open(js.0, node, open);
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
