//! Southstar — the eval budget a script, task or callback runs under, and the long-running script monitor's deadline.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi::{self, Js};

pub(crate) const MONITOR_LIMIT_US: i64 = 60 * 1_000_000;
const DEFAULT_BUDGET_MS: i32 = 60_000;
const MAX_BUDGET_MS: i32 = 60_000;

pub(crate) fn eval_budget_us() -> i64 {
    let configured = southstar_config::get().map_or(DEFAULT_BUDGET_MS, |c| c.js_eval_budget_ms);
    let ms = if configured <= 0 {
        DEFAULT_BUDGET_MS
    } else {
        configured.min(MAX_BUDGET_MS)
    };
    i64::from(ms) * 1000
}

pub(crate) fn push(js: Js) -> i64 {
    let state = js.st();
    let saved = state.eval_deadline_us.get();
    let now = ffi::monotonic_us();
    if saved == 0 {
        state.monitor_deadline_us.set(now + MONITOR_LIMIT_US);
    }
    let fresh = now + eval_budget_us();
    if saved == 0 || fresh < saved {
        state.eval_deadline_us.set(fresh);
    }
    saved
}

pub(crate) fn pop(js: Js, saved: i64) {
    let state = js.st();
    state.eval_deadline_us.set(saved);
    if saved == 0 {
        state.monitor_deadline_us.set(0);
    }
}

pub(crate) fn credit_pumped_time(js: Js, pump_start_us: i64) {
    let pumped = ffi::monotonic_us() - pump_start_us;
    if pumped <= 0 {
        return;
    }
    let state = js.st();
    for deadline in [&state.eval_deadline_us, &state.monitor_deadline_us] {
        if deadline.get() != 0 {
            deadline.set(deadline.get() + pumped);
        }
    }
}
