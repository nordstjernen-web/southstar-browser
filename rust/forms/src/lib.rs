//! Southstar — HTML form submission triggers, form data collection and the first control failing constraint validation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::{CStr, c_long};
use std::ffi::CString;

use ffi::Query;
use southstar_dom::{Node, ancestors, children, controls, select};

const MAX_DEPTH: i32 = 512;
const PATTERN_MAX_LEN: usize = 2048;
const PATTERN_VALUE_MAX_LEN: usize = 10000;
const LENGTH_LIMIT_MAX: i32 = 1_000_000;
const CUSTOM_VALIDITY_ATTR: &CStr = c"data-nd-custom-validity";
const SUBMITTABLE: [&[u8]; 4] = [b"input", b"textarea", b"select", b"button"];
const VALIDATED: [&[u8]; 3] = [b"input", b"textarea", b"select"];

fn type_is(ty: Option<&CStr>, name: &str) -> bool {
    ty.is_some_and(|t| t.to_bytes().eq_ignore_ascii_case(name.as_bytes()))
}

fn type_is_any(ty: Option<&CStr>, names: &[&str]) -> bool {
    names.iter().any(|name| type_is(ty, name))
}

fn is_named(node: Node, tag: &[u8]) -> bool {
    node.element_name() == Some(tag)
}

fn or_empty(value: Option<&CStr>) -> &CStr {
    value.unwrap_or(c"")
}

fn option_value(option: Node) -> CString {
    CString::new(select::option_value(option)).unwrap_or_default()
}

fn textarea_value(node: Node) -> CString {
    CString::new(controls::textarea_value(Some(node))).unwrap_or_default()
}

pub(crate) fn is_submit_trigger(node: Node) -> bool {
    match node.element_name() {
        Some(b"button") => {
            let ty = node.attr(c"type");
            ty.is_none() || type_is(ty, "submit")
        }
        Some(b"input") => type_is_any(node.attr(c"type"), &["submit", "image"]),
        _ => false,
    }
}

pub(crate) fn is_reset_trigger(node: Node) -> bool {
    let Some(name) = node.element_name() else {
        return false;
    };
    type_is(node.attr(c"type"), "reset") && (name == b"button" || name == b"input")
}

fn belongs_to(form: Option<Node>, control: Node) -> bool {
    form.is_some() && controls::form_owner(control) == form
}

fn option_disabled(option: Node) -> bool {
    if controls::effectively_disabled(option) {
        return true;
    }
    for p in ancestors(option).take(MAX_DEPTH as usize) {
        if is_named(p, b"select") {
            return false;
        }
        if is_named(p, b"optgroup") && p.attr(c"disabled").is_some() {
            return true;
        }
    }
    false
}

fn selected_option_node(option: Node) -> bool {
    is_named(option, b"option") && option.attr(c"selected").is_some() && !option_disabled(option)
}

fn selected_options(select: Node) -> Vec<Node> {
    if select.attr(c"multiple").is_none() {
        return select::chosen_option(select)
            .filter(|opt| !option_disabled(*opt))
            .into_iter()
            .collect();
    }
    let mut out = Vec::new();
    for child in children(select) {
        if is_named(child, b"optgroup") {
            if controls::effectively_disabled(child) || child.attr(c"disabled").is_some() {
                continue;
            }
            out.extend(children(child).filter(|option| selected_option_node(*option)));
        } else if selected_option_node(child) {
            out.push(child);
        }
    }
    out
}

fn suffixed(name: &CStr, suffix: &[u8]) -> CString {
    let mut bytes = name.to_bytes().to_vec();
    bytes.extend_from_slice(suffix);
    CString::new(bytes).unwrap_or_default()
}

fn collect_input(node: Node, name: &CStr, query: &mut Query, submitter: Option<Node>) {
    let ty = node.attr(c"type");
    if type_is_any(ty, &["checkbox", "radio"]) && !controls::is_checked(node) {
        return;
    }
    if type_is(ty, "submit") {
        if Some(node) == submitter {
            query.append(name, Some(or_empty(node.attr(c"value"))));
        }
        return;
    }
    if type_is(ty, "image") {
        if Some(node) == submitter {
            query.append(&suffixed(name, b".x"), Some(c"0"));
            query.append(&suffixed(name, b".y"), Some(c"0"));
        }
        return;
    }
    if type_is_any(ty, &["button", "reset", "file"]) {
        return;
    }
    let mut value = controls::used_value(node);
    if value.is_none() && type_is_any(ty, &["checkbox", "radio"]) {
        value = Some(c"on");
    }
    query.append(name, value);
}

fn collect_control(
    node: Node,
    tag: &[u8],
    name: &CStr,
    query: &mut Query,
    submitter: Option<Node>,
) {
    match tag {
        b"input" => collect_input(node, name, query, submitter),
        b"textarea" => query.append(name, Some(&textarea_value(node))),
        b"select" => {
            for option in selected_options(node) {
                query.append(name, Some(&option_value(option)));
            }
        }
        _ => {
            let ty = node.attr(c"type");
            let acts_as_submit = ty.is_none() || type_is(ty, "submit");
            if acts_as_submit && Some(node) == submitter {
                query.append(name, Some(or_empty(node.attr(c"value"))));
            }
        }
    }
}

fn submission_name<'a>(form: Option<Node>, node: Node<'a>) -> Option<&'a CStr> {
    if !belongs_to(form, node) {
        return None;
    }
    let name = node.attr(c"name").filter(|name| !name.is_empty())?;
    (!controls::effectively_disabled(node)).then_some(name)
}

pub(crate) fn collect_inputs(
    form: Option<Node>,
    node: Option<Node>,
    query: &mut Query,
    submitter: Option<Node>,
    depth: i32,
) {
    let Some(node) = node else { return };
    if depth >= MAX_DEPTH {
        return;
    }
    if let Some(tag) = node.element_name().filter(|tag| SUBMITTABLE.contains(tag))
        && let Some(name) = submission_name(form, node)
    {
        collect_control(node, tag, name, query, submitter);
    }
    for child in children(node) {
        collect_inputs(form, Some(child), query, submitter, depth + 1);
    }
}

fn pattern_matches(value: &CStr, pattern: Option<&CStr>) -> bool {
    let Some(pattern) = pattern.filter(|p| !p.is_empty()) else {
        return true;
    };
    if pattern.to_bytes().len() > PATTERN_MAX_LEN || value.to_bytes().len() > PATTERN_VALUE_MAX_LEN
    {
        return false;
    }
    let mut anchored = b"^(?:".to_vec();
    anchored.extend_from_slice(pattern.to_bytes());
    anchored.extend_from_slice(b")$");
    let anchored = CString::new(anchored).unwrap_or_default();
    ffi::regex_matches(&anchored, value).unwrap_or(true)
}

fn type_matches(node: Node, value: &CStr, ty: Option<&CStr>) -> bool {
    let Some(ty) = ty else { return true };
    if value.is_empty() {
        return true;
    }
    if type_is(Some(ty), "email") {
        return controls::email_value_valid(Some(node), Some(value));
    }
    if type_is(Some(ty), "url") {
        return ffi::url_is_valid_absolute(value);
    }
    if controls::type_has_number_value(Some(ty)) {
        return controls::value_to_number(Some(ty), Some(value)).is_some();
    }
    true
}

fn range_matches(node: Node, value: &CStr) -> bool {
    controls::value_range_state(node, Some(value)).is_none_or(|(under, over)| !under && !over)
}

fn length_matches(node: Node, value: &CStr) -> bool {
    if !controls::length_limits_apply(node) {
        return true;
    }
    let minlen = node.attr(c"minlength");
    let maxlen = node.attr(c"maxlength");
    let length = ffi::utf8_strlen(value);
    let limit = |attr: &CStr| c_long::from(controls::parse_int(Some(attr), 0, 0, LENGTH_LIMIT_MAX));
    minlen.is_none_or(|minlen| length >= limit(minlen))
        && maxlen.is_none_or(|maxlen| length <= limit(maxlen))
}

fn value_valid(node: Node, is_input: bool, ty: Option<&CStr>, value: &CStr) -> bool {
    let pattern = node.attr(c"pattern");
    if is_input && controls::type_supports_text_constraints(ty) && !pattern_matches(value, pattern)
    {
        return false;
    }
    if is_input
        && (!type_matches(node, value, ty)
            || !range_matches(node, value)
            || controls::value_step_mismatch(node, Some(value)))
    {
        return false;
    }
    length_matches(node, value)
}

fn control_invalid(node: Node, tag: &[u8], doc: Option<Node>) -> bool {
    let is_input = tag == b"input";
    let ty = if is_input { node.attr(c"type") } else { None };
    if type_is_any(ty, &["submit", "button", "reset", "image", "hidden"]) {
        return false;
    }
    if node
        .attr(CUSTOM_VALIDITY_ATTR)
        .is_some_and(|custom| !custom.is_empty())
    {
        return true;
    }
    let collected = match tag {
        b"textarea" => Some(textarea_value(node)),
        b"select" => selected_options(node)
            .first()
            .map(|option| option_value(*option)),
        _ => None,
    };
    let value = match tag {
        b"textarea" | b"select" => collected.as_deref().unwrap_or(c""),
        _ => or_empty(controls::used_value(node)),
    };
    let required = controls::supports_required(node) && node.attr(c"required").is_some();
    if required && controls::value_missing(node, Some(value), doc) {
        return true;
    }
    !value.is_empty() && !value_valid(node, is_input, ty, value)
}

pub(crate) fn first_invalid<'a>(
    form: Option<Node<'a>>,
    node: Option<Node<'a>>,
    doc: Option<Node<'a>>,
    depth: i32,
) -> Option<Node<'a>> {
    let node = node?;
    if depth >= MAX_DEPTH {
        return None;
    }
    if let Some(tag) = node.element_name().filter(|tag| VALIDATED.contains(tag))
        && belongs_to(form, node)
        && !controls::effectively_disabled(node)
        && !controls::readonly_bars_validation(node)
        && control_invalid(node, tag, doc)
    {
        return Some(node);
    }
    children(node).find_map(|child| first_invalid(form, Some(child), doc, depth + 1))
}
