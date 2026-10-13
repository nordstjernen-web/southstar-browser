//! Southstar — form.elements and its snapshot, named lookup into the listed controls with RadioNodeList, RadioNodeList.value, and the item() and namedItem() of array-backed collections.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::controls;
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Page};
use crate::{Element, JsResult, named, text_is};

const LIVE_FORM_ELEMENTS: i32 = 9;
const LIVE_RADIO_NODE_LIST: i32 = 11;
const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};
const REPLACEABLE: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: true,
};

fn length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let value = scope
        .get(array, "length")
        .unwrap_or_else(|_| Value::undefined());
    scope.to_number(&value).map_or(0, |n| n as u32)
}

fn items(scope: &mut Scope<'_>, array: &Value) -> Vec<Value> {
    let count = length(scope, array);
    (0..count)
        .map(|index| {
            scope
                .get_index(array, index)
                .unwrap_or_else(|_| Value::undefined())
        })
        .collect()
}

fn id_or_name_is(node: Element, name: &[u8]) -> bool {
    node.attr(c"id").is_some_and(|id| id.to_bytes() == name)
        || node.attr(c"name").is_some_and(|n| n.to_bytes() == name)
}

fn is_radio_input(node: Option<Element>) -> bool {
    node.is_some_and(|n| named(n, "input") && text_is(n.attr(c"type"), "radio"))
}

fn radio_value(node: Element) -> &'static [u8] {
    node.attr(c"value").map_or(b"on", |v| v.to_bytes())
}

pub fn form_elements(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    ffi::make_live(scope, this, LIVE_FORM_ELEMENTS, None)
}

pub(crate) fn controls_snapshot(scope: &mut Scope<'_>, form: &Value) -> JsResult {
    let node = ffi::element(form);
    let cached = ffi::qcache_get(scope, node, b'f');
    if !cached.is_undefined() {
        return Ok(cached);
    }
    let live = form_elements(scope, form, &[])?;
    let snapshot = ffi::live_snapshot(scope, &live);
    ffi::qcache_put(scope, node, b'f', &snapshot);
    Ok(snapshot)
}

pub(crate) fn array_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(arg) = args.first() else {
        return Ok(Value::null());
    };
    let index = scope.to_int32(arg).unwrap_or(0);
    if index < 0 {
        return Ok(Value::null());
    }
    let item = scope.get_index(this, index as u32)?;
    Ok(if item.is_undefined() {
        Value::null()
    } else {
        item
    })
}

pub(crate) fn array_named_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(arg) = args.first().filter(|a| a.is_string()) else {
        return Ok(Value::null());
    };
    let name = crate::until_nul(scope.to_bytes(arg)?);
    if name.is_empty() {
        return Ok(Value::null());
    }
    for item in items(scope, this) {
        if ffi::element(&item).is_some_and(|el| id_or_name_is(el, &name)) {
            return Ok(item);
        }
    }
    Ok(Value::null())
}

pub(crate) fn radio_list_value(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let snapshot = ffi::live_snapshot(scope, this);
    for item in items(scope, &snapshot) {
        let node = ffi::element(&item);
        if let Some(radio) = node.filter(|&n| is_radio_input(Some(n)) && controls::is_checked(n)) {
            return Ok(scope.string_from_bytes(radio_value(radio)));
        }
    }
    Ok(scope.string(""))
}

pub(crate) fn set_radio_list_value(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(arg) = args.first() else {
        return Ok(Value::undefined());
    };
    let wanted = crate::until_nul(scope.to_bytes(arg)?);
    let snapshot = ffi::live_snapshot(scope, this);
    let radios: Vec<Element> = items(scope, &snapshot)
        .iter()
        .filter_map(ffi::element)
        .filter(|&n| is_radio_input(Some(n)))
        .collect();
    let Some(chosen) = radios.iter().copied().find(|&r| radio_value(r) == wanted) else {
        return Ok(Value::undefined());
    };
    let page = Page::of(scope);
    for radio in radios.iter().copied().filter(|&r| r != chosen) {
        crate::checkable::set_checkedness(page, radio, false);
    }
    crate::checkable::set_checkedness(page, chosen, true);
    Ok(Value::undefined())
}

fn radio_node_list(
    scope: &mut Scope<'_>,
    elements: &Value,
    first: &Value,
    name: &[u8],
) -> JsResult {
    let mut form = ffi::element(first)
        .and_then(controls::form_owner)
        .map_or_else(Value::undefined, |owner| ffi::wrap(scope, Some(owner)));
    if !form.is_object() {
        form = ffi::live_owner(scope, elements);
    }
    if !form.is_object() {
        return ffi::make_live(scope, &Value::undefined(), LIVE_RADIO_NODE_LIST, Some(name));
    }
    let key = String::from_utf8_lossy(name).into_owned();
    let map = scope.get(&form, "_ns_rnl")?;
    let (map, existing) = if map.is_object() {
        let existing = scope.get(&map, &key)?;
        (map, existing)
    } else {
        let map = scope.new_object();
        let _ = scope.define(&form, "_ns_rnl", map.clone(), HIDDEN);
        (map, Value::undefined())
    };
    if !existing.is_undefined() {
        return Ok(existing);
    }
    let list = ffi::make_live(scope, &form, LIVE_RADIO_NODE_LIST, Some(name))?;
    let _ = scope.define(&map, &key, list.clone(), REPLACEABLE);
    Ok(list)
}

pub(crate) fn named_lookup(scope: &mut Scope<'_>, elements: &Value, name: &[u8]) -> JsResult {
    let snapshot = ffi::live_snapshot(scope, elements);
    let mut matches = items(scope, &snapshot)
        .into_iter()
        .filter(|item| ffi::element(item).is_some_and(|el| id_or_name_is(el, name)));
    let Some(first) = matches.next() else {
        return Ok(Value::null());
    };
    if matches.next().is_none() {
        return Ok(first);
    }
    radio_node_list(scope, elements, &first, name)
}

pub(crate) fn indexed_control(scope: &mut Scope<'_>, form: &Value, index: u32) -> JsResult {
    let elements = controls_snapshot(scope, form)?;
    if index >= length(scope, &elements) {
        return Ok(Value::undefined());
    }
    scope.get_index(&elements, index)
}

pub(crate) fn named_control(scope: &mut Scope<'_>, form: &Value, name: &[u8]) -> JsResult {
    let elements = controls_snapshot(scope, form)?;
    let key = String::from_utf8_lossy(name).into_owned();
    let result = named_lookup(scope, &elements, name)?;
    if !result.is_null() && !result.is_undefined() {
        let mut past = scope.get(form, "_ns_past_names")?;
        if !past.is_object() {
            past = scope.new_object();
            let _ = scope.define(form, "_ns_past_names", past.clone(), HIDDEN);
        }
        if ffi::element(&result).is_some() {
            let _ = scope.define(&past, &key, result.clone(), REPLACEABLE);
        }
        return Ok(result);
    }
    let past = scope.get(form, "_ns_past_names")?;
    if !past.is_object() {
        return Ok(Value::undefined());
    }
    let candidate = scope.get(&past, &key)?;
    if !candidate.is_object() {
        return Ok(Value::undefined());
    }
    let owner = ffi::element(form);
    let still_owned = owner.is_some()
        && ffi::element(&candidate).is_some_and(|node| controls::form_owner(node) == owner);
    if still_owned {
        return Ok(candidate);
    }
    let _ = scope.set(&past, &key, Value::undefined());
    Ok(Value::undefined())
}
