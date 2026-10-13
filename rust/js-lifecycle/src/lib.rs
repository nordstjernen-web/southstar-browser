//! Southstar — requestAnimationFrame and the animation-frame tick, scrollend, animation and media load events, and the page lifecycle tick.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod events;
mod ffi;
pub mod frames;
#[cfg(feature = "quickjs")]
mod legacy;
mod lifecycle;
mod scrollend;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ffi::Js;

#[derive(Default)]
pub(crate) struct Page {
    frames: RefCell<frames::Frames>,
    scrollend: RefCell<scrollend::Scrollend>,
    lifecycle: RefCell<lifecycle::Lifecycle>,
}

thread_local! {
    static PAGES: RefCell<HashMap<usize, Rc<Page>>> = RefCell::new(HashMap::new());
}

pub(crate) fn page(js: Js) -> Option<Rc<Page>> {
    if js.is_null() {
        return None;
    }
    PAGES
        .try_with(|pages| pages.try_borrow().ok()?.get(&js.key()).cloned())
        .ok()
        .flatten()
}

pub(crate) fn page_or_new(js: Js) -> Rc<Page> {
    PAGES.with(|pages| pages.borrow_mut().entry(js.key()).or_default().clone())
}

pub(crate) fn reset(js: Js) {
    if let Some(page) = page(js) {
        scrollend::clear(&page);
        frames::clear(&page);
    }
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES
        .try_with(|pages| pages.borrow_mut().remove(&js.key()))
        .ok()
        .flatten();
    if let Some(page) = page {
        lifecycle::teardown(js, &page);
        scrollend::clear(&page);
        frames::teardown(&page);
    }
}
