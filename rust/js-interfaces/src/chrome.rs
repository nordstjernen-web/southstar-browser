//! Southstar — window.chrome with the loadTimes() and csi() timing reports of Chrome-compatible realms.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, ffi};

fn set_all(scope: &mut Scope<'_>, object: &Value, fields: Vec<(&str, Value)>) {
    for (key, value) in fields {
        let _ = scope.set(object, key, value);
    }
}

fn load_times(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let (origin_ms, now_ms) = ffi::page_clock(scope);
    let origin = origin_ms / 1000.0;
    let now = now_ms / 1000.0;
    let report = scope.new_object();
    let other = scope.string("Other");
    let h2 = scope.string("h2");
    let connection = scope.string("h2");
    set_all(
        scope,
        &report,
        vec![
            ("requestTime", Value::number(origin)),
            ("startLoadTime", Value::number(origin)),
            ("commitLoadTime", Value::number(origin + 0.04)),
            ("finishDocumentLoadTime", Value::number(origin + 0.09)),
            ("finishLoadTime", Value::number(origin + now)),
            ("firstPaintTime", Value::number(origin + 0.1)),
            ("firstPaintAfterLoadTime", Value::number(0.0)),
            ("navigationType", other),
            ("wasFetchedViaSpdy", Value::boolean(true)),
            ("wasNpnNegotiated", Value::boolean(true)),
            ("npnNegotiatedProtocol", h2),
            ("wasAlternateProtocolAvailable", Value::boolean(false)),
            ("connectionInfo", connection),
        ],
    );
    Ok(report)
}

fn csi(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let (origin_ms, now_ms) = ffi::page_clock(scope);
    let report = scope.new_object();
    set_all(
        scope,
        &report,
        vec![
            ("startE", Value::number(origin_ms)),
            ("onloadT", Value::number(origin_ms + now_ms)),
            ("pageT", Value::number(now_ms)),
            ("tran", Value::int(15)),
        ],
    );
    Ok(report)
}

pub fn install(scope: &mut Scope<'_>, global: &Value) {
    let chrome = scope.new_object();
    let load_times = scope.function("loadTimes", 0, load_times);
    let _ = scope.set(&chrome, "loadTimes", load_times);
    let csi = scope.function("csi", 0, csi);
    let _ = scope.set(&chrome, "csi", csi);
    let _ = scope.set(global, "chrome", chrome);
}
