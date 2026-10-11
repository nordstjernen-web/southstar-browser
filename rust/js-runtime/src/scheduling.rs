//! Southstar — draining microtasks and mutations, the gates timers and observers wait behind, and the queued message tasks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

use crate::budget;
use crate::ffi::glue;
use crate::ffi::scopes::{self, node_root};
use crate::ffi::state::MessageTask;
use crate::ffi::{self, Js};

pub(crate) const TIMER_RUN: c_int = 0;
pub(crate) const TIMER_WAIT: c_int = 1;
pub(crate) const TIMER_DROP: c_int = 2;

pub(crate) fn add_depth(cell: &core::cell::Cell<c_int>, delta: c_int) -> c_int {
    let value = cell.get() + delta;
    cell.set(value);
    value
}

pub(crate) fn drain_microtasks(js: Js) {
    let saved = budget::push(js);
    let state = js.st();
    let mut failed = core::ptr::null_mut();
    let mut status = 0;
    add_depth(&state.callback_depth, 1);
    loop {
        let deadline = state.eval_deadline_us.get();
        if deadline != 0 && ffi::monotonic_us() > deadline {
            break;
        }
        let realm = glue::pending_job_realm(js);
        status = scopes::in_realm(Some(js), realm, || {
            glue::execute_pending_job(js, &mut failed)
        });
        if status <= 0 {
            break;
        }
    }
    add_depth(&state.callback_depth, -1);
    if status < 0 && state.log.is_set() {
        let mut line = b"[error] microtask threw".to_vec();
        if !failed.is_null() {
            let (message, stack) = glue::pending_exception_text(failed);
            line.extend_from_slice(b": ");
            line.extend_from_slice(message.as_deref().unwrap_or(b"(no message)"));
            if let Some(stack) = stack {
                line.push(b'\n');
                line.extend_from_slice(&stack);
            }
        }
        let line = glue::c_line(&line);
        glue::log_line(js, line.as_ptr());
    }
    budget::pop(js, saved);
    glue::storage_drain_deferred_events(js);
    if state.callback_depth.get() == 0 {
        glue::report_pending_rejections(js);
    }
}

pub(crate) fn notify_mutated(js: Js) {
    let state = js.st();
    if state.mutated.get()
        && let Some((hook, data)) = state.mutated_hook.get()
    {
        unsafe { hook(data) };
    }
    state.mutated.set(false);
}

pub(crate) fn drain_mutations(js: Js) {
    drain_microtasks(js);
    notify_mutated(js);
    glue::storage_schedule_flush(js);
    glue::run_due_timers(js);
}

pub(crate) fn dispatch_finish(js: Js) {
    let state = js.st();
    if state.eval_depth.get() == 0 && state.callback_depth.get() == 0 && !state.in_pump.get() {
        drain_mutations(js);
    } else {
        notify_mutated(js);
        glue::storage_schedule_flush(js);
    }
}

pub(crate) fn microtask_checkpoint(js: Js) {
    let ctx = js.ctx();
    if !ctx.is_null() && js.st().callback_depth.get() == 0 && !glue::is_running_script(ctx) {
        drain_microtasks(js);
    }
}

pub(crate) fn due_timers_allowed(js: Js) -> bool {
    let state = js.st();
    if js.ctx().is_null()
        || js.halted()
        || state.in_pump.get()
        || state.dispatch_depth.get() > 0
        || state.iframe_load_depth.get() > 0
        || state.callback_depth.get() > 0
        || state.eval_depth.get() > 0
    {
        return false;
    }
    !glue::engine_in_blocking_fetch()
}

pub(crate) fn timer_gate(js: Js, frame: *mut southstar_dom::NsNode, idle_expired: bool) -> c_int {
    if js.halted() {
        return TIMER_DROP;
    }
    if js.st().in_pump.get() {
        return TIMER_WAIT;
    }
    if glue::engine_in_blocking_fetch() && !idle_expired {
        return TIMER_WAIT;
    }
    if !frame.is_null() && node_root(frame) != node_root(js.current_doc()) {
        return TIMER_DROP;
    }
    TIMER_RUN
}

pub(crate) fn observer_tick(js: Js) -> bool {
    let state = js.st();
    if js.ctx().is_null() {
        state.observer_tick_source.set(0);
        return false;
    }
    if state.in_pump.get() || state.dispatch_depth.get() > 0 {
        return true;
    }
    state.observer_tick_source.set(0);
    glue::observers_tick(js);
    drain_microtasks(js);
    false
}

pub(crate) fn drop_message_tasks(js: Js) {
    let state = js.st();
    let source = state.message_task_source.get();
    if source != 0 {
        glue::source_remove(Some(js), source);
        state.message_task_source.set(0);
    }
    let tasks = core::mem::take(&mut *state.message_tasks.borrow_mut());
    drop(tasks);
}

pub(crate) fn queue_message_task(js: Js, task: MessageTask) -> bool {
    let state = js.st();
    state.message_tasks.borrow_mut().push_back(task);
    state.message_task_source.get() == 0
}

pub(crate) fn run_message_tasks(js: Js) -> RunTasks {
    let state = js.st();
    if js.halted() {
        state.message_task_source.set(0);
        drop_message_tasks(js);
        return RunTasks::Done;
    }
    if state.in_pump.get() || glue::engine_in_blocking_fetch() {
        return RunTasks::Retry;
    }
    let pending = state.message_tasks.borrow().len();
    for _ in 0..pending {
        if js.halted() {
            break;
        }
        let Some(task) = state.message_tasks.borrow_mut().pop_front() else {
            break;
        };
        glue::run_message_task(task);
        drain_microtasks(js);
    }
    if pending > 0 {
        drain_mutations(js);
    }
    if !state.message_tasks.borrow().is_empty() {
        return RunTasks::Continue;
    }
    state.message_task_source.set(0);
    RunTasks::Done
}

pub(crate) enum RunTasks {
    Done,
    Retry,
    Continue,
}
