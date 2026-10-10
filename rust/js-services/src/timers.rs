//! Southstar — setTimeout, setInterval and requestIdleCallback: each page's timers, the GLib sources that fire them and IdleDeadline.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::BTreeMap;

use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Frame, Gate, Js, Realm, Source};
use crate::{Page, page, page_or_new};

const IDLE_PERIOD_US: i64 = 50 * 1000;
const MAX_EXTRA_ARGS: usize = 64;
const MAX_DUE_BATCH: usize = 8;

pub(crate) struct Timer {
    realm: Realm,
    frame: Frame,
    callback: Option<Value>,
    code: Option<String>,
    args: Vec<Value>,
    is_interval: bool,
    is_idle: bool,
    immediate: bool,
    firing: bool,
    due_us: i64,
    idle_deadline_us: i64,
    nesting: i32,
    interval_ms: i32,
    source: Source,
}

#[derive(Default)]
pub(crate) struct Timers {
    pub list: BTreeMap<i32, Timer>,
    pub next_id: i32,
    pub nesting: i32,
    pub immediate: i32,
    pub running_due: bool,
}

struct IdleDeadline {
    deadline_us: i64,
    did_timeout: bool,
}

fn next_id(page: &Page) -> i32 {
    let mut timers = page.timers.borrow_mut();
    timers.next_id += 1;
    timers.next_id
}

fn take(page: &Page, id: i32) -> Option<Timer> {
    let mut timers = page.timers.borrow_mut();
    let timer = timers.list.remove(&id)?;
    if timer.immediate && timers.immediate > 0 {
        timers.immediate -= 1;
    }
    Some(timer)
}

pub(crate) fn discard(js: Js, timer: Timer) {
    ffi::source_remove(js, timer.source);
    drop(timer);
}

pub(crate) fn remove(js: Js, id: i32) {
    let Some(page) = page(js) else {
        return;
    };
    if let Some(timer) = take(&page, id) {
        discard(js, timer);
    }
}

pub(crate) fn clear_all(js: Js, page: &Page) {
    let list = {
        let mut timers = page.timers.borrow_mut();
        timers.immediate = 0;
        core::mem::take(&mut timers.list)
    };
    for timer in list.into_values() {
        discard(js, timer);
    }
}

fn to_int32(scope: &mut Scope<'_>, value: Option<&Value>) -> i32 {
    value.map_or(0, |value| scope.to_int32(value).unwrap_or(0))
}

fn schedule(scope: &mut Scope<'_>, this: &Value, args: &[Value], is_interval: bool) -> Value {
    let js = ffi::js_of(scope);
    let Some(handler) = args.first().filter(|_| !js.is_null()) else {
        return Value::int(0);
    };
    let page = page_or_new(js);
    if ffi::this_is_detached_window(scope, js, this) {
        return Value::int(next_id(&page));
    }
    let is_function = scope.is_function(handler);
    let code = if is_function {
        None
    } else {
        match scope.to_string(handler) {
            Ok(code) => Some(code),
            Err(_) => return Value::int(0),
        }
    };
    let mut ms = to_int32(scope, args.get(1)).max(0);
    let nesting = page.timers.borrow().nesting + 1;
    if nesting > 5 && ms < 4 {
        ms = 4;
    }
    let extra = if is_function && args.len() > 2 {
        args[2..].iter().take(MAX_EXTRA_ARGS).cloned().collect()
    } else {
        Vec::new()
    };
    let immediate = !is_interval && ms <= 1;
    let id = next_id(&page);
    let timer = Timer {
        realm: ffi::realm_of(scope),
        frame: ffi::context_frame(scope, js),
        callback: is_function.then(|| handler.clone()),
        code,
        args: extra,
        is_interval,
        is_idle: false,
        immediate,
        firing: false,
        due_us: ffi::monotonic_us() + i64::from(ms) * 1000,
        idle_deadline_us: 0,
        nesting,
        interval_ms: ms,
        source: ffi::attach_timeout(js, ms as u32, id),
    };
    let mut timers = page.timers.borrow_mut();
    if immediate {
        timers.immediate += 1;
    }
    timers.list.insert(id, timer);
    Value::int(id)
}

pub(crate) fn set_timeout(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    Ok(schedule(scope, this, args, false))
}

pub(crate) fn set_interval(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    Ok(schedule(scope, this, args, true))
}

pub(crate) fn clear(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    if js.is_null() || args.is_empty() {
        return Ok(Value::undefined());
    }
    let id = to_int32(scope, args.first());
    remove(js, id);
    Ok(Value::undefined())
}

pub(crate) fn request_idle_callback(
    scope: &mut Scope<'_>,
    _: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    let Some(callback) = args.first().filter(|_| !js.is_null()) else {
        return Ok(Value::int(0));
    };
    if !scope.is_function(callback) {
        return Ok(Value::int(0));
    }
    let page = page_or_new(js);
    let id = next_id(&page);
    let mut idle_deadline_us = 0;
    if let Some(options) = args.get(1).filter(|options| options.is_object()) {
        let timeout = scope.get(options, "timeout")?;
        if timeout.is_number()
            && let Ok(timeout_ms) = scope.to_int32(&timeout)
            && timeout_ms > 0
        {
            idle_deadline_us = ffi::monotonic_us() + i64::from(timeout_ms) * 1000;
        }
    }
    let timer = Timer {
        realm: ffi::realm_of(scope),
        frame: ffi::context_frame(scope, js),
        callback: Some(callback.clone()),
        code: None,
        args: Vec::new(),
        is_interval: false,
        is_idle: true,
        immediate: false,
        firing: false,
        due_us: 0,
        idle_deadline_us,
        nesting: 0,
        interval_ms: 0,
        source: ffi::attach_default_timeout(1, js, id),
    };
    page.timers.borrow_mut().list.insert(id, timer);
    Ok(Value::int(id))
}

fn idle_timers_end(page: &Page, now: i64, mut end: i64) -> i64 {
    for timer in page.timers.borrow().list.values() {
        if !timer.is_idle
            && !timer.firing
            && !timer.source.is_none()
            && timer.due_us > now
            && timer.due_us < end
        {
            end = timer.due_us;
        }
    }
    end
}

fn idle_deadline(scope: &mut Scope<'_>, js: Js, page: &Page, did_timeout: bool) -> Value {
    let now = ffi::monotonic_us();
    let deadline_us = if did_timeout {
        now
    } else {
        let end = idle_timers_end(page, now, now + IDLE_PERIOD_US);
        ffi::idle_frame_end(js, now, end)
    };
    let global = scope.global();
    let prototype = scope
        .get(&global, "IdleDeadline")
        .ok()
        .filter(Value::is_object)
        .and_then(|constructor| scope.get(&constructor, "prototype").ok())
        .filter(Value::is_object);
    scope.new_host_object(
        prototype.as_ref(),
        IdleDeadline {
            deadline_us,
            did_timeout,
        },
    )
}

fn time_remaining(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    let Some(deadline_us) = this.with_host(|deadline: &IdleDeadline| deadline.deadline_us) else {
        return Err(scope.type_error("Illegal invocation"));
    };
    let left = (deadline_us - ffi::monotonic_us()).max(0);
    Ok(Value::number(((left / 100) * 100) as f64 / 1000.0))
}

fn did_timeout(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> Result<Value, Value> {
    match this.with_host(|deadline: &IdleDeadline| deadline.did_timeout) {
        Some(did_timeout) => Ok(Value::boolean(did_timeout)),
        None => Err(scope.type_error("Illegal invocation")),
    }
}

pub(crate) fn install_idle_deadline(scope: &mut Scope<'_>, prototype: &Value) {
    let method = scope.function("timeRemaining", 0, time_remaining);
    let _ = scope.set(prototype, "timeRemaining", method);
    let getter = scope.function("get didTimeout", 0, did_timeout);
    let _ = scope.define_accessor(
        prototype,
        "didTimeout",
        Some(&getter),
        None,
        Attributes {
            writable: false,
            enumerable: true,
            configurable: true,
        },
    );
    let _ = scope.define_to_string_tag(prototype, "IdleDeadline");
}

struct Firing {
    realm: Realm,
    frame: Frame,
    callback: Option<Value>,
    code: Option<String>,
    args: Vec<Value>,
    is_idle: bool,
    is_interval: bool,
    idle_deadline_us: i64,
    nesting: i32,
}

fn drop_timer(js: Js, page: &Page, id: i32) {
    if let Some(mut timer) = take(page, id) {
        timer.source = Source::NONE;
        discard(js, timer);
    }
}

pub(crate) fn fire(js: Js, id: i32) -> bool {
    let Some(page) = page(js) else {
        return false;
    };
    let firing = {
        let timers = page.timers.borrow();
        let Some(timer) = timers.list.get(&id) else {
            return false;
        };
        Firing {
            realm: timer.realm,
            frame: timer.frame,
            callback: timer.callback.clone(),
            code: timer.code.clone(),
            args: timer.args.clone(),
            is_idle: timer.is_idle,
            is_interval: timer.is_interval,
            idle_deadline_us: timer.idle_deadline_us,
            nesting: timer.nesting,
        }
    };
    let idle_expired = firing.is_idle
        && firing.idle_deadline_us > 0
        && ffi::monotonic_us() >= firing.idle_deadline_us;
    match ffi::timer_gate(js, firing.frame, idle_expired) {
        Gate::Run => {}
        Gate::Wait => return true,
        Gate::Drop => {
            drop_timer(js, &page, id);
            return false;
        }
    }
    let previous_nesting = {
        let mut timers = page.timers.borrow_mut();
        if let Some(timer) = timers.list.get_mut(&id) {
            timer.firing = true;
        }
        core::mem::replace(&mut timers.nesting, firing.nesting)
    };
    ffi::in_timer_scope(js, firing.realm, firing.frame, |scope| {
        let result = if let Some(code) = &firing.code {
            scope.eval_script(code, "<timer>")
        } else if let Some(callback) = &firing.callback {
            if firing.is_idle {
                let deadline = idle_deadline(scope, js, &page, idle_expired);
                scope.call(callback, &Value::undefined(), &[deadline])
            } else {
                scope.call(callback, &Value::undefined(), &firing.args)
            }
        } else {
            Ok(Value::undefined())
        };
        page.timers.borrow_mut().nesting = previous_nesting;
        result
    });
    drop(firing.callback);
    drop(firing.args);
    let mut timers = page.timers.borrow_mut();
    let Some(timer) = timers.list.get_mut(&id) else {
        return false;
    };
    timer.firing = false;
    if !firing.is_interval {
        timer.source = Source::NONE;
        drop(timers);
        drop_timer(js, &page, id);
        return false;
    }
    if timer.interval_ms < 4 {
        timer.interval_ms = 4;
        timer.source = ffi::attach_timeout(js, 4, id);
        return false;
    }
    true
}

pub(crate) fn run_due(js: Js) {
    let Some(page) = page(js) else {
        return;
    };
    {
        let timers = page.timers.borrow();
        if timers.running_due || timers.immediate <= 0 {
            return;
        }
    }
    if !ffi::due_timers_allowed(js) {
        return;
    }
    let now = ffi::monotonic_us();
    let due: Vec<i32> = page
        .timers
        .borrow()
        .list
        .iter()
        .filter(|(_, timer)| {
            timer.immediate && !timer.firing && !timer.source.is_none() && timer.due_us <= now
        })
        .map(|(&id, _)| id)
        .take(MAX_DUE_BATCH)
        .collect();
    if due.is_empty() {
        return;
    }
    page.timers.borrow_mut().running_due = true;
    for id in due {
        let source = {
            let mut timers = page.timers.borrow_mut();
            let Some(timer) = timers.list.get_mut(&id) else {
                continue;
            };
            core::mem::replace(&mut timer.source, Source::NONE)
        };
        if source.is_none() {
            continue;
        }
        ffi::source_remove(js, source);
        if fire(js, id) {
            let mut timers = page.timers.borrow_mut();
            if let Some(timer) = timers.list.get_mut(&id)
                && timer.source.is_none()
            {
                timer.source = ffi::attach_timeout(js, 0, id);
            }
        }
    }
    page.timers.borrow_mut().running_due = false;
}

pub(crate) fn pending(js: Js, include_idle: bool) -> bool {
    page(js).is_some_and(|page| {
        page.timers
            .borrow()
            .list
            .values()
            .any(|timer| include_idle || !timer.is_idle)
    })
}

pub(crate) fn count(js: Js) -> usize {
    page(js).map_or(0, |page| page.timers.borrow().list.len())
}

pub(crate) fn purge_frame(js: Js, frame: Frame) {
    let Some(page) = page(js) else {
        return;
    };
    let ids: Vec<i32> = page
        .timers
        .borrow()
        .list
        .iter()
        .filter(|(_, timer)| timer.frame == frame)
        .map(|(&id, _)| id)
        .collect();
    for id in ids {
        if let Some(timer) = take(&page, id) {
            discard(js, timer);
        }
    }
}
