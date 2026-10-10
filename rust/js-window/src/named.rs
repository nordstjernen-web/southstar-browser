//! Southstar — named access on the window and document: child frames by index and name, elements by id on the window, and the legacy named forms and images on the document.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::cell::Cell;
use std::ffi::CString;

use southstar_dom::{Kind, Node, ancestors, children, index};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi;
use crate::text_of;

const MAX_FRAME_DEPTH: i32 = 256;

thread_local! {
    static SUSPENDED: Cell<u32> = const { Cell::new(0) };
}

pub(crate) fn suspend() {
    SUSPENDED.with(|depth| depth.set(depth.get() + 1));
}

pub(crate) fn resume() {
    SUSPENDED.with(|depth| depth.set(depth.get().saturating_sub(1)));
}

fn named_document(scope: &mut Scope<'_>, window: &Value) -> Option<Node<'static>> {
    if SUSPENDED.with(Cell::get) > 0 {
        return None;
    }
    suspend();
    let doc = ffi::current_document_for(scope, window);
    resume();
    doc
}

fn is_frame_owner(node: Node<'_>) -> bool {
    node.element_name().is_some_and(|name| {
        name.eq_ignore_ascii_case(b"iframe") || name.eq_ignore_ascii_case(b"frame")
    })
}

fn frame_matches(frame: Node<'_>, index: &mut u32, name: Option<&CStr>) -> bool {
    match name {
        Some(name) => frame.attr(c"name") == Some(name),
        None => {
            let hit = *index == 0;
            *index = index.wrapping_sub(1);
            hit
        }
    }
}

fn walk_child_frame<'a>(
    node: Node<'a>,
    index: &mut u32,
    name: Option<&CStr>,
    depth: i32,
) -> Option<Node<'a>> {
    if depth >= MAX_FRAME_DEPTH {
        return None;
    }
    for child in children(node) {
        if is_frame_owner(child) {
            if frame_matches(child, index, name) {
                return Some(child);
            }
            continue;
        }
        if let Some(hit) = walk_child_frame(child, index, name, depth + 1) {
            return Some(hit);
        }
    }
    None
}

fn indexed_iframe(doc: Node<'_>, mut index: u32, name: Option<&CStr>) -> Option<Node<'static>> {
    index::tag_lookup(doc, c"iframe", |iframes| {
        (0..iframes.len()).map(|i| iframes.get(i)).find(|&frame| {
            let owner = ancestors(frame).find(|a| a.kind() == Kind::Document);
            owner.is_some_and(|owner| owner.as_ptr() == doc.as_ptr())
                && frame_matches(frame, &mut index, name)
        })
    })
    .flatten()
}

pub(crate) fn child_frame<'a>(doc: Node<'a>, index: u32, name: Option<&CStr>) -> Option<Node<'a>> {
    if name.is_some_and(CStr::is_empty) {
        return None;
    }
    if doc.tag_table().is_some() && index::find_first_element(doc, c"frame").is_none() {
        return indexed_iframe(doc, index, name);
    }
    let mut index = index;
    walk_child_frame(doc, &mut index, name, 0)
}

fn count_child_frames(node: Node<'_>, depth: i32) -> u32 {
    if depth >= MAX_FRAME_DEPTH {
        return 0;
    }
    children(node)
        .map(|child| {
            if is_frame_owner(child) {
                1
            } else {
                count_child_frames(child, depth + 1)
            }
        })
        .sum()
}

pub(crate) fn child_frame_count(doc: Option<Node<'_>>) -> u32 {
    doc.map_or(0, |doc| count_child_frames(doc, 0))
}

fn array_index(key: &[u8]) -> Option<u32> {
    if key.is_empty() || key.len() > 10 || !key.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if key.len() > 1 && key[0] == b'0' {
        return None;
    }
    let value = key
        .iter()
        .fold(0u64, |n, &digit| n * 10 + u64::from(digit - b'0'));
    u32::try_from(value).ok().filter(|&n| n != u32::MAX)
}

fn object_or_undefined(value: Value) -> Value {
    if value.is_object() {
        value
    } else {
        Value::undefined()
    }
}

pub(crate) fn named_property(scope: &mut Scope<'_>, window: &Value, key: &Value) -> Value {
    let Some(doc) = named_document(scope, window) else {
        return Value::undefined();
    };
    if !key.is_string() {
        return Value::undefined();
    }
    let Some(key) = text_of(scope, key) else {
        return Value::undefined();
    };
    if let Some(index) = array_index(&key) {
        return object_or_undefined(ffi::child_frame_window(scope, doc, index, None));
    }
    let Ok(name) = CString::new(key) else {
        return Value::undefined();
    };
    let window = ffi::child_frame_window(scope, doc, 0, Some(&name));
    if window.is_object() {
        return window;
    }
    match index::find_by_id(doc, &name) {
        Some(element) => ffi::wrap(scope, element),
        None => Value::undefined(),
    }
}

const LEGACY_NAMED: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

fn define_missing(scope: &mut Scope<'_>, object: &Value, name: Option<&CStr>, value: &Value) {
    let Some(name) = name.filter(|name| !name.is_empty()) else {
        return;
    };
    let name = String::from_utf8_lossy(name.to_bytes());
    if !matches!(scope.has_property(object, &name), Ok(true)) {
        let _ = scope.define(object, &name, value.clone(), LEGACY_NAMED);
    }
}

fn expose_legacy_named_in(scope: &mut Scope<'_>, document: &Value, node: Node<'_>, depth: i32) {
    if depth >= southstar_dom::MAX_DEPTH {
        return;
    }
    match node.element_name() {
        Some(b"form") => {
            let element = ffi::wrap(scope, node);
            define_missing(scope, document, node.attr(c"name"), &element);
        }
        Some(b"img") => {
            if let Some(name) = node.attr(c"name").filter(|name| !name.is_empty()) {
                let element = ffi::wrap(scope, node);
                define_missing(scope, document, node.attr(c"id"), &element);
                define_missing(scope, document, Some(name), &element);
            }
        }
        _ => {}
    }
    for child in children(node) {
        expose_legacy_named_in(scope, document, child, depth + 1);
    }
}

pub(crate) fn expose_legacy_named(scope: &mut Scope<'_>, root: Node<'_>, document: &Value) {
    if document.is_object() {
        expose_legacy_named_in(scope, document, root, 0);
    }
}
