//! Southstar — the camera and microphone helpers getUserMedia and enumerateDevices call: permission, acquire and release, the active camera's label and the device list.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi;

type JsResult = Result<Value, Value>;

pub(crate) fn request(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let want_video = args.first().is_some_and(|v| scope.to_bool(v));
    let want_audio = args.get(1).is_some_and(|v| scope.to_bool(v));
    let answer = match ffi::js_of(scope).camera_permission() {
        Some(true) => {
            if want_video {
                southstar_camera::acquire();
            }
            if want_audio && southstar_mic::acquire() {
                southstar_mic::grant(page_key(scope));
            }
            "granted"
        }
        Some(false) => "denied",
        None => "pending",
    };
    Ok(scope.string(answer))
}

pub(crate) fn release(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    southstar_camera::release();
    Ok(Value::undefined())
}

pub(crate) fn mic_release(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    if southstar_mic::revoke(page_key(scope)) {
        southstar_mic::release();
    }
    Ok(Value::undefined())
}

fn page_key(scope: &Scope<'_>) -> usize {
    scope.context_opaque() as usize
}

pub(crate) fn release_page_mic(page: usize) {
    for _ in 0..southstar_mic::revoke_all(page) {
        southstar_mic::release();
    }
}

pub(crate) fn label(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let devices = southstar_camera::devices();
    let active = southstar_camera::active_device();
    let label = active
        .and_then(|device| devices.iter().find(|d| d.device == device))
        .or(devices.first())
        .map_or(&b"Camera"[..], |d| &d.label);
    Ok(scope.string_from_bytes(label))
}

pub(crate) fn enumerate(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let list = scope.new_array();
    for (index, info) in southstar_camera::devices().into_iter().enumerate() {
        let device = scope.new_object();
        for (key, text) in [
            ("deviceId", &info.device[..]),
            ("groupId", &info.device[..]),
            ("kind", &b"videoinput"[..]),
            ("label", &info.label[..]),
        ] {
            let value = scope.string_from_bytes(text);
            let _ = scope.set(&device, key, value);
        }
        let _ = scope.set_index(&list, index as u32, device);
    }
    Ok(list)
}
