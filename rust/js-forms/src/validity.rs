//! Southstar — constraint validation: ValidityState, checkValidity(), validationMessage, willValidate, setCustomValidity() and the first invalid control of a form.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{ancestors, children, controls, select};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Page, cstring};
use crate::submit::is_submit_trigger;
use crate::{
    Element, JsResult, MAX_DEPTH, c_bytes, form_owner, hidden_child, named, text_is, text_is_any,
    type_of, until_nul,
};

const CUSTOM_VALIDITY_ATTR: &CStr = c"data-nd-custom-validity";
const PATTERN_MAX_LEN: usize = 2048;
const PATTERN_VALUE_MAX_LEN: usize = 10000;
const HIDDEN_FLAG: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

#[derive(Default)]
struct Flags {
    value_missing: bool,
    type_mismatch: bool,
    pattern_mismatch: bool,
    too_long: bool,
    too_short: bool,
    range_underflow: bool,
    range_overflow: bool,
    step_mismatch: bool,
}

impl Flags {
    fn any(&self) -> bool {
        self.value_missing
            || self.type_mismatch
            || self.pattern_mismatch
            || self.too_long
            || self.too_short
            || self.range_underflow
            || self.range_overflow
            || self.step_mismatch
    }
}

fn regexp_test(scope: &mut Scope<'_>, value: &[u8], pattern: &[u8]) -> Option<bool> {
    let global = scope.global();
    let ctor = scope.get(&global, "RegExp").ok()?;
    if !scope.is_function(&ctor) {
        return None;
    }
    let flags = scope.string("v");
    let source = scope.string_from_bytes(pattern);
    if scope.construct(&ctor, &[source, flags.clone()]).is_err() {
        return Some(true);
    }
    let anchored = [&b"^(?:"[..], pattern, b")$"].concat();
    let anchored = scope.string_from_bytes(&anchored);
    let Ok(re) = scope.construct(&ctor, &[anchored, flags]) else {
        return Some(true);
    };
    let test = scope
        .get(&re, "test")
        .unwrap_or_else(|_| Value::undefined());
    let subject = scope.string_from_bytes(value);
    match scope.call(&test, &re, &[subject]) {
        Ok(result) => Some(scope.to_bool(&result)),
        Err(_) => Some(true),
    }
}

fn value_matches_pattern(value: &[u8], pattern: Option<&[u8]>) -> bool {
    let Some(pattern) = pattern.filter(|p| !p.is_empty()) else {
        return true;
    };
    if pattern.len() > PATTERN_MAX_LEN || value.len() > PATTERN_VALUE_MAX_LEN {
        return false;
    }
    if let Some(matched) = ffi::with_pattern_scope(|scope| regexp_test(scope, value, pattern)) {
        return matched;
    }
    let anchored = [&b"^(?:"[..], pattern, b")$"].concat();
    ffi::regex_matches(&anchored, value).unwrap_or(true)
}

fn is_radio(node: Element) -> bool {
    named(node, "input") && text_is(type_of(node), "radio")
}

fn radio_group_has_required(
    scan: Element,
    owner: Option<Element>,
    name: &CStr,
    depth: i32,
) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    if is_radio(scan)
        && scan.attr(c"required").is_some()
        && scan.attr(c"name") == Some(name)
        && form_owner(scan) == owner
    {
        return true;
    }
    children(scan).any(|child| radio_group_has_required(child, owner, name, depth + 1))
}

fn radio_group_required(radio: Element) -> bool {
    let Some(name) = radio.attr(c"name").filter(|n| !n.is_empty()) else {
        return false;
    };
    if radio.attr(c"required").is_some() {
        return true;
    }
    radio_group_has_required(radio.root(), form_owner(radio), name, 0)
}

fn control_value(node: Element, tag: &[u8]) -> Vec<u8> {
    match tag {
        b"textarea" => until_nul(controls::textarea_value(Some(node))),
        b"select" => {
            let option = if node.attr(c"multiple").is_some() {
                select::first_selected_option(node)
            } else {
                select::chosen_option(node)
            };
            option.map_or_else(Vec::new, |o| until_nul(select::option_value(o)))
        }
        _ => controls::used_value(node).map_or_else(Vec::new, |v| v.to_bytes().to_vec()),
    }
}

fn strtoll(text: &[u8]) -> Option<i64> {
    let start = text
        .iter()
        .position(|c| !c.is_ascii_whitespace() && *c != 0x0b)
        .unwrap_or(text.len());
    let rest = &text[start..];
    let (negative, digits) = match rest.first() {
        Some(b'-') => (true, &rest[1..]),
        Some(b'+') => (false, &rest[1..]),
        _ => (false, rest),
    };
    let len = digits.iter().take_while(|c| c.is_ascii_digit()).count();
    if len == 0 {
        return None;
    }
    let mut out: i64 = 0;
    for &d in &digits[..len] {
        let d = i64::from(d - b'0');
        out = if negative {
            out.saturating_mul(10).saturating_sub(d)
        } else {
            out.saturating_mul(10).saturating_add(d)
        };
    }
    Some(out)
}

fn utf8_len(bytes: &[u8]) -> i64 {
    bytes.iter().filter(|&&b| b & 0xc0 != 0x80).count() as i64
}

fn strip(bytes: &[u8]) -> &[u8] {
    let space = |c: &u8| c.is_ascii_whitespace() || *c == 0x0b;
    let start = bytes.iter().position(|c| !space(c)).unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|c| !space(c))
        .map_or(start, |e| e + 1);
    &bytes[start..end.max(start)]
}

fn pattern_mismatch(node: Element, ty: Option<&CStr>, value: &[u8]) -> bool {
    let Some(pattern) = node.attr(c"pattern").map(|p| p.to_bytes().to_vec()) else {
        return false;
    };
    if !controls::type_supports_text_constraints(ty) {
        return false;
    }
    if text_is(ty, "email") && node.attr(c"multiple").is_some() {
        value
            .split(|&c| c == b',')
            .any(|item| !value_matches_pattern(strip(item), Some(&pattern)))
    } else {
        !value_matches_pattern(value, Some(&pattern))
    }
}

fn type_mismatch(node: Element, ty: &CStr, value: &[u8]) -> bool {
    let ty = Some(ty);
    let text = cstring(value);
    if text_is(ty, "email") {
        !controls::email_value_valid(Some(node), Some(&text))
    } else if text_is(ty, "url") {
        !ffi::url_is_valid_absolute(value)
    } else {
        controls::type_has_number_value(ty) && controls::value_to_number(ty, Some(&text)).is_none()
    }
}

fn compute(node: Option<Element>) -> Flags {
    let mut flags = Flags::default();
    let Some((node, tag)) = node.and_then(|n| n.element_name().map(|t| (n, t))) else {
        return flags;
    };
    let is_input = tag == b"input";
    let is_textarea = tag == b"textarea";
    if !is_input && !is_textarea && tag != b"select" {
        return flags;
    }
    let ty = if is_input { type_of(node) } else { None };
    if text_is_any(ty, &["submit", "button", "reset", "image", "hidden"]) {
        return flags;
    }
    let value = control_value(node, tag);
    let needs_mutable =
        is_textarea || (is_input && !is_radio(node) && !text_is_any(ty, &["checkbox", "file"]));
    let required = if is_radio(node) {
        radio_group_required(node)
    } else {
        controls::supports_required(node)
            && node.attr(c"required").is_some()
            && (!needs_mutable
                || (!controls::effectively_disabled(node)
                    && !controls::readonly_bars_validation(node)))
    };
    let text = cstring(&value);
    if required && controls::value_missing(node, Some(&text), Some(node.root())) {
        flags.value_missing = true;
        return flags;
    }
    if value.is_empty() {
        return flags;
    }
    if let Some(ty) = ty {
        flags.type_mismatch = type_mismatch(node, ty, &value);
    }
    if is_input {
        flags.pattern_mismatch = pattern_mismatch(node, ty, &value);
    }
    if controls::length_limits_apply(node) && node.attr(c"data-nd-user-edited").is_some() {
        let length = utf8_len(&value);
        let limit = |attr: &CStr| node.attr(attr).and_then(|v| strtoll(v.to_bytes()));
        flags.too_short = limit(c"minlength").is_some_and(|min| length < min);
        flags.too_long = limit(c"maxlength").is_some_and(|max| length > max);
    }
    if is_input && ty.is_some() {
        if let Some((under, over)) = controls::value_range_state(node, Some(&text)) {
            flags.range_underflow = under;
            flags.range_overflow = over;
        }
        flags.step_mismatch = controls::value_step_mismatch(node, Some(&text));
    }
    flags
}

fn has_custom_error(node: Option<Element>) -> bool {
    node.and_then(|n| n.attr(CUSTOM_VALIDITY_ATTR))
        .is_some_and(|msg| !msg.is_empty())
}

pub(crate) fn will_validate(node: Option<Element>) -> bool {
    let Some((node, tag)) = node.and_then(|n| n.element_name().map(|t| (n, t))) else {
        return false;
    };
    let is_input = tag == b"input";
    if !matches!(tag, b"input" | b"textarea" | b"select" | b"button") {
        return false;
    }
    if tag == b"button" && !is_submit_trigger(node) {
        return false;
    }
    if controls::effectively_disabled(node) || controls::readonly_bars_validation(node) {
        return false;
    }
    if is_input && node.attr(c"readonly").is_some() {
        return false;
    }
    if ancestors(node).any(|p| named(p, "datalist")) {
        return false;
    }
    let ty = if is_input { type_of(node) } else { None };
    !text_is_any(ty, &["button", "reset", "image", "hidden"])
}

fn element_valid(node: Element) -> bool {
    !will_validate(Some(node)) || (!has_custom_error(Some(node)) && !compute(Some(node)).any())
}

fn invalid_here(form: Element, scan: Element) -> bool {
    scan.element_name().is_some() && form_owner(scan) == Some(form) && !element_valid(scan)
}

pub(crate) fn first_invalid(form: Element, scan: Element, depth: i32) -> Option<Element> {
    if depth >= MAX_DEPTH || hidden_child(scan) {
        return None;
    }
    if invalid_here(form, scan) {
        return Some(scan);
    }
    children(scan).find_map(|child| first_invalid(form, child, depth + 1))
}

fn collect_invalid(form: Element, scan: Element, depth: i32, out: &mut Vec<Element>) {
    if depth >= MAX_DEPTH || hidden_child(scan) {
        return;
    }
    if invalid_here(form, scan) {
        out.push(scan);
    }
    for child in children(scan) {
        collect_invalid(form, child, depth + 1, out);
    }
}

pub(crate) fn valid(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let value = scope
        .get(this, "__ndValid")
        .unwrap_or_else(|_| Value::undefined());
    Ok(Value::boolean(scope.to_bool(&value)))
}

pub(crate) fn validity(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let node = ffi::element(this);
    let custom = has_custom_error(node);
    let flags = compute(node);
    let is_valid = !(flags.any() || custom);
    let global = scope.global();
    let ctor = scope
        .get(&global, "ValidityState")
        .unwrap_or_else(|_| Value::undefined());
    let proto = if ctor.is_object() {
        scope
            .get(&ctor, "prototype")
            .unwrap_or_else(|_| Value::undefined())
    } else {
        Value::undefined()
    };
    let state = if proto.is_object() {
        scope.new_object_with_proto(&proto)
    } else {
        scope.new_object()
    };
    let fields = [
        ("valueMissing", flags.value_missing),
        ("typeMismatch", flags.type_mismatch),
        ("patternMismatch", flags.pattern_mismatch),
        ("tooLong", flags.too_long),
        ("tooShort", flags.too_short),
        ("rangeUnderflow", flags.range_underflow),
        ("rangeOverflow", flags.range_overflow),
        ("stepMismatch", flags.step_mismatch),
        ("badInput", false),
        ("customError", custom),
    ];
    for (name, flag) in fields {
        let _ = scope.set(&state, name, Value::boolean(flag));
    }
    let _ = scope.define(&state, "__ndValid", Value::boolean(is_valid), HIDDEN_FLAG);
    Ok(state)
}

pub(crate) fn check_validity(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let node = ffi::element(this);
    let page = Page::of(scope);
    if let Some(form) = node.filter(|n| named(*n, "form")) {
        let mut bad = Vec::new();
        collect_invalid(form, form.root(), 0, &mut bad);
        if let Some(page) = page {
            for control in &bad {
                page.dispatch_event(*control, c"invalid");
            }
        }
        return Ok(Value::boolean(bad.is_empty()));
    }
    let is_valid = !will_validate(node) || !(compute(node).any() || has_custom_error(node));
    if let (false, Some(page), Some(node)) = (is_valid, page, node) {
        page.dispatch_event(node, c"invalid");
    }
    Ok(Value::boolean(is_valid))
}

fn message(node: Element) -> &'static str {
    let flags = compute(Some(node));
    let ty = if named(node, "input") {
        type_of(node)
    } else {
        None
    };
    if flags.value_missing {
        return if text_is(ty, "checkbox") {
            "Please check this box."
        } else if text_is(ty, "radio") {
            "Please select one of these options."
        } else if named(node, "select") {
            "Please select an item in the list."
        } else {
            "Please fill out this field."
        };
    }
    if flags.type_mismatch {
        return if text_is(ty, "email") {
            "Please enter an email address."
        } else if text_is(ty, "url") {
            "Please enter a URL."
        } else {
            "Please enter a valid value."
        };
    }
    if flags.pattern_mismatch {
        "Please match the requested format."
    } else if flags.too_long {
        "Please shorten this text."
    } else if flags.too_short {
        "Please lengthen this text."
    } else if flags.range_underflow {
        "Value must be greater than or equal to the minimum."
    } else if flags.range_overflow {
        "Value must be less than or equal to the maximum."
    } else if flags.step_mismatch {
        "Please enter a valid step value."
    } else {
        ""
    }
}

pub(crate) fn validation_message(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let node = ffi::element(this).filter(|n| will_validate(Some(*n)));
    let Some(node) = node else {
        return Ok(scope.string(""));
    };
    if let Some(custom) = node.attr(CUSTOM_VALIDITY_ATTR).filter(|m| !m.is_empty()) {
        return Ok(scope.string_from_bytes(custom.to_bytes()));
    }
    Ok(scope.string(message(node)))
}

pub(crate) fn will_validate_getter(
    _scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> JsResult {
    Ok(Value::boolean(will_validate(ffi::element(this))))
}

pub(crate) fn set_custom_validity(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::element(this) else {
        return Ok(Value::undefined());
    };
    let text = match args.first() {
        Some(arg) => c_bytes(scope, arg),
        None => None,
    };
    match text.filter(|t| !t.is_empty()) {
        Some(text) => ffi::set_attr(node, CUSTOM_VALIDITY_ATTR, &text),
        None => ffi::remove_attr(node, CUSTOM_VALIDITY_ATTR),
    }
    Page::mark_mutated(Page::of(scope));
    Ok(Value::undefined())
}
