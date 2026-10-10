//! Southstar — the microdata API: itemScope, itemId, itemType, itemProp, itemRef, itemValue, properties and document.getItems.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::collections::{HashSet, VecDeque};

use southstar_dom::{Kind, MAX_DEPTH};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, NameSet};
use crate::{Element, JsResult};

const INVALID_ACCESS_ERR: i32 = 15;

const NAMED_PROPERTY: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

const RESERVED_NAMES: &[&[u8]] = &[
    b"length",
    b"item",
    b"forEach",
    b"names",
    b"namedItem",
    b"getValues",
    b"entries",
    b"keys",
    b"values",
];

fn has_itemprop(n: Element) -> bool {
    n.attr(c"itemprop").is_some_and(|v| !v.is_empty())
}

fn has_itemscope(n: Element) -> bool {
    n.attr(c"itemscope").is_some()
}

fn is_space(b: &u8) -> bool {
    matches!(b, b' ' | 0x09 | 0x0a | 0x0b | 0x0c | 0x0d)
}

fn key(n: Element) -> usize {
    n.as_ptr() as usize
}

fn tokens(value: &[u8]) -> impl Iterator<Item = &[u8]> {
    value.split(is_space).filter(|token| !token.is_empty())
}

fn attr_tokens(n: Element, name: &CStr) -> Vec<&'static [u8]> {
    n.attr(name)
        .map_or_else(Vec::new, |v| tokens(v.to_bytes()).collect())
}

fn itemprop_has(n: Element, name: &[u8]) -> bool {
    n.attr(c"itemprop")
        .is_some_and(|v| tokens(v.to_bytes()).any(|token| token == name))
}

fn value_attr(name: &[u8]) -> Option<&'static CStr> {
    match name {
        b"meta" => Some(c"content"),
        b"audio" | b"embed" | b"iframe" | b"img" | b"source" | b"track" | b"video" => Some(c"src"),
        b"a" | b"area" | b"link" => Some(c"href"),
        b"object" => Some(c"data"),
        b"data" => Some(c"value"),
        _ => None,
    }
}

fn url_value(scope: &mut Scope<'_>, n: Element, attr: &CStr) -> Value {
    let Some(value) = n.attr(attr).filter(|v| !v.is_empty()) else {
        return scope.string_from_bytes(b"");
    };
    match ffi::resolve_against_page(ffi::js_of(scope), value) {
        Some(resolved) => scope.string_from_bytes(&resolved),
        None => scope.string_from_bytes(value.to_bytes()),
    }
}

fn item_value(scope: &mut Scope<'_>, n: Option<Element>) -> Value {
    let Some((n, name)) = n.and_then(|n| n.name().map(|name| (n, name.to_bytes()))) else {
        return Value::null();
    };
    if !has_itemprop(n) {
        return Value::null();
    }
    if has_itemscope(n) {
        return ffi::wrap_node(scope, n);
    }
    if let Some(attr) = value_attr(name) {
        if matches!(attr.to_bytes(), b"src" | b"href" | b"data") {
            return url_value(scope, n, attr);
        }
        let value = n.attr(attr).map_or(&b""[..], CStr::to_bytes);
        return scope.string_from_bytes(value);
    }
    if name == b"time"
        && let Some(datetime) = n.attr(c"datetime")
    {
        return scope.string_from_bytes(datetime.to_bytes());
    }
    let text = southstar_dom::serialize::collect_text(Some(n));
    scope.string_from_bytes(&text)
}

pub(crate) fn item_scope_get(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(Value::boolean(
        ffi::unwrap_node(this).is_some_and(has_itemscope),
    ))
}

pub(crate) fn item_scope_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let js = ffi::js_of(scope);
    if scope.to_bool(val) {
        ffi::set_attr_recorded(js, n, c"itemscope", c"");
    } else {
        ffi::remove_attr_recorded(js, n, c"itemscope");
    }
    Ok(())
}

pub(crate) fn item_id_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let value = ffi::unwrap_node(this)
        .and_then(|n| n.attr(c"itemid"))
        .map_or(&b""[..], CStr::to_bytes);
    Ok(scope.string_from_bytes(value))
}

pub(crate) fn item_id_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let value = ffi::c_string(&scope.to_bytes(val)?);
    ffi::set_attr_recorded(ffi::js_of(scope), n, c"itemid", &value);
    Ok(())
}

pub(crate) fn item_type_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(ffi::token_list(scope, this, c"itemtype"))
}

pub(crate) fn item_prop_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(ffi::token_list(scope, this, c"itemprop"))
}

pub(crate) fn item_ref_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(ffi::token_list(scope, this, c"itemref"))
}

pub(crate) fn item_value_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(item_value(scope, ffi::unwrap_node(this)))
}

pub(crate) fn item_value_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    let Some((n, name)) =
        ffi::unwrap_node(this).and_then(|n| n.name().map(|name| (n, name.to_bytes())))
    else {
        return Ok(());
    };
    if has_itemscope(n) {
        return Err(ffi::dom_exception(
            scope,
            c"InvalidAccessError",
            INVALID_ACCESS_ERR,
            c"cannot set itemValue on an item",
        ));
    }
    let attr = value_attr(name)
        .or_else(|| (name == b"time" && n.attr(c"datetime").is_some()).then_some(c"datetime"));
    match attr {
        Some(attr) => {
            let value = ffi::c_string(&scope.to_bytes(val)?);
            ffi::set_attr_recorded(ffi::js_of(scope), n, attr, &value);
            Ok(())
        }
        None => crate::text::text_content_set(scope, this, val),
    }
}

fn property_set(root: Element, doc: Option<Element>) -> HashSet<usize> {
    let mut memory = HashSet::from([key(root)]);
    let mut props = HashSet::new();
    let mut pending: VecDeque<Element> = southstar_dom::children(root)
        .filter(|c| c.kind() == Kind::Element)
        .collect();
    if let Some(doc) = doc {
        let ids = NameSet::new();
        for id in attr_tokens(root, c"itemref") {
            ids.insert(id);
        }
        for id in ids.names() {
            let id = ffi::c_string(&id);
            if let Some(referenced) = southstar_dom::index::find_by_id(doc, &id)
                && referenced.kind() == Kind::Element
            {
                pending.push_back(referenced);
            }
        }
    }
    while let Some(cur) = pending.pop_front() {
        if !memory.insert(key(cur)) {
            continue;
        }
        if has_itemprop(cur) {
            props.insert(key(cur));
        }
        if !has_itemscope(cur) {
            pending.extend(southstar_dom::children(cur).filter(|c| c.kind() == Kind::Element));
        }
    }
    props
}

fn collect_in_order(n: Element, props: &HashSet<usize>, out: &mut Vec<Element>, depth: i32) {
    if depth >= MAX_DEPTH || ffi::hidden_child(n) {
        return;
    }
    if n.kind() == Kind::Element && props.contains(&key(n)) {
        out.push(n);
    }
    for child in southstar_dom::children(n) {
        collect_in_order(child, props, out, depth + 1);
    }
}

fn property_nodelist(scope: &mut Scope<'_>, array: Value) -> JsResult {
    let list = ffi::nodelist_from_array(scope, array);
    let get_values = scope.function("getValues", 0, property_get_values);
    scope.define(&list, "getValues", get_values, Attributes::METHOD)?;
    Ok(list)
}

fn property_get_values(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let out = scope.new_array();
    let len = ffi::array_length(scope, this);
    for i in 0..len {
        let element = scope.get_index(this, i)?;
        let value = item_value(scope, ffi::unwrap_node(&element));
        scope.set_index(&out, i, value)?;
    }
    Ok(out)
}

fn property_named(scope: &mut Scope<'_>, collection: &Value, name: &[u8]) -> JsResult {
    let array = scope.new_array();
    let mut count = 0;
    let len = ffi::array_length(scope, collection);
    for i in 0..len {
        let element = scope.get_index(collection, i)?;
        if ffi::unwrap_node(&element).is_some_and(|n| itemprop_has(n, name)) {
            scope.set_index(&array, count, element)?;
            count += 1;
        }
    }
    property_nodelist(scope, array)
}

fn property_named_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(name) = args.first() else {
        let empty = scope.new_array();
        return property_nodelist(scope, empty);
    };
    let name = scope.to_bytes(name)?;
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    property_named(scope, this, &name[..end])
}

pub(crate) fn properties_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let n = ffi::unwrap_node(this);
    let doc = ffi::current_document(ffi::js_of(scope));
    let mut nodes = Vec::new();
    if let Some(n) = n.filter(|&n| has_itemscope(n)) {
        let props = property_set(n, doc);
        collect_in_order(doc.unwrap_or(n), &props, &mut nodes, 0);
    }
    let array = scope.new_array();
    for (i, &node) in nodes.iter().enumerate() {
        let wrapped = ffi::wrap_node(scope, node);
        scope.set_index(&array, i as u32, wrapped)?;
    }
    let collection = ffi::nodelist_from_array(scope, array);

    let names = scope.new_array();
    let mut names_len = 0;
    let seen = NameSet::new();
    for &node in &nodes {
        for token in attr_tokens(node, c"itemprop") {
            if seen.insert(token) {
                let name = scope.string_from_bytes(token);
                scope.set_index(&names, names_len, name)?;
                names_len += 1;
            }
        }
    }
    scope.define(&collection, "names", names, NAMED_PROPERTY)?;
    let named_item = scope.function("namedItem", 1, property_named_item);
    scope.define(&collection, "namedItem", named_item, Attributes::METHOD)?;

    for name in seen.names() {
        if RESERVED_NAMES.contains(&name.as_slice()) {
            continue;
        }
        let list = property_named(scope, &collection, &name)?;
        let key = String::from_utf8_lossy(&name);
        scope.define(&collection, &key, list, NAMED_PROPERTY)?;
    }
    Ok(collection)
}

fn types_match(item: Element, wanted: &[&[u8]]) -> bool {
    let have = attr_tokens(item, c"itemtype");
    wanted.iter().all(|w| have.contains(w))
}

fn collect_top_items(
    scope: &mut Scope<'_>,
    n: Element,
    wanted: &[&[u8]],
    array: &Value,
    count: &mut u32,
    depth: i32,
) -> JsResult<()> {
    if depth >= MAX_DEPTH {
        return Ok(());
    }
    if n.kind() == Kind::Element && has_itemscope(n) && !has_itemprop(n) && types_match(n, wanted) {
        let wrapped = ffi::wrap_node(scope, n);
        scope.set_index(array, *count, wrapped)?;
        *count += 1;
    }
    for child in southstar_dom::children(n) {
        collect_top_items(scope, child, wanted, array, count, depth + 1)?;
    }
    Ok(())
}

pub(crate) fn get_items(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let array = scope.new_array();
    let Some(doc) = ffi::current_document(ffi::js_of(scope)) else {
        return Ok(ffi::nodelist_from_array(scope, array));
    };
    let types = match args.first() {
        Some(arg) if !arg.is_undefined() && !arg.is_null() => {
            let mut bytes = scope.to_bytes(arg)?;
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            bytes.truncate(end);
            bytes
        }
        _ => Vec::new(),
    };
    let wanted: Vec<&[u8]> = tokens(&types).collect();
    let mut count = 0;
    collect_top_items(scope, doc, &wanted, &array, &mut count, 0)?;
    Ok(ffi::nodelist_from_array(scope, array))
}
