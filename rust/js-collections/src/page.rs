//! Southstar — per-page collection state: the DOM generation counter, the sixteen-slot query cache it invalidates, the static NodeList helpers and the live collection prototypes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;

use southstar_js_engine::Value;

use crate::ffi::Js;

const QCACHE_SLOTS: usize = 16;

struct Entry {
    root: usize,
    kind: u8,
    key: Box<[u8]>,
    generation: u64,
    value: Value,
}

#[derive(Clone)]
pub(crate) struct NodeListHelpers {
    pub decorator: Option<Value>,
    pub item: Value,
    pub named_item: Value,
    pub for_each: Value,
}

#[derive(Clone)]
pub(crate) struct LiveProtos {
    pub html: Value,
    pub node: Value,
    pub radio: Value,
}

#[derive(Default)]
struct Page {
    dom_gen: u64,
    qcache: [Option<Entry>; QCACHE_SLOTS],
    qcache_next: usize,
    nodelist: Option<NodeListHelpers>,
    live_protos: Option<LiveProtos>,
}

thread_local! {
    static PAGES: RefCell<Vec<(Js, Page)>> = const { RefCell::new(Vec::new()) };
}

fn with_page<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> R {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let index = match pages.iter().position(|(owner, _)| *owner == js) {
            Some(index) => index,
            None => {
                pages.push((js, Page::default()));
                pages.len() - 1
            }
        };
        f(&mut pages[index].1)
    })
}

fn peek_page<R>(js: Js, f: impl FnOnce(&Page) -> R) -> Option<R> {
    PAGES.with(|pages| {
        pages
            .borrow()
            .iter()
            .find(|(owner, _)| *owner == js)
            .map(|(_, page)| f(page))
    })
}

pub(crate) fn dom_gen(js: Js) -> u64 {
    if js.is_null() {
        return 0;
    }
    peek_page(js, |page| page.dom_gen).unwrap_or(0)
}

pub(crate) fn invalidate(js: Js) {
    if !js.is_null() {
        with_page(js, |page| page.dom_gen += 1);
    }
}

pub(crate) fn qcache_get(js: Js, root: usize, kind: u8, key: &[u8]) -> Option<Value> {
    if js.is_null() {
        return None;
    }
    peek_page(js, |page| {
        page.qcache.iter().flatten().find_map(|entry| {
            (entry.generation == page.dom_gen
                && entry.root == root
                && entry.kind == kind
                && *entry.key == *key)
                .then(|| entry.value.clone())
        })
    })
    .flatten()
}

pub(crate) fn qcache_put(js: Js, root: usize, kind: u8, key: &[u8], value: &Value) {
    if js.is_null() || value.is_undefined() || value.is_null() {
        return;
    }
    let stale = with_page(js, |page| {
        let generation = page.dom_gen;
        if let Some(entry) = page
            .qcache
            .iter_mut()
            .flatten()
            .find(|entry| entry.root == root && entry.kind == kind && *entry.key == *key)
        {
            entry.generation = generation;
            return Some(core::mem::replace(&mut entry.value, value.clone()));
        }
        let slot = page.qcache_next % QCACHE_SLOTS;
        page.qcache_next = (page.qcache_next + 1) % QCACHE_SLOTS;
        page.qcache[slot]
            .replace(Entry {
                root,
                kind,
                key: key.into(),
                generation,
                value: value.clone(),
            })
            .map(|entry| entry.value)
    });
    drop(stale);
}

pub(crate) fn nodelist_helpers(js: Js) -> Option<NodeListHelpers> {
    peek_page(js, |page| page.nodelist.clone()).flatten()
}

pub(crate) fn set_nodelist_helpers(js: Js, helpers: NodeListHelpers) {
    with_page(js, |page| page.nodelist = Some(helpers));
}

pub(crate) fn live_protos(js: Js) -> Option<LiveProtos> {
    peek_page(js, |page| page.live_protos.clone()).flatten()
}

pub(crate) fn set_live_protos(js: Js, protos: LiveProtos) {
    let old = with_page(js, |page| page.live_protos.replace(protos));
    drop(old);
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        pages
            .iter()
            .position(|(owner, _)| *owner == js)
            .map(|index| pages.swap_remove(index).1)
    });
    drop(page);
}
