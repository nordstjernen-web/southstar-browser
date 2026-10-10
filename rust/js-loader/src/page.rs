//! Southstar — per-page loader state: the import map, the deferred and async script roots with their drain timer, the parser holds and the module load counters.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;

use southstar_dom::{Node, NsNode};

use crate::ffi::{self, Js};
use crate::hold::Hold;

#[derive(Default)]
pub(crate) struct Page {
    pub import_map: Vec<(Vec<u8>, Vec<u8>)>,
    pub deferred_roots: Vec<usize>,
    pub async_roots: Vec<usize>,
    pub async_source: u32,
    pub holds: Vec<Hold>,
    pub module_load_count: u32,
    pub module_load_bytes: usize,
    pub module_load_capped: bool,
}

thread_local! {
    static PAGES: RefCell<Vec<(usize, Page)>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn with_page<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> R {
    PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        let index = match pages.iter().position(|(owner, _)| *owner == js.key()) {
            Some(index) => index,
            None => {
                pages.push((js.key(), Page::default()));
                pages.len() - 1
            }
        };
        f(&mut pages[index].1)
    })
}

pub(crate) fn with_existing<R>(js: Js, f: impl FnOnce(&mut Page) -> R) -> Option<R> {
    PAGES.with(|pages| {
        pages
            .borrow_mut()
            .iter_mut()
            .find(|(owner, _)| *owner == js.key())
            .map(|(_, page)| f(page))
    })
}

pub(crate) fn peek_page<R>(js: Js, f: impl FnOnce(&Page) -> R) -> Option<R> {
    PAGES.with(|pages| {
        pages
            .borrow()
            .iter()
            .find(|(owner, _)| *owner == js.key())
            .map(|(_, page)| f(page))
    })
}

pub(crate) fn has_pending_roots(js: Js) -> bool {
    peek_page(js, |page| {
        !page.deferred_roots.is_empty() || !page.async_roots.is_empty()
    })
    .unwrap_or(false)
}

pub(crate) fn async_pending(js: Js) -> bool {
    peek_page(js, |page| {
        page.async_source != 0 || !page.async_roots.is_empty()
    })
    .unwrap_or(false)
}

pub(crate) fn cancel_async(js: Js) {
    let source = peek_page(js, |page| page.async_source).unwrap_or(0);
    if source == 0 && !has_pending_roots(js) {
        return;
    }
    with_page(js, |page| {
        page.async_source = 0;
        page.async_roots.clear();
    });
    if source != 0 {
        ffi::source_remove(source);
    }
}

fn in_tree(addr: usize, root: usize) -> bool {
    unsafe { Node::from_ptr(addr as *const NsNode) }
        .is_some_and(|n| southstar_dom::ancestors_and_self(n).any(|p| p.as_ptr() as usize == root))
}

pub(crate) fn forget_roots_in(js: Js, root: usize) {
    with_existing(js, |page| {
        page.deferred_roots.retain(|&addr| !in_tree(addr, root));
        page.async_roots.retain(|&addr| !in_tree(addr, root));
    });
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES.with(|pages| {
        let mut pages = pages.borrow_mut();
        pages
            .iter()
            .position(|(owner, _)| *owner == js.key())
            .map(|index| pages.swap_remove(index).1)
    });
    if let Some(page) = page {
        if page.async_source != 0 {
            ffi::source_remove(page.async_source);
        }
        crate::hold::released(page.holds.len());
    }
}
