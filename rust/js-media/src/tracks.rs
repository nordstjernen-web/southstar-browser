//! Southstar — text tracks: the VTTCue constructor, HTMLMediaElement.textTracks and addTextTrack.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::{JsResult, ffi};

fn number_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> JsResult<f64> {
    match args.get(index) {
        Some(value) => scope.to_number(value),
        None => Ok(0.0),
    }
}

fn text_arg(scope: &mut Scope<'_>, args: &[Value], index: usize) -> Value {
    let mut bytes = args
        .get(index)
        .and_then(|value| scope.to_bytes(value).ok())
        .unwrap_or_default();
    if let Some(end) = bytes.iter().position(|&b| b == 0) {
        bytes.truncate(end);
    }
    scope.string_from_bytes(&bytes)
}

pub(crate) fn vtt_cue(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let start = number_arg(scope, args, 0)?;
    let end = number_arg(scope, args, 1)?;
    let text = text_arg(scope, args, 2);
    let cue = scope.new_object();
    crate::set(scope, &cue, "startTime", Value::number(start));
    crate::set(scope, &cue, "endTime", Value::number(end));
    crate::set(scope, &cue, "text", text);
    crate::set_str(scope, &cue, "id", "");
    crate::set(scope, &cue, "pauseOnExit", Value::boolean(false));
    crate::set(scope, &cue, "track", Value::null());
    crate::set_str(scope, &cue, "vertical", "");
    crate::set(scope, &cue, "snapToLines", Value::boolean(true));
    crate::set_str(scope, &cue, "line", "auto");
    crate::set_str(scope, &cue, "lineAlign", "start");
    crate::set_str(scope, &cue, "position", "auto");
    crate::set_str(scope, &cue, "positionAlign", "auto");
    crate::set(scope, &cue, "size", Value::int(100));
    crate::set_str(scope, &cue, "align", "center");
    crate::set(scope, &cue, "region", Value::null());
    crate::set(scope, &cue, "onenter", Value::null());
    crate::set(scope, &cue, "onexit", Value::null());
    let listeners = scope.new_array();
    crate::set(scope, &cue, "_listeners", listeners);
    ffi::bind_listeners(scope, &cue);
    Ok(cue)
}

pub(crate) fn text_tracks(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(scope.new_array())
}

pub(crate) fn add_text_track(_: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    Ok(Value::undefined())
}
