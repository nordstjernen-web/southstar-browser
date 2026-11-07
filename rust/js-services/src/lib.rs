//! Southstar — the window-level services: timers, navigator and its sub-objects, the camera and microphone helpers, the window console, alert, confirm and prompt, the document editing commands, screen, matchMedia, Notification, queueMicrotask, ClipboardItem and the WebRTC stubs.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod caches;
mod camera;
mod clipboard_item;
pub mod commands;
mod console;
pub(crate) mod ffi;
pub mod links;
mod media;
mod navigator;
mod rtc;
mod screen;
pub mod timers;
mod window;

pub use ffi::install_runtime_hooks;
pub use ffi::{
    purge_frame_timers, reeval_media_queries, reset_page, screen_size, timer_count, timer_remove,
    timers_pending,
};

pub(crate) use southstar_js_bind::{make_constructor, natives::resolved};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use southstar_js_engine::Value;

use crate::ffi::Js;

pub use ffi::nsr_services_console_emit;

pub use ffi::nsr_cache_open;
pub use ffi::nsr_element_async_method;
pub use ffi::nsr_services_clear_timer;
pub use ffi::nsr_services_queue_microtask;
pub use ffi::nsr_services_set_interval;
pub use ffi::nsr_services_set_timeout;

#[derive(Default)]
pub(crate) struct Page {
    counts: RefCell<HashMap<String, u32>>,
    timers: RefCell<timers::Timers>,
    console_timers: RefCell<Option<HashMap<String, Instant>>>,
    media_lists: RefCell<Vec<Value>>,
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

pub(crate) fn with_page<R>(js: Js, f: impl FnOnce(&Page) -> R) -> Option<R> {
    page(js).map(|page| f(&page))
}

pub(crate) fn reset(js: Js) {
    if let Some(page) = page(js) {
        timers::clear_all(js, &page);
        let lists = core::mem::take(&mut *page.media_lists.borrow_mut());
        drop(lists);
    }
}

pub(crate) fn teardown(js: Js) {
    camera::release_page_mic(js.key());
    let page = PAGES
        .try_with(|pages| pages.borrow_mut().remove(&js.key()))
        .ok()
        .flatten();
    if let Some(page) = page {
        timers::clear_all(js, &page);
        let lists = core::mem::take(&mut *page.media_lists.borrow_mut());
        drop(lists);
    }
}
