//! Southstar — the window-level services: navigator and its sub-objects, the window console, screen, matchMedia, Notification and queueMicrotask.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod console;
mod ffi;
mod media;
mod navigator;
mod screen;
mod window;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use southstar_js_engine::Value;

use crate::ffi::Js;

#[derive(Default)]
pub(crate) struct Page {
    counts: RefCell<HashMap<String, u32>>,
    timers: RefCell<Option<HashMap<String, Instant>>>,
    media_lists: RefCell<Vec<Value>>,
}

thread_local! {
    static PAGES: RefCell<HashMap<usize, Rc<Page>>> = RefCell::new(HashMap::new());
}

pub(crate) fn page(js: Js) -> Option<Rc<Page>> {
    PAGES
        .try_with(|pages| pages.try_borrow().ok()?.get(&js.key()).cloned())
        .ok()
        .flatten()
}

pub(crate) fn page_or_new(js: Js) -> Rc<Page> {
    PAGES.with(|pages| pages.borrow_mut().entry(js.key()).or_default().clone())
}

pub(crate) fn with_page<R>(js: Js, f: impl FnOnce(&Page) -> R) -> Option<R> {
    page(js).map(|page| f(&page))
}

pub(crate) fn reset(js: Js) {
    if let Some(page) = page(js) {
        let lists = core::mem::take(&mut *page.media_lists.borrow_mut());
        drop(lists);
    }
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES
        .try_with(|pages| pages.borrow_mut().remove(&js.key()))
        .ok()
        .flatten();
    if let Some(page) = page {
        let lists = core::mem::take(&mut *page.media_lists.borrow_mut());
        drop(lists);
    }
}
