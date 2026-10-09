//! Southstar — each page's performance timeline, its PerformanceObservers and the time origins of its frame realms.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use southstar_js_engine::Value;

use crate::entry::{ENTRY_CAP, Entry, coarsen_real_ms, coarsen_us};
use crate::ffi::{self, Js, RawObject};

#[derive(Clone, Copy)]
struct Origin {
    origin_us: i64,
    origin_real_ms: f64,
}

#[derive(Default)]
pub(crate) struct Timeline {
    entries: RefCell<Option<Vec<Entry>>>,
    observers: RefCell<Vec<Weak<Observer>>>,
    drain_scheduled: Cell<bool>,
}

#[derive(Default)]
pub(crate) struct ObserverState {
    pub disconnected: bool,
    pub pinned: Option<Value>,
    pub entry_types: Vec<Vec<u8>>,
    pub records: Vec<Entry>,
}

pub(crate) struct Observer {
    pub js: Js,
    pub realm: usize,
    pub callback: Option<Value>,
    pub wrapper: Cell<Option<RawObject>>,
    pub state: RefCell<ObserverState>,
}

thread_local! {
    static TIMELINES: RefCell<HashMap<Js, Rc<Timeline>>> = RefCell::new(HashMap::new());
    static CLOCKS: RefCell<HashMap<Js, HashMap<usize, Origin>>> = RefCell::new(HashMap::new());
}

pub(crate) fn timeline(js: Js) -> Option<Rc<Timeline>> {
    TIMELINES
        .try_with(|timelines| timelines.borrow().get(&js).cloned())
        .ok()
        .flatten()
}

pub(crate) fn timeline_or_new(js: Js) -> Rc<Timeline> {
    TIMELINES.with(|timelines| timelines.borrow_mut().entry(js).or_default().clone())
}

pub(crate) fn init(js: Js) {
    *timeline_or_new(js).entries.borrow_mut() = Some(Vec::new());
}

pub(crate) fn reset_observers(js: Js) {
    let Some(timeline) = timeline(js) else {
        return;
    };
    for observer in timeline.live_observers() {
        observer.disconnect();
    }
    timeline.drain_scheduled.set(false);
}

pub(crate) fn teardown(js: Js) {
    let Some(timeline) = timeline(js) else {
        return;
    };
    drop(timeline.entries.borrow_mut().take());
    for observer in timeline.live_observers() {
        observer.disconnect();
    }
    TIMELINES.with(|timelines| timelines.borrow_mut().remove(&js));
}

impl Timeline {
    pub fn has_entries(&self) -> bool {
        self.entries.borrow().is_some()
    }

    pub fn entries(&self) -> Vec<Entry> {
        self.entries.borrow().clone().unwrap_or_default()
    }

    pub fn entries_in(&self, realm: usize) -> Vec<Entry> {
        self.entries
            .borrow()
            .iter()
            .flatten()
            .filter(|entry| entry.realm == realm)
            .cloned()
            .collect()
    }

    pub fn push(&self, entry: Entry) -> bool {
        let mut entries = self.entries.borrow_mut();
        let Some(entries) = entries.as_mut() else {
            return false;
        };
        if entries.len() >= ENTRY_CAP {
            entries.remove(0);
        }
        entries.push(entry);
        true
    }

    pub fn any(&self, f: impl Fn(&Entry) -> bool) -> bool {
        self.entries.borrow().iter().flatten().any(f)
    }

    pub fn latest(&self, f: impl Fn(&Entry) -> bool) -> Option<Entry> {
        self.entries
            .borrow()
            .iter()
            .flatten()
            .rev()
            .find(|e| f(e))
            .cloned()
    }

    pub fn retain(&self, keep: impl Fn(&Entry) -> bool) {
        if let Some(entries) = self.entries.borrow_mut().as_mut() {
            entries.retain(keep);
        }
    }

    pub fn move_realm(&self, from: usize, to: usize) {
        for entry in self.entries.borrow_mut().iter_mut().flatten() {
            if entry.realm == from {
                entry.realm = to;
            }
        }
    }

    pub fn add_observer(&self, observer: &Rc<Observer>) {
        self.observers.borrow_mut().push(Rc::downgrade(observer));
    }

    pub fn observer_at(&self, index: usize) -> Option<Option<Rc<Observer>>> {
        let weak = self.observers.borrow().get(index).cloned()?;
        Some(weak.upgrade())
    }

    fn live_observers(&self) -> Vec<Rc<Observer>> {
        let weaks = self.observers.borrow().clone();
        weaks.iter().filter_map(Weak::upgrade).collect()
    }

    pub fn take_drain(&self) {
        self.drain_scheduled.set(false);
    }

    pub fn claim_drain(&self) -> bool {
        !self.drain_scheduled.replace(true)
    }
}

impl Observer {
    pub fn wants(&self, kind: &[u8]) -> bool {
        self.state
            .borrow()
            .entry_types
            .iter()
            .any(|want| want == kind)
    }

    pub fn takes(&self, entry: &Entry) -> bool {
        !self.state.borrow().disconnected
            && self.callback.is_some()
            && self.realm == entry.realm
            && self.wants(&entry.kind)
    }

    pub fn record(&self, entry: Entry) {
        let mut state = self.state.borrow_mut();
        if state.records.len() >= ENTRY_CAP {
            state.records.remove(0);
        }
        state.records.push(entry);
    }

    pub fn pin(&self, pinned: impl FnOnce() -> Option<Value>) {
        if self.state.borrow().pinned.is_some() {
            return;
        }
        let value = pinned();
        self.state.borrow_mut().pinned = value;
    }

    pub fn disconnect(&self) {
        let pinned = {
            let mut state = self.state.borrow_mut();
            state.disconnected = true;
            state.records.clear();
            state.entry_types.clear();
            state.pinned.take()
        };
        drop(pinned);
    }

    pub fn take_records(&self) -> Vec<Entry> {
        std::mem::take(&mut self.state.borrow_mut().records)
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        let Some(timeline) = timeline(self.js) else {
            return;
        };
        let me: *const Observer = self;
        let Ok(mut observers) = timeline.observers.try_borrow_mut() else {
            return;
        };
        if let Some(index) = observers.iter().position(|w| w.as_ptr() == me) {
            observers.swap_remove(index);
        }
    }
}

pub(crate) fn queue(js: Js, entry: &Entry) {
    let Some(timeline) = timeline(js) else {
        return;
    };
    if ffi::main_context(js).is_none() {
        return;
    }
    let mut queued = false;
    let mut index = 0;
    while let Some(observer) = timeline.observer_at(index) {
        index += 1;
        let Some(observer) = observer else {
            continue;
        };
        if !observer.takes(entry) {
            continue;
        }
        observer.record(entry.clone());
        observer.pin(|| ffi::pin_wrapper(js, &observer));
        queued = true;
    }
    if queued {
        schedule_drain(js, &timeline);
    }
}

pub(crate) fn schedule_drain(js: Js, timeline: &Timeline) {
    if ffi::main_context(js).is_none() || !timeline.claim_drain() {
        return;
    }
    ffi::enqueue_drain(js);
}

pub(crate) fn time_origin_us(js: Js, realm: usize) -> i64 {
    if js.is_null() {
        return 0;
    }
    clock(js, realm).map_or_else(|| ffi::page_time_origin_us(js), |o| o.origin_us)
}

pub(crate) fn time_origin_real_ms(js: Js, realm: usize) -> f64 {
    if js.is_null() {
        return 0.0;
    }
    clock(js, realm).map_or_else(|| ffi::page_time_origin_real_ms(js), |o| o.origin_real_ms)
}

fn clock(js: Js, realm: usize) -> Option<Origin> {
    if realm == 0 {
        return None;
    }
    CLOCKS.with(|clocks| clocks.borrow().get(&js)?.get(&realm).copied())
}

pub(crate) fn start_frame_clock(js: Js, frame: usize) {
    if js.is_null() || frame == 0 {
        return;
    }
    let origin = Origin {
        origin_us: coarsen_us(ffi::monotonic_us()),
        origin_real_ms: coarsen_real_ms(ffi::real_us()),
    };
    CLOCKS.with(|clocks| {
        clocks
            .borrow_mut()
            .entry(js)
            .or_default()
            .insert(frame, origin)
    });
}

pub(crate) fn adopt_frame_clock(js: Js, frame: usize, realm: usize) {
    if js.is_null() || realm == 0 {
        return;
    }
    let adopted = frame != 0
        && CLOCKS.with(|clocks| {
            let mut clocks = clocks.borrow_mut();
            let Some(table) = clocks.get_mut(&js) else {
                return false;
            };
            let Some(origin) = table.remove(&frame) else {
                return false;
            };
            table.insert(realm, origin);
            true
        });
    if !adopted {
        start_frame_clock(js, realm);
    }
}

pub(crate) fn clear_frame_clocks(js: Js, destroy: bool) {
    if js.is_null() {
        return;
    }
    CLOCKS.with(|clocks| {
        let mut clocks = clocks.borrow_mut();
        if destroy {
            clocks.remove(&js);
        } else if let Some(table) = clocks.get_mut(&js) {
            table.clear();
        }
    });
}
