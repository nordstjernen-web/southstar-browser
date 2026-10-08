//! Southstar — HTML form submission triggers, form data collection and the first control failing constraint validation.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::ffi::{CStr, c_long};
use std::ffi::CString;

use ffi::{GStr, Node, Query};

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

fn gstr_or_empty(value: &Option<GStr>) -> &CStr {
    value.as_deref().unwrap_or(c"")
}

fn children(node: Node<'_>) -> impl Iterator<Item = Node<'_>> {
    core::iter::successors(node.first_child(), |child| child.next_sibling())
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

fn belongs_to(form: Option<Node>, control: Node, doc: Option<Node>) -> bool {
    form.is_some() && control.form_owner(doc) == form
}

fn option_disabled(option: Node) -> bool {
    if option.effectively_disabled() {
        return true;
    }
    let ancestors = core::iter::successors(option.parent(), |p| p.parent());
    for p in ancestors.take(MAX_DEPTH as usize) {
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
        return select
            .chosen_option()
            .filter(|opt| !option_disabled(*opt))
            .into_iter()
            .collect();
    }
    let mut out = Vec::new();
    for child in children(select) {
        if is_named(child, b"optgroup") {
            if child.effectively_disabled() || child.attr(c"disabled").is_some() {
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
    if type_is_any(ty, &["checkbox", "radio"]) && !node.is_checked() {
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
    let mut value = node.used_value();
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
        b"textarea" => query.append(name, Some(gstr_or_empty(&node.textarea_value()))),
        b"select" => {
            for option in selected_options(node) {
                query.append(name, Some(gstr_or_empty(&option.option_value())));
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

fn submission_name<'a>(form: Option<Node>, node: Node<'a>, doc: Option<Node>) -> Option<&'a CStr> {
    if !belongs_to(form, node, doc) {
        return None;
    }
    let name = node.attr(c"name").filter(|name| !name.is_empty())?;
    (!node.effectively_disabled()).then_some(name)
}

pub(crate) fn collect_inputs(
    form: Option<Node>,
    node: Option<Node>,
    doc: Option<Node>,
    query: &mut Query,
    submitter: Option<Node>,
    depth: i32,
) {
    let Some(node) = node else { return };
    if depth >= MAX_DEPTH {
        return;
    }
    if let Some(tag) = node.element_name().filter(|tag| SUBMITTABLE.contains(tag)) {
        if let Some(name) = submission_name(form, node, doc) {
            collect_control(node, tag, name, query, submitter);
        }
    }
    for child in children(node) {
        collect_inputs(form, Some(child), doc, query, submitter, depth + 1);
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
        return node.email_value_valid(value);
    }
    if type_is(Some(ty), "url") {
        return ffi::url_is_valid_absolute(value);
    }
    if ffi::type_has_number_value(ty) {
        return ffi::value_to_number(ty, value);
    }
    true
}

fn range_matches(node: Node, value: &CStr) -> bool {
    node.range_state(value)
        .is_none_or(|(under, over)| !under && !over)
}

fn length_matches(node: Node, value: &CStr) -> bool {
    if !node.length_limits_apply() {
        return true;
    }
    let minlen = node.attr(c"minlength");
    let maxlen = node.attr(c"maxlength");
    let length = ffi::utf8_strlen(value);
    let limit = |attr: &CStr| c_long::from(ffi::parse_int(attr, 0, 0, LENGTH_LIMIT_MAX));
    minlen.is_none_or(|minlen| length >= limit(minlen))
        && maxlen.is_none_or(|maxlen| length <= limit(maxlen))
}

fn value_valid(node: Node, is_input: bool, ty: Option<&CStr>, value: &CStr) -> bool {
    let pattern = node.attr(c"pattern");
    if is_input && ffi::type_supports_text_constraints(ty) && !pattern_matches(value, pattern) {
        return false;
    }
    if is_input
        && (!type_matches(node, value, ty)
            || !range_matches(node, value)
            || node.step_mismatch(value))
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
        b"textarea" => node.textarea_value(),
        b"select" => selected_options(node)
            .first()
            .and_then(|option| option.option_value()),
        _ => None,
    };
    let value = match tag {
        b"textarea" | b"select" => gstr_or_empty(&collected),
        _ => or_empty(node.used_value()),
    };
    let required = node.supports_required() && node.attr(c"required").is_some();
    if required && node.value_missing(value, doc) {
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
    if let Some(tag) = node.element_name().filter(|tag| VALIDATED.contains(tag)) {
        if belongs_to(form, node, doc)
            && !node.effectively_disabled()
            && !node.readonly_bars_validation()
            && control_invalid(node, tag, doc)
        {
            return Some(node);
        }
    }
    children(node).find_map(|child| first_invalid(form, Some(child), doc, depth + 1))
}
