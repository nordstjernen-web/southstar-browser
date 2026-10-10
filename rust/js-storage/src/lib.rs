//! Southstar — Web Storage: localStorage and sessionStorage, their persistence, quota and partitioning, and the storage event fan-out.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod binding;
mod events;
mod ffi;
mod store;

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

use southstar_js_engine::Value;

use crate::ffi::Js;
use crate::store::Storage;

pub(crate) type JsResult<T = Value> = Result<T, Value>;

pub(crate) struct PendingEvent {
    pub event: Value,
    pub source: Value,
}

pub(crate) struct Page {
    pub storage: RefCell<Storage>,
    pub events: RefCell<VecDeque<PendingEvent>>,
    pub draining: Cell<bool>,
    pub flush_source: Cell<u32>,
}

thread_local! {
    static PAGES: RefCell<HashMap<Js, Rc<Page>>> = RefCell::new(HashMap::new());
}

pub(crate) fn page(js: Js) -> Option<Rc<Page>> {
    if js.is_null() {
        return None;
    }
    PAGES
        .try_with(|pages| pages.borrow().get(&js).cloned())
        .ok()
        .flatten()
}

pub(crate) fn init(js: Js) {
    let disabled = southstar_config::get().is_some_and(|config| config.local_storage_enabled == 0);
    let page = Rc::new(Page {
        storage: RefCell::new(Storage::new(disabled)),
        events: RefCell::new(VecDeque::new()),
        draining: Cell::new(false),
        flush_source: Cell::new(0),
    });
    PAGES.with(|pages| pages.borrow_mut().insert(js, page));
}

pub(crate) fn teardown(js: Js) {
    let page = PAGES
        .try_with(|pages| pages.borrow_mut().remove(&js))
        .ok()
        .flatten();
    if let Some(page) = page {
        ffi::cancel_flush(&page);
        let events = core::mem::take(&mut *page.events.borrow_mut());
        drop(events);
    }
}
