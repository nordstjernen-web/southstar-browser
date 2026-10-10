//! Southstar — string, URL and enumerated reflection: the generic attribute accessors plus autocomplete, dir, translate, type, autocapitalize, spellcheck, htmlFor, draggable and contentEditable.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Kind, controls};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js};
use crate::tables::{self, Keywords};
use crate::{
    Element, JsResult, is_custom_element, is_named, keyword_match, name_is_any_of, normalize,
    raw_name,
};

const OWN_PROPERTY: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};

fn text(scope: &mut Scope<'_>, bytes: &[u8]) -> JsResult {
    Ok(scope.string_from_bytes(bytes))
}

fn keyword(scope: &mut Scope<'_>, kw: &CStr) -> Value {
    scope.string_from_bytes(kw.to_bytes())
}

fn is_global(name: &[u8]) -> bool {
    name.starts_with(b"aria-") || tables::GLOBAL_ATTRS.contains(&name)
}

fn reflects_name(node: Element) -> bool {
    name_is_any_of(node, tables::NAME_REFLECTING_TAGS)
}

fn null_is_empty(node: Element, attr: &[u8]) -> bool {
    tables::NULL_IS_EMPTY
        .iter()
        .find(|(elem, _)| is_named(node, elem))
        .is_some_and(|(_, attrs)| attrs.contains(&attr))
}

fn reflected_name(magic: i32) -> Option<(usize, &'static CStr)> {
    let index = usize::try_from(magic).ok()?;
    tables::REFLECTED_NAMES
        .get(index)
        .map(|&name| (index, name))
}

pub(crate) fn attr_get(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    let Some((index, name)) = reflected_name(magic) else {
        return text(scope, b"");
    };
    let Some(n) = ffi::unwrap_node(this) else {
        return text(scope, b"");
    };
    if is_custom_element(n) && !is_global(name.to_bytes()) {
        return Ok(Value::undefined());
    }
    if index == tables::NAME && !reflects_name(n) {
        return Ok(Value::undefined());
    }
    let v = ffi::attr(n, name);
    let defaults_to_document = (index == tables::ACTION && is_named(n, b"form"))
        || (index == tables::FORM_ACTION && (is_named(n, b"button") || is_named(n, b"input")));
    let js = ffi::js_of(scope);
    if defaults_to_document && v.is_none_or(|v| v.first().is_none_or(|&b| b == 0)) {
        return text(scope, ffi::current_url(js));
    }
    if tables::URL_ATTRS.contains(&index) {
        return match v {
            None => text(scope, b""),
            Some(v) => Ok(ffi::resolved_url(scope, js, v)),
        };
    }
    if let Some(norm) = normalize(name.to_bytes(), v) {
        return Ok(keyword(scope, norm));
    }
    text(scope, v.unwrap_or_default())
}

pub(crate) fn attr_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    magic: i32,
) -> JsResult<()> {
    let Some((index, name)) = reflected_name(magic) else {
        return Ok(());
    };
    let Some(n) = ffi::unwrap_node(this).filter(|n| n.kind() == Kind::Element) else {
        return Ok(());
    };
    if (is_custom_element(n) && !is_global(name.to_bytes()))
        || (index == tables::NAME && !reflects_name(n))
    {
        ffi::define_own(scope, this, name, val, OWN_PROPERTY);
        return Ok(());
    }
    let js = ffi::js_of(scope);
    if val.is_null() && null_is_empty(n, name.to_bytes()) {
        ffi::set_attr_len(js, n, name, b"");
        return Ok(());
    }
    let s = ffi::to_text(scope, val)?;
    if index == tables::SRC {
        ffi::set_src(js, n, &s);
    } else {
        ffi::set_attr_len(js, n, name, s.bytes());
    }
    Ok(())
}

pub(crate) fn reflect_get(
    scope: &mut Scope<'_>,
    this: &Value,
    attr: &CStr,
    null_if_absent: bool,
) -> Value {
    match ffi::unwrap_node(this) {
        None if null_if_absent => Value::null(),
        None => scope.string_from_bytes(b""),
        Some(n) => scope.string_from_bytes(ffi::attr(n, attr).unwrap_or_default()),
    }
}

pub(crate) fn reflect_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    attr: &CStr,
) -> JsResult<()> {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let s = ffi::to_text(scope, val)?;
    ffi::set_attr_len(ffi::js_of(scope), n, attr, s.bytes());
    Ok(())
}

fn set_flag(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    attr: &CStr,
    on: &CStr,
    off: &CStr,
) -> JsResult<()> {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let value = if scope.to_bool(val) { on } else { off };
    ffi::set_attr(ffi::js_of(scope), n, attr, value);
    Ok(())
}

fn is_autofill_space(b: &u8) -> bool {
    matches!(*b, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn autocomplete_tokens(n: Element, element: &[u8], tokens: &[&[u8]]) -> Option<Vec<u8>> {
    let first = *tokens.first()?;
    let hidden = element.eq_ignore_ascii_case(b"input")
        && ffi::attr_c(n, c"type")
            .map_or(&b""[..], CStr::to_bytes)
            .eq_ignore_ascii_case(b"hidden");
    if (first == b"on" || first == b"off") && tokens.len() == 1 && !hidden {
        return Some(first.to_vec());
    }
    let mut i = 0;
    if first.len() > 8 && first.starts_with(b"section-") {
        i += 1;
    }
    if tokens
        .get(i)
        .is_some_and(|t| *t == b"shipping" || *t == b"billing")
    {
        i += 1;
    }
    if tokens
        .get(i)
        .is_some_and(|t| tables::AUTOFILL_CONTACTS.contains(t))
    {
        i += 1;
    }
    if !tokens
        .get(i)
        .is_some_and(|t| tables::AUTOFILL_FIELDS.contains(t))
    {
        return None;
    }
    i += 1;
    if tokens.get(i).is_some_and(|t| *t == b"webauthn") {
        i += 1;
    }
    (i == tokens.len()).then(|| tokens.join(&b' '))
}

pub(crate) fn autocomplete_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some((n, element)) = ffi::unwrap_node(this).and_then(|n| raw_name(n).map(|name| (n, name)))
    else {
        return text(scope, b"");
    };
    let is_form = element.eq_ignore_ascii_case(b"form");
    let Some(raw) = ffi::attr_c(n, c"autocomplete") else {
        return text(scope, if is_form { b"on" } else { b"" });
    };
    let lower = raw.to_bytes().to_ascii_lowercase();
    let tokens: Vec<&[u8]> = lower
        .split(is_autofill_space)
        .filter(|t| !t.is_empty())
        .collect();
    if is_form {
        let off = tokens.len() == 1 && tokens[0] == b"off";
        return text(scope, if off { b"off" } else { b"on" });
    }
    let out = autocomplete_tokens(n, element, &tokens).unwrap_or_default();
    text(scope, &out)
}

pub(crate) fn autocomplete_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    reflect_set(scope, this, val, c"autocomplete")
}

const DIRECTIONS: &[&CStr] = &[c"ltr", c"rtl", c"auto"];

pub(crate) fn dir_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let canonical = ffi::unwrap_node(this)
        .and_then(|n| ffi::attr(n, c"dir"))
        .and_then(|v| keyword_match(DIRECTIONS, v));
    Ok(keyword(scope, canonical.unwrap_or(c"")))
}

pub(crate) fn dir_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    reflect_set(scope, this, val, c"dir")
}

pub(crate) fn translate_get(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(Value::boolean(true));
    };
    for node in southstar_dom::ancestors_and_self(n) {
        if node.kind() != Kind::Element {
            continue;
        }
        let Some(v) = ffi::attr_c(node, c"translate") else {
            continue;
        };
        let v = v.to_bytes();
        if v.eq_ignore_ascii_case(b"no") {
            return Ok(Value::boolean(false));
        }
        if v.is_empty() || v.eq_ignore_ascii_case(b"yes") {
            return Ok(Value::boolean(true));
        }
    }
    Ok(Value::boolean(true))
}

pub(crate) fn translate_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    set_flag(scope, this, val, c"translate", c"yes", c"no")
}

fn reflect_enum(scope: &mut Scope<'_>, n: Option<Element>, def: &Keywords) -> Value {
    let found = match n.and_then(|n| ffi::attr(n, def.attr)) {
        None => def.missing,
        Some(v) => keyword_match(def.keywords, v).or(def.invalid),
    };
    found.map_or_else(Value::null, |kw| keyword(scope, kw))
}

fn enumerated(magic: i32) -> Option<&'static Keywords> {
    usize::try_from(magic)
        .ok()
        .and_then(|i| tables::ENUMERATED.get(i))
}

pub(crate) fn enum_get(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    let Some(def) = enumerated(magic) else {
        return Ok(Value::null());
    };
    Ok(reflect_enum(scope, ffi::unwrap_node(this), def))
}

pub(crate) fn enum_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    magic: i32,
) -> JsResult<()> {
    let Some(def) = enumerated(magic) else {
        return Ok(());
    };
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let js = ffi::js_of(scope);
    if (def.nullable || def.missing.is_none()) && (val.is_null() || val.is_undefined()) {
        ffi::remove_attr(js, n, def.attr);
        return Ok(());
    }
    let s = ffi::to_text(scope, val)?;
    ffi::set_attr_len(js, n, def.attr, s.bytes());
    Ok(())
}

fn aria_name(magic: i32) -> Option<&'static CStr> {
    usize::try_from(magic)
        .ok()
        .and_then(|i| tables::ARIA_STRINGS.get(i).copied())
}

pub(crate) fn aria_get(scope: &mut Scope<'_>, this: &Value, magic: i32) -> JsResult {
    let v =
        aria_name(magic).and_then(|name| ffi::unwrap_node(this).and_then(|n| ffi::attr(n, name)));
    Ok(v.map_or_else(Value::null, |v| scope.string_from_bytes(v)))
}

pub(crate) fn aria_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    magic: i32,
) -> JsResult<()> {
    let Some(name) = aria_name(magic) else {
        return Ok(());
    };
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    if val.is_null() || val.is_undefined() {
        ffi::remove_attr(ffi::js_of(scope), n, name);
        return Ok(());
    }
    reflect_set(scope, this, val, name)
}

pub(crate) fn type_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let Some((n, element)) = ffi::unwrap_node(this).and_then(|n| raw_name(n).map(|name| (n, name)))
    else {
        return text(scope, b"");
    };
    match element {
        b"input" => Ok(reflect_enum(scope, Some(n), &tables::INPUT_TYPE)),
        b"button" if ffi::is_submit_trigger(n) => text(scope, b"submit"),
        b"button" => Ok(reflect_enum(scope, Some(n), &tables::BUTTON_TYPE)),
        b"select" if ffi::attr(n, c"multiple").is_some() => text(scope, b"select-multiple"),
        b"select" => text(scope, b"select-one"),
        b"textarea" | b"output" | b"fieldset" => text(scope, element),
        _ => text(scope, ffi::attr(n, c"type").unwrap_or_default()),
    }
}

fn autocapitalize_canonical(raw: Option<&CStr>) -> Option<&'static [u8]> {
    let raw = raw?.to_bytes();
    let is = |kw: &[u8]| raw.eq_ignore_ascii_case(kw);
    if is(b"off") || is(b"none") {
        Some(b"none")
    } else if is(b"on") || is(b"sentences") {
        Some(b"sentences")
    } else if is(b"words") {
        Some(b"words")
    } else if is(b"characters") {
        Some(b"characters")
    } else {
        None
    }
}

fn autocapitalize_of(js: Js, n: Element) -> Option<&'static [u8]> {
    if let Some(own) = autocapitalize_canonical(ffi::attr_c(n, c"autocapitalize")) {
        return Some(own);
    }
    if !name_is_any_of(n, tables::AUTOCAPITALIZE_INHERITING) {
        return None;
    }
    let form = ffi::form_owner(js, n)?;
    autocapitalize_canonical(ffi::attr_c(form, c"autocapitalize"))
}

pub(crate) fn autocapitalize_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let js = ffi::js_of(scope);
    let value = ffi::unwrap_node(this).and_then(|n| autocapitalize_of(js, n));
    text(scope, value.unwrap_or_default())
}

pub(crate) fn autocapitalize_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let s = ffi::to_text(scope, val)?;
    ffi::set_attr(ffi::js_of(scope), n, c"autocapitalize", s.c_str());
    Ok(())
}

pub(crate) fn spellcheck_get(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(Value::boolean(
        ffi::unwrap_node(this).is_some_and(controls::spellcheck_used),
    ))
}

pub(crate) fn spellcheck_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    set_flag(scope, this, val, c"spellcheck", c"true", c"false")
}

pub(crate) fn html_for_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let foreign = southstar_dom::FLAG_SVG_NS | southstar_dom::FLAG_FOREIGN_NS;
    let Some(n) = ffi::unwrap_node(this).filter(|n| n.flags() & foreign == 0) else {
        return Ok(Value::undefined());
    };
    if is_named(n, b"output") {
        return Ok(ffi::token_list(scope, this, c"for"));
    }
    if !is_named(n, b"label") && !is_named(n, b"script") {
        return Ok(Value::undefined());
    }
    Ok(reflect_get(scope, this, c"for", false))
}

pub(crate) fn html_for_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    reflect_set(scope, this, val, c"for")
}

fn draggable_by_default(n: Element) -> bool {
    let Some(name) = n.element_name() else {
        return false;
    };
    if name.eq_ignore_ascii_case(b"img") {
        return true;
    }
    name.eq_ignore_ascii_case(b"a") && ffi::attr_c(n, c"href").is_some_and(|href| !href.is_empty())
}

pub(crate) fn draggable_get(_scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let n = ffi::unwrap_node(this);
    if let Some(v) = n.and_then(|n| ffi::attr_c(n, c"draggable")) {
        let v = v.to_bytes();
        if v.eq_ignore_ascii_case(b"true") {
            return Ok(Value::boolean(true));
        }
        if v.eq_ignore_ascii_case(b"false") {
            return Ok(Value::boolean(false));
        }
    }
    Ok(Value::boolean(n.is_some_and(draggable_by_default)))
}

pub(crate) fn draggable_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    set_flag(scope, this, val, c"draggable", c"true", c"false")
}

pub(crate) fn content_editable_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let state: &[u8] = match ffi::unwrap_node(this).and_then(|n| ffi::attr_c(n, c"contenteditable"))
    {
        None => b"inherit",
        Some(v) => {
            let v = v.to_bytes();
            if v.is_empty() || v.eq_ignore_ascii_case(b"true") {
                b"true"
            } else if v.eq_ignore_ascii_case(b"false") {
                b"false"
            } else if v.eq_ignore_ascii_case(b"plaintext-only") {
                b"plaintext-only"
            } else {
                b"inherit"
            }
        }
    };
    text(scope, state)
}

pub(crate) fn content_editable_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
) -> JsResult<()> {
    let Some(n) = ffi::unwrap_node(this) else {
        return Ok(());
    };
    let s = ffi::to_text(scope, val)?;
    let js = ffi::js_of(scope);
    let v = s.c_str().to_bytes();
    if v.eq_ignore_ascii_case(b"inherit") {
        ffi::remove_attr(js, n, c"contenteditable");
    } else if v.eq_ignore_ascii_case(b"true") {
        ffi::set_attr(js, n, c"contenteditable", c"true");
    } else if v.eq_ignore_ascii_case(b"false") {
        ffi::set_attr(js, n, c"contenteditable", c"false");
    } else if v.eq_ignore_ascii_case(b"plaintext-only") {
        ffi::set_attr(js, n, c"contenteditable", c"plaintext-only");
    } else {
        return Err(ffi::dom_exception(
            scope,
            c"SyntaxError",
            12,
            c"The value provided is not a valid contentEditable state.",
        ));
    }
    Ok(())
}

pub(crate) fn id_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(reflect_get(scope, this, c"id", true))
}

pub(crate) fn id_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    reflect_set(scope, this, val, c"id")
}

pub(crate) fn class_name_get(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    Ok(reflect_get(scope, this, c"class", true))
}

pub(crate) fn class_name_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    reflect_set(scope, this, val, c"class")
}

pub(crate) fn sizes_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    reflect_set(scope, this, val, c"sizes")
}

pub(crate) fn sandbox_set(scope: &mut Scope<'_>, this: &Value, val: &Value) -> JsResult<()> {
    reflect_set(scope, this, val, c"sandbox")
}
