//! Southstar — window.screen and its orientation, sized from the display the browser runs on.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, CMethod};
use crate::navigator;

#[derive(Clone, Copy)]
pub(crate) struct Metrics {
    pub width: i32,
    pub height: i32,
    pub avail_width: i32,
    pub avail_height: i32,
    pub avail_left: i32,
    pub avail_top: i32,
}

pub(crate) fn metrics() -> Metrics {
    let mut metrics = Metrics {
        width: 1920,
        height: 1080,
        avail_width: 1920,
        avail_height: 1040,
        avail_left: 0,
        avail_top: 0,
    };
    if let Some(display) = ffi::display_metrics() {
        if display.width > 0 {
            metrics.width = display.width;
        }
        if display.height > 0 {
            metrics.height = display.height;
        }
        if let Some((left, top, right, bottom)) = display.work_area {
            metrics.avail_width = right - left;
            metrics.avail_height = bottom - top;
            metrics.avail_left = left;
            metrics.avail_top = top;
        }
    }
    metrics
}

fn event_target(scope: &mut Scope<'_>, object: &Value) {
    let listeners = scope.new_array();
    let _ = scope.set(object, "_listeners", listeners);
    ffi::bind_event_target(scope, object);
    let dispatch = ffi::c_method(scope, "dispatchEvent", 1, CMethod::DispatchEvent);
    let _ = scope.set(object, "dispatchEvent", dispatch);
}

fn orientation(scope: &mut Scope<'_>) -> Value {
    let orientation = scope.new_object();
    let kind = scope.string("landscape-primary");
    let _ = scope.set(&orientation, "type", kind);
    let _ = scope.set(&orientation, "angle", Value::int(0));
    let lock = scope.function("lock", 1, navigator::rejected_not_supported);
    let _ = scope.set(&orientation, "lock", lock);
    let unlock = scope.function("unlock", 0, navigator::noop);
    let _ = scope.set(&orientation, "unlock", unlock);
    event_target(scope, &orientation);
    let _ = scope.define_to_string_tag(&orientation, "ScreenOrientation");
    orientation
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let metrics = metrics();
    ffi::set_device_size(metrics.width, metrics.height);
    let screen = scope.new_object();
    for (key, value) in [
        ("width", metrics.width),
        ("height", metrics.height),
        ("availWidth", metrics.avail_width),
        ("availHeight", metrics.avail_height),
        ("availLeft", metrics.avail_left),
        ("availTop", metrics.avail_top),
        ("colorDepth", 24),
        ("pixelDepth", 24),
    ] {
        let _ = scope.set(&screen, key, Value::int(value));
    }
    let orientation = orientation(scope);
    let _ = scope.set(&screen, "orientation", orientation);
    event_target(scope, &screen);
    let _ = scope.define_to_string_tag(&screen, "Screen");
    let _ = scope.set(global, "screen", screen);
}
