//! Southstar — the camera and microphone helpers getUserMedia and enumerateDevices call: permission, acquire and release, the active camera's label and the device list.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Permission};

type JsResult = Result<Value, Value>;

pub(crate) fn request(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let want_video = args.first().is_some_and(|v| scope.to_bool(v));
    let want_audio = args.get(1).is_some_and(|v| scope.to_bool(v));
    let answer = match ffi::camera_permission(ffi::js_of(scope)) {
        Permission::Granted => {
            if want_video {
                ffi::camera_acquire();
            }
            if want_audio {
                ffi::mic_acquire();
            }
            "granted"
        }
        Permission::Denied => "denied",
        Permission::Pending => "pending",
    };
    Ok(scope.string(answer))
}

pub(crate) fn release(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    ffi::camera_release();
    Ok(Value::undefined())
}

pub(crate) fn mic_release(_scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    ffi::mic_release();
    Ok(Value::undefined())
}

pub(crate) fn label(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let devices = ffi::camera_devices();
    let active = ffi::active_camera_device();
    let label = active
        .and_then(|device| devices.iter().find(|d| d.device == device))
        .or(devices.first())
        .map_or(&b"Camera"[..], |d| &d.label);
    Ok(scope.string_from_bytes(label))
}

pub(crate) fn enumerate(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let list = scope.new_array();
    for (index, info) in ffi::camera_devices().into_iter().enumerate() {
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
