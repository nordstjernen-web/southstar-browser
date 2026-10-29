//! Southstar — the page lifecycle tick: readystatechange, deferred and async scripts, DOMContentLoaded, load and pageshow.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::{Node, WeakNode};
use southstar_glib::SourceId;

use southstar_js_loader::{Schedule, ScriptTasks};

use crate::ffi::{self, Js};
use crate::{Page, page, page_or_new};

const BUSY_RETRY_MS: u32 = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Interactive,
    Deferred,
    Async,
    Complete,
}

pub(crate) struct Lifecycle {
    source: Option<SourceId>,
    doc: Option<WeakNode>,
    origin: Option<CString>,
    phase: Phase,
    start_us: i64,
}

impl Default for Lifecycle {
    fn default() -> Lifecycle {
        Lifecycle {
            source: None,
            doc: None,
            origin: None,
            phase: Phase::Interactive,
            start_us: 0,
        }
    }
}

fn release(js: Js, page: &Page) {
    if let Some(source) = core::mem::take(&mut *page.lifecycle.borrow_mut()).source {
        js.source_remove(source);
    }
    js.set_lifecycle_tasks(None);
}

pub(crate) fn clear(js: Js) {
    if let Some(page) = page(js) {
        release(js, &page);
    }
}

pub(crate) fn teardown(js: Js, page: &Page) {
    release(js, page);
}

pub(crate) fn start(
    js: Js,
    doc: Node<'_>,
    tasks: ScriptTasks,
    origin: Option<&CStr>,
    start_us: i64,
) {
    if js.is_null() {
        return;
    }
    let page = page_or_new(js);
    release(js, &page);
    js.set_lifecycle_tasks(Some(tasks));
    *page.lifecycle.borrow_mut() = Lifecycle {
        source: None,
        doc: Some(doc.downgrade()),
        origin: origin.map(CStr::to_owned),
        phase: Phase::Interactive,
        start_us,
    };
    schedule(js, &page);
}

fn schedule(js: Js, page: &Page) {
    if page.lifecycle.borrow().source.is_some() {
        return;
    }
    let source = js.attach_timeout(0, tick);
    page.lifecycle.borrow_mut().source = Some(source);
}

fn advance(js: Js, page: &Page, phase: Phase) {
    page.lifecycle.borrow_mut().phase = phase;
    schedule(js, page);
}

pub(crate) fn in_progress(js: Js) -> bool {
    page(js).is_some_and(|page| {
        let lifecycle = page.lifecycle.borrow();
        lifecycle.source.is_some()
            || (lifecycle.doc.is_some() && lifecycle.phase != Phase::Complete)
    })
}

fn load_blocked(js: Js) -> bool {
    js.eval_depth() > 0 || js.iframe_load_depth() > 0 || js.in_pump() || js.load_blocked()
}

fn complete(js: Js, page: &Page, doc: Option<Node<'_>>) {
    js.mark_navigation("domComplete");
    js.set_ready_state(2);
    js.scope(|scope| scope.gc());
    js.flush_autofocus();
    js.dispatch(doc, c"readystatechange");
    js.mark_navigation("loadEventStart");
    js.dispatch(doc, c"load");
    js.mark_navigation("loadEventEnd");
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

fn tick(js: Js) {
    let Some(page) = page(js) else {
        return;
    };
    page.lifecycle.borrow_mut().source = None;
    let (doc, origin, phase) = {
        let lifecycle = page.lifecycle.borrow();
        (lifecycle.doc, lifecycle.origin.clone(), lifecycle.phase)
    };
    if js.halted() || doc.is_none() || !js.has_lifecycle_tasks() {
        release(js, &page);
        return;
    }
    let doc = doc.and_then(WeakNode::get);
    if js.eval_depth() > 0 || js.callback_depth() > 0 || js.in_pump() {
        let source = js.attach_timeout(BUSY_RETRY_MS, tick);
        page.lifecycle.borrow_mut().source = Some(source);
        return;
    }
    let origin = origin
        .filter(|origin| !origin.is_empty())
        .unwrap_or_else(|| c"inline".to_owned());
    match phase {
        Phase::Interactive => {
            js.mark_navigation("domInteractive");
            js.set_ready_state(1);
            js.dispatch(doc, c"readystatechange");
            advance(js, &page, Phase::Deferred);
        }
        Phase::Deferred => {
            if js.run_next_script_schedule(Schedule::Deferred, &origin) {
                schedule(js, &page);
                return;
            }
            js.mark_navigation("domContentLoadedEventStart");
            js.dispatch(doc, c"DOMContentLoaded");
            js.mark_navigation("domContentLoadedEventEnd");
            js.upgrade_custom_elements(doc);
            js.fire_parsed_stylesheet_loads(doc);
            advance(js, &page, Phase::Async);
        }
        Phase::Async => {
            if js.run_next_script_schedule(Schedule::Async, &origin) {
                schedule(js, &page);
                return;
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
                return;
            }
            complete(js, &page, doc);
        }
    }
}
