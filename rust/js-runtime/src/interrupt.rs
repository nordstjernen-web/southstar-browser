//! Southstar — the engine's interrupt check: halted realms, closing workers, the eval budget and the long-running script monitor; and a worker thread's stack limit.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::budget::MONITOR_LIMIT_US;
use crate::ffi::{self, Js};

const STACK_LIMIT: usize = 5 * 1024 * 1024;

pub(crate) fn should_interrupt(js: Js) -> bool {
    if js.halted() {
        return true;
    }
    if js.worker_closing() {
        js.halt();
        return true;
    }
    let now = ffi::monotonic_us();
    let state = js.st();
    let monitor_deadline = state.monitor_deadline_us.get();
    if monitor_deadline != 0 && now > monitor_deadline {
        js.halt();
        ffi::warn(&format!(
            "[js] monitor: page JavaScript ran longer than {} s — halting it",
            MONITOR_LIMIT_US / 1_000_000
        ));
        return true;
    }
    let eval_deadline = state.eval_deadline_us.get();
    eval_deadline != 0 && now > eval_deadline
}

pub(crate) fn worker_stack_limit() -> usize {
    let stack = ffi::thread_stack_size();
    if stack > 0 && stack - stack / 4 < STACK_LIMIT {
        stack - stack / 4
    } else {
        STACK_LIMIT
    }
}
