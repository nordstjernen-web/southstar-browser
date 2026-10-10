//! Southstar — focus, click activation, fullscreen and pointer lock: the focused element and document, focus events, sequential navigation, click(), requestFullscreen and requestPointerLock.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod click;
mod ffi;
mod focus;
mod fullscreen;

use std::cell::RefCell;
use std::collections::HashMap;

use southstar_dom::{Kind, MAX_DEPTH, Node, ancestors_and_self};
use southstar_js_engine::Value;

use crate::ffi::Js;

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

const FLAG_TEMPLATE_CONTENT: u32 = 1 << 4;

pub(crate) type Guard = [Option<Element>; 4];

#[derive(Default)]
pub(crate) struct Page {
    pub focused: Option<Element>,
    pub focused_doc: Option<Element>,
    pub nav_start: Option<Element>,
    pub guards: Vec<Guard>,
    pub pointer_input: bool,
    pub pointer_lock: usize,
    pub fullscreen_target: Option<Element>,
    pub fullscreen_resolve: Option<Value>,
    pub synthetic_clicks: i32,
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

pub(crate) fn focused(js: Js) -> Option<Element> {
    peek(js, |page| page.focused)
}

pub(crate) fn forget_node(js: Js, n: Element) {
    let slot = |s: &mut Option<Element>| {
        if *s == Some(n) {
            *s = None;
        }
    };
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let Some(page) = pages.get_mut(&js) else {
            return;
        };
        slot(&mut page.nav_start);
        slot(&mut page.focused);
        slot(&mut page.focused_doc);
        for guard in &mut page.guards {
            guard.iter_mut().for_each(slot);
        }
    });
}

pub(crate) fn forget_subtree(js: Js, root: Element) {
    with(js, |page| {
        if page
            .fullscreen_target
            .is_some_and(|t| ancestors_and_self(t).any(|p| p == root))
        {
            page.fullscreen_target = None;
        }
    });
}

pub(crate) fn reset(js: Js) {
    with(js, |page| {
        page.focused = None;
        page.focused_doc = None;
        page.fullscreen_target = None;
    });
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES.with(|pages| pages.borrow_mut().remove(&js));
    drop(page);
}

pub(crate) fn is_named(n: Option<Element>, name: &[u8]) -> bool {
    n.is_some_and(|n| n.element_name() == Some(name))
}

pub(crate) fn has_attr(n: Element, name: &core::ffi::CStr) -> bool {
    n.attr(name).is_some()
}

pub(crate) fn attr_is(n: Element, name: &core::ffi::CStr, value: &[u8]) -> bool {
    n.attr(name)
        .is_some_and(|v| v.to_bytes().eq_ignore_ascii_case(value))
}

pub(crate) fn owner_document(n: Element) -> Option<Element> {
    ancestors_and_self(n).find(|p| p.kind() == Kind::Document)
}

pub(crate) fn top_document(doc: Option<Element>) -> Option<Element> {
    let mut doc = doc?;
    while let Some(parent) = doc.parent() {
        doc = owner_document(parent)?;
    }
    Some(doc)
}

pub(crate) fn inclusive_ancestor(root: Option<Element>, desc: Option<Element>) -> bool {
    let (Some(root), Some(desc)) = (root, desc) else {
        return false;
    };
    if root == desc {
        return true;
    }
    let mut depth = 0;
    let mut p = desc.parent();
    while let Some(n) = p {
        if depth >= MAX_DEPTH {
            return false;
        }
        depth += 1;
        if n == root {
            return true;
        }
        p = n.parent();
    }
    false
}

pub(crate) fn in_template_content(n: Element) -> bool {
    ancestors_and_self(n)
        .any(|p| p.flags() & FLAG_TEMPLATE_CONTENT != 0 || is_named(Some(p), b"template"))
}
