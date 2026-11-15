//! Southstar — the eval budget a script, task or callback runs under, and the long-running script monitor's deadline.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::sync::OnceLock;

use crate::ffi::Js;

const DEFAULT_BUDGET_MS: i32 = 60_000;
const MAX_BUDGET_MS: i32 = 3_600_000;

fn environment_budget_ms() -> Option<i32> {
    static BUDGET: OnceLock<Option<i32>> = OnceLock::new();
    *BUDGET.get_or_init(|| {
        std::env::var("NS_JS_EVAL_BUDGET_MS")
            .ok()
            .and_then(|text| text.trim().parse().ok())
    })
}

pub(crate) fn eval_budget_us() -> i64 {
    let configured = environment_budget_ms().unwrap_or_else(|| {
        southstar_config::get().map_or(DEFAULT_BUDGET_MS, |c| c.js_eval_budget_ms)
    });
    let ms = if configured <= 0 {
        DEFAULT_BUDGET_MS
    } else {
        configured.min(MAX_BUDGET_MS)
    };
    i64::from(ms) * 1000
}

pub(crate) fn monitor_limit_us() -> i64 {
    eval_budget_us().max(i64::from(DEFAULT_BUDGET_MS) * 1000)
}

pub(crate) fn push(js: Js) -> i64 {
    let state = js.st();
    let saved = state.eval_deadline_us.get();
    let now = southstar_glib::monotonic_us();
    if saved == 0 {
        state.monitor_deadline_us.set(now + monitor_limit_us());
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
    let pumped = southstar_glib::monotonic_us() - pump_start_us;
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
