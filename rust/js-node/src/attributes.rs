//! Southstar — the Element attribute methods: get/has/set/remove/toggleAttribute, their NS variants, getAttributeNames and hasAttributes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::borrow::Cow;
use std::ffi::CString;

use southstar_dom::attrs::{self, is_internal};
use southstar_dom::{Attr, FLAG_FOREIGN_NS, FLAG_SVG_NS};
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, JsResult};

const INVALID_CHARACTER_ERR: i32 = 5;

fn page_attr(attr: &Attr<'_>) -> bool {
    attr.name().is_some_and(|n| !is_internal(n.to_bytes()))
}

fn normalize<'a>(node: Element, raw: &'a CStr) -> Cow<'a, CStr> {
    if node.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) != 0
        || !raw.to_bytes().iter().any(u8::is_ascii_uppercase)
    {
        return Cow::Borrowed(raw);
    }
    let lowered = raw.to_bytes().to_ascii_lowercase();
    Cow::Owned(CString::new(lowered).unwrap_or_default())
}

fn find_page_attr(node: Element, name: &CStr) -> Option<Attr<'static>> {
    node.attrs()
        .filter(page_attr)
        .find(|attr| attr.name() == Some(name))
}

fn find_page_attr_ns(
    node: Element,
    namespace: Option<&CStr>,
    local: &CStr,
) -> Option<Attr<'static>> {
    attrs::find_ns(node, namespace, local).filter(page_attr)
}

fn element(this: &Value) -> Option<Element> {
    ffi::unwrap_node(this).filter(|n| n.is_element())
}

fn arg(args: &[Value], index: usize) -> &Value {
    &args[index]
}

fn namespace_arg(scope: &mut Scope<'_>, value: &Value) -> JsResult<Option<CString>> {
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    ffi::with_text(scope, value, |_, bytes, _| {
        (!bytes.is_empty()).then(|| CString::new(bytes).unwrap_or_default())
    })
}

fn text_arg(scope: &mut Scope<'_>, value: &Value) -> JsResult<CString> {
    ffi::with_text(scope, value, |_, _, text| text.to_owned())
}

fn bytes_arg(scope: &mut Scope<'_>, value: &Value) -> JsResult<Vec<u8>> {
    ffi::with_text(scope, value, |_, bytes, _| bytes.to_vec())
}

fn invalid_character(scope: &mut Scope<'_>, message: &str) -> Value {
    ffi::dom_exception(
        scope,
        "InvalidCharacterError",
        INVALID_CHARACTER_ERR,
        message,
    )
}

pub(crate) fn attribute_value(node: Element, raw: &CStr) -> Option<&'static [u8]> {
    find_page_attr(node, &normalize(node, raw)).and_then(Attr::value_bytes)
}

pub(crate) fn has_name(node: Element, raw: &CStr) -> bool {
    find_page_attr(node, &normalize(node, raw)).is_some()
}

pub(crate) fn get_attribute_names(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> JsResult {
    let array = scope.new_array();
    let Some(node) = element(this) else {
        return Ok(array);
    };
    let names = node.attrs().filter(page_attr).filter_map(Attr::name);
    for (index, name) in (0..).zip(names) {
        let name = scope.string_from_bytes(name.to_bytes());
        scope.set_index(&array, index, name)?;
    }
    Ok(array)
}

pub(crate) fn has_attributes(_scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::boolean(
        element(this).is_some_and(|n| n.attrs().any(|a| page_attr(&a))),
    ))
}

pub(crate) fn toggle_attribute(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() {
        return Err(scope.type_error("1 argument required, but only 0 present"));
    }
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::boolean(false));
    };
    ffi::with_text(scope, arg(args, 0), |scope, bytes, raw| {
        let raw = checked_name(scope, bytes, raw, "toggleAttribute: invalid attribute name")?;
        let name = normalize(node, raw);
        if is_internal(name.to_bytes()) {
            return Ok(Value::boolean(false));
        }
        let had = node.attr(&name).is_some();
        let want = match args.get(1).filter(|force| !force.is_undefined()) {
            Some(force) => scope.to_bool(force),
            None => !had,
        };
        let js = ffi::js_of(scope);
        if want && !had {
            ffi::set_attr_recorded(js, node, &name, c"");
        } else if !want && had {
            ffi::remove_attr_recorded(js, node, &name);
        }
        Ok(Value::boolean(want))
    })?
}

pub(crate) fn get_attribute_ns(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = element(this).filter(|_| args.len() >= 2) else {
        return Ok(Value::null());
    };
    let namespace = namespace_arg(scope, arg(args, 0))?;
    let local = text_arg(scope, arg(args, 1))?;
    Ok(
        match find_page_attr_ns(node, namespace.as_deref(), &local) {
            Some(attr) => scope.string_from_bytes(attr.value().unwrap_or(c"").to_bytes()),
            None => Value::null(),
        },
    )
}

pub(crate) fn has_attribute_ns(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = element(this).filter(|_| args.len() >= 2) else {
        return Ok(Value::boolean(false));
    };
    let namespace = namespace_arg(scope, arg(args, 0))?;
    let local = text_arg(scope, arg(args, 1))?;
    Ok(Value::boolean(
        find_page_attr_ns(node, namespace.as_deref(), &local).is_some(),
    ))
}

pub(crate) fn set_attribute_ns(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.len() < 3 {
        return Err(scope.type_error(&format!(
            "3 arguments required, but only {} present",
            args.len()
        )));
    }
    let namespace = namespace_arg(scope, arg(args, 0))?;
    let Ok(name) = CString::new(bytes_arg(scope, arg(args, 1))?) else {
        return Err(invalid_character(scope, "invalid qualified name"));
    };
    ffi::validate_attr_ns(scope, namespace.as_deref(), &name)?;
    let Some(node) = element(this) else {
        return Ok(Value::undefined());
    };
    let value = text_arg(scope, arg(args, 2))?;
    if is_internal(name.to_bytes()) {
        return Ok(Value::undefined());
    }
    let (prefix, local) = match name.to_bytes().iter().position(|&b| b == b':') {
        Some(colon) => (
            CString::new(&name.to_bytes()[..colon]).ok(),
            CString::new(&name.to_bytes()[colon + 1..]).unwrap_or_default(),
        ),
        None => (None, name.clone()),
    };
    let qname = ffi::NsAttrName {
        namespace: namespace.as_deref(),
        prefix: prefix.as_deref(),
        local: &local,
        name: &name,
    };
    ffi::set_attr_ns_recorded(ffi::js_of(scope), node, qname, &value);
    Ok(Value::undefined())
}

pub(crate) fn remove_attribute_ns(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = element(this).filter(|_| args.len() >= 2) else {
        return Ok(Value::undefined());
    };
    let namespace = namespace_arg(scope, arg(args, 0))?;
    let local = text_arg(scope, arg(args, 1))?;
    if find_page_attr_ns(node, namespace.as_deref(), &local).is_some() {
        ffi::remove_attr_ns_recorded(ffi::js_of(scope), node, namespace.as_deref(), &local);
    }
    Ok(Value::undefined())
}

fn named(node: Element, tag: &[u8]) -> bool {
    node.element_name() == Some(tag)
}

fn is(name: &CStr, attr: &str) -> bool {
    name.to_bytes().eq_ignore_ascii_case(attr.as_bytes())
}

struct OldValue(Option<Vec<u8>>);

impl OldValue {
    fn capture(node: Element, name: &CStr) -> (OldValue, Option<&'static [u8]>) {
        let old = attrs::find(node, name).and_then(Attr::value_bytes);
        let copy = old.map(|bytes| {
            let mut copy = Vec::with_capacity(bytes.len() + 1);
            copy.extend_from_slice(bytes);
            copy.push(0);
            copy
        });
        (OldValue(copy), old)
    }

    fn as_c(&self) -> Option<&CStr> {
        self.0
            .as_deref()
            .and_then(|bytes| CStr::from_bytes_until_nul(bytes).ok())
    }
}

fn after_set(js: Option<Js>, node: Element, name: &CStr, value: &CStr, changed: bool, had: bool) {
    if changed && is(name, "type") && named(node, b"input") {
        ffi::input_resanitize_value(node);
    }
    if is(name, "open") && !had && named(node, b"details") {
        ffi::details_toggle_open(js, node, true);
    }
    if changed && is(name, "src") && named(node, b"img") {
        ffi::start_image_load(js, node, value);
    }
    if named(node, b"iframe") && (is(name, "src") || is(name, "srcdoc")) {
        ffi::schedule_iframe_load_full(js, node);
    } else if changed && named(node, b"object") && is(name, "data") {
        ffi::schedule_iframe_load(js, node);
    }
}

fn apply_set(scope: &mut Scope<'_>, node: Element, name: &CStr, value: &[u8], value_c: &CStr) {
    let (old, current) = OldValue::capture(node, name);
    let changed = current != Some(value);
    let js = ffi::js_of(scope);
    let paint_only = changed && is(name, "src") && ffi::img_src_layout_neutral(node);
    if changed {
        attrs::set_len(node, name, Some(value), value.len());
    }
    ffi::body_forward_content_handler(scope, node, name, Some(value_c));
    if let Some(js) = js.filter(|_| changed) {
        if !paint_only && ffi::attr_may_affect_style(node, name) {
            ffi::mark_mutated(js);
        }
        if paint_only {
            ffi::request_repaint(js);
        }
    }
    if let Some(js) = js {
        ffi::record_attr_change(js, node, name, old.as_c());
        ffi::ce_attr_changed(js, node, name, old.as_c(), Some(value_c));
    }
    after_set(js, node, name, value_c, changed, old.0.is_some());
}

fn checked_name<'a>(
    scope: &mut Scope<'_>,
    bytes: &[u8],
    raw: &'a CStr,
    message: &str,
) -> JsResult<&'a CStr> {
    if bytes.len() != raw.to_bytes().len() || !ffi::valid_attr_name(raw) {
        return Err(invalid_character(scope, message));
    }
    Ok(raw)
}

pub(crate) fn set_attribute(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.len() < 2 {
        return Ok(Value::undefined());
    }
    ffi::with_text(scope, arg(args, 0), |scope, bytes, raw| {
        let raw = checked_name(scope, bytes, raw, "setAttribute: invalid attribute name")?;
        let Some(node) = element(this) else {
            return Ok(Value::undefined());
        };
        let name = normalize(node, raw);
        ffi::with_text(scope, arg(args, 1), |scope, value, value_c| {
            if !is_internal(name.to_bytes()) {
                apply_set(scope, node, &name, value, value_c);
            }
            Value::undefined()
        })
    })?
}

pub(crate) fn remove_attribute(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = element(this).filter(|_| !args.is_empty()) else {
        return Ok(Value::undefined());
    };
    let raw = text_arg(scope, arg(args, 0))?;
    let name = normalize(node, &raw);
    if is_internal(name.to_bytes()) {
        return Ok(Value::undefined());
    }
    if attrs::find(node, &name).is_some() {
        let js = ffi::js_of(scope);
        ffi::remove_attr_recorded(js, node, &name);
        if is(&name, "open") && named(node, b"details") {
            ffi::details_toggle_open(js, node, false);
        }
    }
    ffi::body_forward_content_handler(scope, node, &name, None);
    Ok(Value::undefined())
}

pub(crate) fn is_same_node(_scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    Ok(Value::boolean(
        args.first().is_some_and(|other| other.same_object(this)),
    ))
}
