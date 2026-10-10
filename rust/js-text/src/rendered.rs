//! Southstar — the innerText and outerText accessors, wholeText and normalize: rendered text in, text and br nodes out, adjacent text merged.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{
    Element, FLAG_CDATA, JsResult, MAX_DEPTH, arg, element_named, inner_text, unsupported,
};

const NO_MODIFICATION_ALLOWED_ERR: i32 = 7;

pub(crate) fn get_inner_text(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    if unsupported(Some(node)) {
        return Ok(Value::undefined());
    }
    if ["iframe", "frame", "frameset"]
        .iter()
        .any(|tag| element_named(node, tag))
    {
        return Ok(scope.string(""));
    }
    let js = ffi::js_of(scope);
    ffi::flush_layout(js);
    let styles = ffi::style_table(js);
    let text = if styles.as_ptr().is_null() || inner_text::display_none(styles, node) {
        inner_text::text_content(node)
    } else {
        inner_text::rendered_text(styles, node)
    };
    Ok(scope.string_from_bytes(&text))
}

fn rendered_nodes(text: &[u8]) -> Vec<Element> {
    let is_break = |b: u8| b == b'\n' || b == b'\r';
    let mut nodes = Vec::new();
    let mut p = 0;
    while p < text.len() {
        let start = p;
        while p < text.len() && !is_break(text[p]) {
            p += 1;
        }
        if p > start {
            nodes.push(ffi::new_text(&text[start..p]));
        }
        while p < text.len() && is_break(text[p]) {
            if text[p] == b'\r' && text.get(p + 1) == Some(&b'\n') {
                p += 1;
            }
            p += 1;
            nodes.push(ffi::new_element(b"br"));
        }
    }
    nodes
}

fn setter_text(scope: &mut Scope<'_>, value: &Value) -> JsResult<Vec<u8>> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    scope.to_bytes(value)
}

pub(crate) fn set_inner_text(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this).filter(|n| !unsupported(Some(*n))) else {
        return Ok(Value::undefined());
    };
    let text = setter_text(scope, &arg(args, 0))?;
    let js = ffi::js_of(scope);
    ffi::clear_children(js, node);
    for child in rendered_nodes(&text) {
        node.append(child);
    }
    ffi::mark_mutated(js);
    Ok(Value::undefined())
}

pub(crate) fn set_outer_text(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this).filter(|n| !unsupported(Some(*n))) else {
        return Ok(Value::undefined());
    };
    let Some(parent) = node.parent() else {
        return Err(ffi::dom_exception(
            scope,
            "NoModificationAllowedError",
            NO_MODIFICATION_ALLOWED_ERR,
            "Cannot set outerText on a detached node.",
        ));
    };
    let text = setter_text(scope, &arg(args, 0))?;
    let mut nodes = rendered_nodes(&text);
    if nodes.is_empty() {
        nodes.push(ffi::new_text(b""));
    }
    let previous = node.prev_sibling();
    let next = node.next_sibling();
    let js = ffi::js_of(scope);
    for child in nodes {
        ffi::insert_sibling_before(node, child);
        ffi::record_child_change(
            js,
            parent,
            Some(child),
            None,
            child.prev_sibling(),
            child.next_sibling(),
        );
    }
    let saved_prev = node.prev_sibling();
    let saved_next = node.next_sibling();
    node.detach();
    if !js.is_null() {
        ffi::orphan_node(js, node);
        ffi::record_child_change(js, parent, None, Some(node), saved_prev, saved_next);
    }
    if let Some(before_next) = next.and_then(|n| n.prev_sibling()) {
        merge_with_next(js, before_next);
    }
    if let Some(previous) = previous {
        merge_with_next(js, previous);
    }
    ffi::mark_mutated(js);
    Ok(Value::undefined())
}

fn concat(dst: Element, src: Element) -> bool {
    let a = dst.text_with_len().unwrap_or_default();
    let b = src.text_with_len().unwrap_or_default();
    if a.len() + b.len() >= u32::MAX as usize {
        return false;
    }
    let mut merged = Vec::with_capacity(a.len() + b.len());
    merged.extend_from_slice(a);
    merged.extend_from_slice(b);
    ffi::replace_text(dst, &merged);
    true
}

fn merge_with_next(js: Js, node: Element) {
    if !node.is_text() {
        return;
    }
    let Some(next) = node.next_sibling().filter(|n| n.is_text()) else {
        return;
    };
    if !concat(node, next) {
        return;
    }
    next.detach();
    ffi::orphan_node(js, next);
}

pub(crate) fn get_whole_text(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this).filter(|n| n.is_text()) else {
        return Ok(Value::undefined());
    };
    let start = core::iter::successors(Some(node), |n| n.prev_sibling().filter(|p| p.is_text()))
        .last()
        .unwrap_or(node);
    let mut text = Vec::new();
    for run in core::iter::successors(Some(start), |n| n.next_sibling().filter(|s| s.is_text())) {
        text.extend_from_slice(run.text_with_len().unwrap_or_default());
    }
    Ok(scope.string_from_bytes(&text))
}

fn is_plain_text(node: Element) -> bool {
    node.is_text() && node.flags() & FLAG_CDATA == 0
}

fn remove_recorded(js: Js, parent: Element, child: Element) {
    let (prev, next) = (child.prev_sibling(), child.next_sibling());
    child.detach();
    ffi::orphan_node(js, child);
    ffi::record_child_change(js, parent, None, Some(child), prev, next);
}

fn normalize_walk(js: Js, node: Element, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    let mut cursor = node.first_child();
    while let Some(child) = cursor {
        let mut next = child.next_sibling();
        if is_plain_text(child) {
            if child.text().is_none_or(|t| t.is_empty()) {
                remove_recorded(js, node, child);
                cursor = next;
                continue;
            }
            while let Some(following) = next.filter(|n| is_plain_text(*n)) {
                let after = following.next_sibling();
                if !concat(child, following) {
                    break;
                }
                remove_recorded(js, node, following);
                next = after;
            }
        } else if child.is_element() {
            normalize_walk(js, child, depth + 1);
        }
        cursor = next;
    }
}

pub(crate) fn normalize(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    if let Some(node) = ffi::unwrap_node(this) {
        let js = ffi::js_of(scope);
        normalize_walk(js, node, 0);
        ffi::mark_mutated(js);
    }
    Ok(Value::undefined())
}
