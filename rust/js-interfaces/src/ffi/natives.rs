//! Southstar — the natives the DOM property tables point at that have no ready-made shape elsewhere: getNumberOfChars, accessKeyLabel, the type setter, the document's forms, images, all and links collections and the node, color, dimension and hyperlink accessors.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_dom::serialize::collect_text;
use southstar_js_attrs as attrs;
use southstar_js_collections as collections;
use southstar_js_cssom as cssom;
use southstar_js_document as document;
use southstar_js_dom_factory as dom_factory;
use southstar_js_engine::{Scope, Value};
use southstar_js_fonts as fonts;
use southstar_js_forms as forms;
use southstar_js_geometry as geometry;
use southstar_js_node as node;
use southstar_js_reflect as reflect;
use southstar_js_runtime::api;

type JsResult<T = Value> = Result<T, Value>;

const LIVE_DOC_TAG: c_int = 3;
const LIVE_LINKS: c_int = 10;

fn utf8_char_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b & 0xc0 != 0x80).count()
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes.split(|&b| b == 0).next().unwrap_or_default()
}

fn c_string(bytes: Vec<u8>) -> CString {
    CString::new(until_nul(&bytes)).unwrap_or_default()
}

pub(crate) fn element_get_number_of_chars(
    _scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> JsResult {
    let Some(node) = api::unwrap_element(this) else {
        return Ok(Value::int(0));
    };
    let text = collect_text(Some(node));
    Ok(Value::int(utf8_char_count(until_nul(&text)) as i32))
}

pub(crate) fn element_get_access_key_label(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let key = api::unwrap_element(this)
        .and_then(|node| node.attr(c"accesskey"))
        .map(CStr::to_bytes)
        .filter(|key| core::str::from_utf8(key).is_ok_and(|key| key.chars().count() == 1))
        .unwrap_or_default();
    Ok(scope.string_from_bytes(key))
}

pub(crate) fn element_set_type(scope: &mut Scope<'_>, this: &Value, value: &Value) -> JsResult<()> {
    let result = reflect::strings::reflect_set(scope, this, value, c"type");
    if let Some(node) = api::unwrap_element(this) {
        southstar_js_forms::resanitize_value(node);
    }
    result
}

pub(crate) fn event_true(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::boolean(true))
}

pub(crate) fn event_false(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::boolean(false))
}

pub(crate) fn document_get_forms(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(collections::make_live(
        scope,
        this,
        LIVE_DOC_TAG,
        Some(b"form"),
    ))
}

pub(crate) fn document_get_images(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(collections::make_live(
        scope,
        this,
        LIVE_DOC_TAG,
        Some(b"img"),
    ))
}

pub(crate) fn document_get_all(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let all = collections::make_live(scope, this, LIVE_DOC_TAG, Some(b"*"));
    scope.set_is_html_dda(&all);
    Ok(all)
}

pub(crate) fn document_get_links(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(collections::make_live(scope, this, LIVE_LINKS, None))
}

pub(crate) fn document_get_color(scope: &mut Scope<'_>, _this: &Value, magic: i32) -> JsResult {
    document::props::get_color(scope, magic)
}

pub(crate) fn document_set_color(
    scope: &mut Scope<'_>,
    _this: &Value,
    value: &Value,
    magic: i32,
) -> JsResult<()> {
    document::props::set_color(scope, value, magic).map(drop)
}

pub(crate) fn document_get_default_view(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    document::view::default_view(scope, this, &[])
}

pub(crate) fn document_get_fonts(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    fonts::font_set::document_fonts(scope, this, &[])
}

pub(crate) fn document_implementation(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    dom_factory::implementation::of_document(scope, this)
}

pub(crate) fn element_anchor_part_get(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    dom_factory::hyperlink::part_get(scope, this, magic)
}

pub(crate) fn element_anchor_href_set(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    _magic: i32,
) -> JsResult<()> {
    reflect::strings::reflect_set(scope, this, value, c"href")
}

pub(crate) fn element_url_part_set(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    magic: i32,
) -> JsResult<()> {
    dom_factory::hyperlink::part_set(scope, this, value, magic)
}

pub(crate) fn element_dimension_getter(
    scope: &mut Scope<'_>,
    this: &Value,
    magic: i32,
) -> JsResult {
    geometry::metrics::dimension(scope, this, magic)
}

pub(crate) fn element_dimension_setter(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    magic: i32,
) -> JsResult<()> {
    reflect::numeric::dimension_set(scope, this, value, magic)
}

pub(crate) fn element_range_number_getter(
    scope: &mut Scope<'_>,
    this: &Value,
    magic: i32,
) -> JsResult {
    forms::value::range_number(scope, this, magic)
}

pub(crate) fn element_range_number_setter(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
    magic: i32,
) -> JsResult<()> {
    forms::value::set_range_number(scope, this, value, magic).map(drop)
}

pub(crate) fn element_get_dataset(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    attrs::class::element_dataset(scope, this)
}

pub(crate) fn element_get_style(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    cssom::declaration::element_style(scope, this, &[])
}

pub(crate) fn element_get_attribute(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let (Some(element), Some(name)) = (api::unwrap_element(this), args.first()) else {
        return Ok(Value::null());
    };
    let name = c_string(scope.to_bytes(name)?);
    Ok(match node::attributes::attribute_value(element, &name) {
        Some(value) => scope.string_from_bytes(value),
        None => Value::null(),
    })
}

pub(crate) fn element_has_attribute(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let (Some(element), Some(name)) = (api::unwrap_element(this), args.first()) else {
        return Ok(Value::boolean(false));
    };
    let name = c_string(scope.to_bytes(name)?);
    Ok(Value::boolean(node::attributes::has_name(element, &name)))
}

pub(crate) fn element_contains(_scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let other = args.first().and_then(api::unwrap_element);
    Ok(Value::boolean(node::tree::contains(
        api::unwrap_element(this),
        other,
    )))
}

pub(crate) fn element_compare_document_position(
    _scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let position = match (api::unwrap_element(this), args.first()) {
        (Some(a), Some(b)) => node::tree::compare_document_position(a, api::unwrap_element(b)),
        _ => 0,
    };
    Ok(Value::int(position))
}

pub(crate) fn element_is_equal_node(
    _scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let other = args.first().and_then(api::unwrap_element);
    let equal = match (api::unwrap_element(this), other) {
        (Some(a), Some(b)) => node::tree::equal(a, b, 0),
        _ => false,
    };
    Ok(Value::boolean(equal))
}

pub(crate) fn element_has_child_nodes(
    _scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> JsResult {
    Ok(Value::boolean(node::tree::has_child_nodes(
        api::unwrap_element(this),
    )))
}

pub(crate) fn element_get_child_element_count(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(Value::int(node::tree::child_element_count(
        api::unwrap_element(this),
    )))
}

pub(crate) fn element_get_is_connected(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(Value::boolean(node::tree::is_connected(
        api::unwrap_element(this),
    )))
}

pub(crate) fn element_get_node_type(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(Value::int(node::tree::node_type(api::unwrap_element(this))))
}
