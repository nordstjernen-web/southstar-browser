//! Southstar — the page's navigation timing: coarsening the times the network reported, and the load milestones performance.timing records.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::NavigationTiming;

fn coarsen_ms(value: f64) -> f64 {
    if value <= 0.0 {
        0.0
    } else {
        (value * 10.0).floor() / 10.0
    }
}

pub(crate) fn coarsen(timing: &mut NavigationTiming) {
    timing.origin_us = (timing.origin_us / 100) * 100;
    timing.origin_real_ms = coarsen_ms(timing.origin_real_ms);
    for field in [
        &mut timing.domain_lookup_start_ms,
        &mut timing.domain_lookup_end_ms,
        &mut timing.connect_start_ms,
        &mut timing.connect_end_ms,
        &mut timing.secure_connection_start_ms,
        &mut timing.request_start_ms,
        &mut timing.response_start_ms,
        &mut timing.response_end_ms,
    ] {
        *field = coarsen_ms(*field);
    }
}

pub(crate) fn milestone<'a>(timing: &'a mut NavigationTiming, key: &str) -> Option<&'a mut f64> {
    Some(match key {
        "domLoading" => &mut timing.dom_loading_ms,
        "domInteractive" => &mut timing.dom_interactive_ms,
        "domContentLoadedEventStart" => &mut timing.dom_content_loaded_event_start_ms,
        "domContentLoadedEventEnd" => &mut timing.dom_content_loaded_event_end_ms,
        "domComplete" => &mut timing.dom_complete_ms,
        "loadEventStart" => &mut timing.load_event_start_ms,
        "loadEventEnd" => &mut timing.load_event_end_ms,
        _ => return None,
    })
}

pub(crate) fn record_legacy_milestone(scope: &mut Scope<'_>, key: &str, absolute_ms: i64) {
    let global = scope.global();
    let Ok(performance) = scope.get(&global, "performance") else {
        return;
    };
    let Ok(timing) = scope.get(&performance, "timing") else {
        return;
    };
    if timing.is_object() {
        let _ = scope.set(&timing, key, Value::int64(absolute_ms));
    }
}
