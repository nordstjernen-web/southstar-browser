//! Southstar —EventTarget and event dispatch: listener registration and its options, the capture, target and bubble path, event handler attributes and properties, error reporting and unhandled promise rejections.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod dispatch;
mod errors;
mod ffi;
mod handlers;
mod listeners;
mod target;
mod ui_events;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use southstar_dom::{Kind, Node, NsNode};
use southstar_js_engine::{Scope, Value};

use crate::errors::Rejection;
use crate::ffi::Js;
use crate::listeners::Listener;

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) const FLAG_FRAGMENT: u32 = 1 << 2;
pub(crate) const FLAG_HAS_LISTENERS: u32 = 1 << 19;

pub(crate) struct DispatchPath {
    pub nodes: *const *const NsNode,
    pub len: usize,
    pub window: bool,
}

#[derive(Default)]
pub(crate) struct Page {
    pub listeners: HashMap<usize, Vec<Rc<Listener>>>,
    pub paths: Vec<DispatchPath>,
    pub spare_paths: Vec<Vec<*const NsNode>>,
    pub spare_flags: Vec<Vec<bool>>,
    pub spare_snapshots: Vec<Vec<Rc<Listener>>>,
    pub pending_rejections: HashMap<usize, Rejection>,
    pub reported_rejections: HashMap<usize, Rejection>,
    pub rejection_seq: u64,
    pub in_error_report: bool,
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Page>> = RefCell::new(HashMap::new());
}

pub(crate) fn with<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> R {
    PAGES.with(|pages| f(pages.borrow_mut().entry(js).or_default()))
}

pub(crate) fn peek<R: Default>(js: Js, f: impl FnOnce(&Page) -> R) -> R {
    PAGES
        .try_with(|pages| pages.try_borrow().ok()?.get(&js).map(f))
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub(crate) fn take_snapshot(js: Js) -> Vec<Rc<Listener>> {
    with(js, |page| page.spare_snapshots.pop()).unwrap_or_default()
}

pub(crate) fn return_snapshot(js: Js, mut snapshot: Vec<Rc<Listener>>) {
    snapshot.clear();
    with(js, |page| page.spare_snapshots.push(snapshot));
}

pub(crate) fn forget_node(js: Js, node: *const NsNode) {
    let dropped = PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        pages
            .get_mut(&js)
            .and_then(|page| page.listeners.remove(&(node as usize)))
    });
    if let Some(dropped) = dropped {
        for listener in &dropped {
            listener.kill();
        }
    }
}

pub(crate) fn reset(js: Js) {
    let (listeners, pending, reported) = with(js, |page| {
        (
            std::mem::take(&mut page.listeners),
            std::mem::take(&mut page.pending_rejections),
            std::mem::take(&mut page.reported_rejections),
        )
    });
    for listener in listeners.values().flatten() {
        listener.kill();
    }
    drop(listeners);
    drop(pending);
    drop(reported);
}

pub(crate) fn teardown_listeners(js: Js) {
    let listeners = with(js, |page| {
        page.spare_snapshots.clear();
        std::mem::take(&mut page.listeners)
    });
    for listener in listeners.values().flatten() {
        listener.kill();
    }
    drop(listeners);
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES.with(|pages| pages.borrow_mut().remove(&js));
    drop(page);
}

pub(crate) fn listener_count(js: Js) -> usize {
    peek(js, |page| page.listeners.values().map(Vec::len).sum())
}

pub(crate) fn is_named(node: Option<Element>, name: &[u8]) -> bool {
    node.is_some_and(|n| n.element_name() == Some(name))
}

pub(crate) fn is_document(node: Element) -> bool {
    node.kind() == Kind::Document
}

pub(crate) fn is_real_document(node: Element) -> bool {
    is_document(node) && node.flags() & FLAG_FRAGMENT == 0
}

pub(crate) fn nearest_document(node: Option<Element>) -> Option<Element> {
    let mut cur = node;
    while let Some(n) = cur {
        if is_document(n) {
            return Some(n);
        }
        cur = n.parent();
    }
    None
}

const KEY_BUFFER: usize = 96;

pub(crate) fn with_key<R>(parts: &[&str], f: impl FnOnce(&str) -> R) -> R {
    let total: usize = parts.iter().map(|part| part.len()).sum();
    if total > KEY_BUFFER {
        return f(&parts.concat());
    }
    let mut buffer = [0u8; KEY_BUFFER];
    let mut at = 0;
    for part in parts {
        buffer[at..at + part.len()].copy_from_slice(part.as_bytes());
        at += part.len();
    }
    match core::str::from_utf8(&buffer[..at]) {
        Ok(key) => f(key),
        Err(_) => f(&parts.concat()),
    }
}

pub(crate) fn with_c_key<R>(parts: &[&str], f: impl FnOnce(&core::ffi::CStr) -> R) -> R {
    let total: usize = parts.iter().map(|part| part.len()).sum();
    let mut buffer = [0u8; KEY_BUFFER];
    if total >= KEY_BUFFER || parts.iter().any(|part| part.contains('\0')) {
        let owned = std::ffi::CString::new(parts.concat().replace('\0', " ")).unwrap_or_default();
        return f(&owned);
    }
    let mut at = 0;
    for part in parts {
        buffer[at..at + part.len()].copy_from_slice(part.as_bytes());
        at += part.len();
    }
    match core::ffi::CStr::from_bytes_until_nul(&buffer[..=at]) {
        Ok(key) => f(key),
        Err(_) => f(c""),
    }
}

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn set(scope: &mut Scope<'_>, object: &Value, key: &str, value: Value) {
    let _ = scope.set(object, key, value);
}

pub(crate) fn truthy(scope: &mut Scope<'_>, object: &Value, key: &str) -> bool {
    match scope.get(object, key) {
        Ok(value) => scope.to_bool(&value),
        Err(_) => false,
    }
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

pub(crate) fn same_object(a: &Value, b: &Value) -> bool {
    a.same_object(b)
}

pub(crate) fn message_of(scope: &mut Scope<'_>, exception: &Value) -> Option<String> {
    scope.to_string(exception).ok()
}
