//! Southstar — the page lifecycle tick: readystatechange, deferred and async scripts, DOMContentLoaded, load and pageshow.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::NsNode;
use southstar_glib::{GArray, GBoolean};

use crate::ffi::{self, Js, SOURCE_REMOVE};
use crate::{Page, page, page_or_new};

const SCRIPT_DEFERRED: c_int = 1;
const SCRIPT_ASYNC: c_int = 2;
const BUSY_RETRY_MS: c_uint = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Interactive,
    Deferred,
    Async,
    Complete,
}

pub(crate) struct Lifecycle {
    source: c_uint,
    tasks: *mut GArray,
    doc: *mut NsNode,
    origin: Option<CString>,
    phase: Phase,
    start_us: i64,
}

impl Default for Lifecycle {
    fn default() -> Lifecycle {
        Lifecycle {
            source: 0,
            tasks: ptr::null_mut(),
            doc: ptr::null_mut(),
            origin: None,
            phase: Phase::Interactive,
            start_us: 0,
        }
    }
}

fn release(js: Js, page: &Page) {
    let (source, tasks) = {
        let mut lifecycle = page.lifecycle.borrow_mut();
        let taken = core::mem::take(&mut *lifecycle);
        (taken.source, taken.tasks)
    };
    if source != 0 {
        js.source_remove(source);
    }
    ffi::array_free(tasks);
}

pub(crate) fn clear(js: Js) {
    if let Some(page) = page(js) {
        release(js, &page);
    }
}

pub(crate) fn teardown(js: Js, page: &Page) {
    release(js, page);
}

pub(crate) fn tasks(js: Js) -> *mut GArray {
    page(js).map_or(ptr::null_mut(), |page| page.lifecycle.borrow().tasks)
}

pub(crate) fn start(
    js: Js,
    doc: *mut NsNode,
    tasks: *mut GArray,
    origin: Option<&CStr>,
    start_us: i64,
) {
    if js.is_null() {
        ffi::array_free(tasks);
        return;
    }
    let page = page_or_new(js);
    release(js, &page);
    *page.lifecycle.borrow_mut() = Lifecycle {
        source: 0,
        tasks,
        doc,
        origin: origin.map(CStr::to_owned),
        phase: Phase::Interactive,
        start_us,
    };
    schedule(js, &page);
}

fn schedule(js: Js, page: &Page) {
    if page.lifecycle.borrow().source != 0 {
        return;
    }
    let source = js.attach_timeout(0, tick);
    page.lifecycle.borrow_mut().source = source;
}

fn advance(js: Js, page: &Page, phase: Phase) {
    page.lifecycle.borrow_mut().phase = phase;
    schedule(js, page);
}

fn load_blocked(js: Js) -> bool {
    js.eval_depth() > 0 || js.iframe_load_depth() > 0 || js.in_pump() || js.load_blocked()
}

fn complete(js: Js, page: &Page, doc: *mut NsNode) {
    js.mark_navigation(c"domComplete");
    js.set_ready_state(2);
    js.scope(|scope| scope.gc());
    js.flush_autofocus();
    js.dispatch(doc, c"readystatechange");
    js.mark_navigation(c"loadEventStart");
    js.dispatch(doc, c"load");
    js.mark_navigation(c"loadEventEnd");
    js.fire_page_transition(c"pageshow");
    js.upgrade_custom_elements(doc);
    let origin = page.lifecycle.borrow().origin.clone();
    js.run_content_scripts(origin.as_deref());
    if ffi::profile_enabled() {
        let start_us = page.lifecycle.borrow().start_us;
        eprintln!(
            "[profile] js lifecycle total={:.1}ms",
            (ffi::monotonic_us() - start_us) as f64 / 1000.0
        );
    }
    release(js, page);
}

unsafe extern "C" fn tick(data: *mut c_void) -> GBoolean {
    let js = Js::from_data(data);
    let Some(page) = page(js) else {
        return SOURCE_REMOVE;
    };
    page.lifecycle.borrow_mut().source = 0;
    let (doc, tasks, origin, phase) = {
        let lifecycle = page.lifecycle.borrow();
        (
            lifecycle.doc,
            lifecycle.tasks,
            lifecycle.origin.clone(),
            lifecycle.phase,
        )
    };
    if js.halted() || doc.is_null() || tasks.is_null() {
        release(js, &page);
        return SOURCE_REMOVE;
    }
    if js.eval_depth() > 0 || js.callback_depth() > 0 || js.in_pump() {
        let source = js.attach_timeout(BUSY_RETRY_MS, tick);
        page.lifecycle.borrow_mut().source = source;
        return SOURCE_REMOVE;
    }
    let origin = origin
        .filter(|origin| !origin.is_empty())
        .unwrap_or_else(|| c"inline".to_owned());
    match phase {
        Phase::Interactive => {
            js.mark_navigation(c"domInteractive");
            js.set_ready_state(1);
            js.dispatch(doc, c"readystatechange");
            advance(js, &page, Phase::Deferred);
        }
        Phase::Deferred => {
            if js.run_next_script_schedule(tasks, SCRIPT_DEFERRED, &origin) {
                schedule(js, &page);
                return SOURCE_REMOVE;
            }
            js.mark_navigation(c"domContentLoadedEventStart");
            js.dispatch(doc, c"DOMContentLoaded");
            js.mark_navigation(c"domContentLoadedEventEnd");
            js.upgrade_custom_elements(doc);
            js.fire_parsed_stylesheet_loads(doc);
            advance(js, &page, Phase::Async);
        }
        Phase::Async => {
            if js.run_next_script_schedule(tasks, SCRIPT_ASYNC, &origin) {
                schedule(js, &page);
                return SOURCE_REMOVE;
            }
            js.drain_load_event_scripts();
            js.process_pending_iframes();
            js.drain_microtasks();
            js.schedule_pending_script_drain();
            advance(js, &page, Phase::Complete);
        }
        Phase::Complete => {
            js.drain_load_event_scripts();
            js.process_pending_iframes();
            js.drain_microtasks();
            if load_blocked(js) {
                schedule(js, &page);
                return SOURCE_REMOVE;
            }
            complete(js, &page, doc);
        }
    }
    SOURCE_REMOVE
}
