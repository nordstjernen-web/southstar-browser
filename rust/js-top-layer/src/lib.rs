//! Southstar — the top layer: popovers, dialogs, close watchers, invoker commands, focusing steps and exclusive details groups.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod details;
mod dialog;
mod ffi;
mod focus;
mod invoker;
mod popover;

use core::ffi::CStr;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, Node};
use southstar_js_engine::Value;

use crate::ffi::Js;

pub(crate) type Element = Node<'static>;
pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) const MAX_WALK: i32 = 4096;
pub(crate) const MAX_DEPTH: i32 = 512;
const FLAG_FRAGMENT: u32 = 1 << 2;

#[derive(Clone, Copy, Default)]
pub(crate) struct Toggle {
    pub timer: i32,
    pub old_open: bool,
    pub new_open: bool,
    pub source: Option<Element>,
}

#[derive(Default)]
pub(crate) struct Info {
    pub hiding: bool,
    pub type_changing: i32,
    pub trigger: Option<Element>,
    pub prev_focus: Option<Element>,
    pub has_prev_focus: bool,
    pub popover_toggle: Toggle,
    pub dialog_toggle: Toggle,
    pub dialog_modal: bool,
    pub dialog_prev_focus: Option<Element>,
    pub enable_request_close: bool,
    pub running_cancel: bool,
    pub request_rv: Option<CString>,
    pub request_source: Option<Element>,
}

pub(crate) struct AttrRef {
    pub owner: Element,
    pub attr: &'static CStr,
    pub target: Element,
}

#[derive(Default)]
pub(crate) struct Page {
    pub info: HashMap<usize, Info>,
    pub auto: Vec<Element>,
    pub hint: Vec<Element>,
    pub close_watchers: Vec<Element>,
    pub modal_dialogs: Vec<Element>,
    pub attr_refs: Vec<AttrRef>,
    pub hint_parent: Option<Element>,
    pub popover_pointerdown: Option<Element>,
    pub dialog_pointerdown: Option<Element>,
    pub showing: bool,
    pub hiding_count: i32,
}

impl Page {
    pub fn stack(&self, hint: bool) -> &Vec<Element> {
        if hint { &self.hint } else { &self.auto }
    }

    pub fn stack_mut(&mut self, hint: bool) -> &mut Vec<Element> {
        if hint { &mut self.hint } else { &mut self.auto }
    }
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Page>> = RefCell::new(HashMap::new());
}

pub(crate) fn with<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> R {
    PAGES.with(|pages| f(pages.borrow_mut().entry(js).or_default()))
}

pub(crate) fn peek<R>(js: Js, f: impl FnOnce(&Page) -> R) -> Option<R> {
    PAGES
        .try_with(|pages| pages.try_borrow().ok()?.get(&js).map(f))
        .ok()
        .flatten()
}

pub(crate) fn key(el: Element) -> usize {
    el.as_ptr() as usize
}

pub(crate) fn has_info(js: Js, el: Element) -> bool {
    peek(js, |page| page.info.contains_key(&key(el))).unwrap_or(false)
}

pub(crate) fn info_peek<R>(js: Js, el: Element, f: impl FnOnce(&Info) -> R) -> Option<R> {
    peek(js, |page| page.info.get(&key(el)).map(f)).flatten()
}

pub(crate) fn info<R>(js: Js, el: Element, f: impl FnOnce(&mut Info) -> R) -> R {
    if !has_info(js, el) {
        ffi::note(el);
    }
    with(js, |page| f(page.info.entry(key(el)).or_default()))
}

pub(crate) fn in_stack(js: Js, hint: bool, el: Element) -> bool {
    peek(js, |page| page.stack(hint).contains(&el)).unwrap_or(false)
}

pub(crate) fn remove_from(list: &mut Vec<Element>, el: Element) -> bool {
    match list.iter().position(|p| *p == el) {
        Some(index) => {
            list.remove(index);
            true
        }
        None => false,
    }
}

pub(crate) fn forget_node(js: Js, n: Element) {
    let forgotten = PAGES
        .try_with(|pages| {
            let mut pages = pages.try_borrow_mut().ok()?;
            let page = pages.get_mut(&js)?;
            remove_from(&mut page.auto, n);
            remove_from(&mut page.hint, n);
            for slot in [
                &mut page.hint_parent,
                &mut page.popover_pointerdown,
                &mut page.dialog_pointerdown,
            ] {
                if *slot == Some(n) {
                    *slot = None;
                }
            }
            remove_from(&mut page.close_watchers, n);
            let refresh = remove_from(&mut page.modal_dialogs, n);
            let removed = page.info.remove(&key(n));
            for info in page.info.values_mut() {
                for slot in [
                    &mut info.trigger,
                    &mut info.prev_focus,
                    &mut info.popover_toggle.source,
                    &mut info.dialog_toggle.source,
                    &mut info.dialog_prev_focus,
                    &mut info.request_source,
                ] {
                    if *slot == Some(n) {
                        *slot = None;
                    }
                }
            }
            page.attr_refs.retain(|r| r.owner != n && r.target != n);
            Some((refresh, removed))
        })
        .ok()
        .flatten();
    let Some((refresh, removed)) = forgotten else {
        return;
    };
    if refresh {
        dialog::refresh_top_layer(js);
    }
    if let Some(info) = removed {
        remove_timers(js, &info);
    }
}

fn remove_timers(js: Js, info: &Info) {
    for timer in [info.popover_toggle.timer, info.dialog_toggle.timer] {
        if timer != 0 {
            js.timer_remove(timer);
        }
    }
}

pub(crate) fn clear(js: Js) {
    let removed = PAGES
        .try_with(|pages| pages.try_borrow_mut().ok()?.remove(&js))
        .ok()
        .flatten();
    if let Some(page) = removed {
        page.info.values().for_each(|info| remove_timers(js, info));
    }
}

pub(crate) fn is_named(n: Option<Element>, name: &[u8]) -> bool {
    n.is_some_and(|n| n.element_name() == Some(name))
}

pub(crate) fn has_attr(n: Element, name: &CStr) -> bool {
    n.attr(name).is_some()
}

pub(crate) fn attr_is(n: Element, name: &CStr, keyword: &[u8]) -> bool {
    n.attr(name)
        .is_some_and(|v| v.to_bytes().eq_ignore_ascii_case(keyword))
}

pub(crate) fn is_html_element(n: Option<Element>) -> bool {
    n.is_some_and(|n| {
        n.is_element() && n.name().is_some() && n.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0
    })
}

pub(crate) fn is_connected(n: Element) -> bool {
    let root = n.root();
    root.kind_raw() == southstar_dom::node::KIND_DOCUMENT && root.flags() & FLAG_FRAGMENT == 0
}

pub(crate) fn in_active_document(js: Js, n: Element) -> bool {
    is_connected(n)
        && js
            .current_document()
            .is_some_and(|doc| n.root() == doc.root())
}

pub(crate) fn inclusive_ancestor(ancestor: Element, node: Option<Element>) -> bool {
    core::iter::successors(node, |p| p.parent()).any(|p| p == ancestor)
}

pub(crate) fn is_shadow_root(n: Element) -> bool {
    n.is_element() && has_attr(n, c"data-nd-shadow-root")
}

pub(crate) fn flat_parent(n: Element) -> Option<Element> {
    if let Some(slot) = ffi::assigned_slot(n) {
        return Some(slot);
    }
    let parent = n.parent()?;
    if is_shadow_root(parent) {
        parent.parent()
    } else {
        Some(parent)
    }
}

pub(crate) fn flat_inclusive_ancestors(node: Element) -> impl Iterator<Item = Element> {
    core::iter::successors(Some(node), |n| flat_parent(*n)).take(MAX_WALK as usize)
}

pub(crate) fn is_flat_inclusive_descendant(node: Element, ancestor: Element) -> bool {
    flat_inclusive_ancestors(node).any(|p| p == ancestor)
}

pub(crate) fn tree_root(n: Element) -> Element {
    let mut n = n;
    while let Some(parent) = n.parent() {
        if is_shadow_root(n) {
            break;
        }
        n = parent;
    }
    n
}

pub(crate) fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}
