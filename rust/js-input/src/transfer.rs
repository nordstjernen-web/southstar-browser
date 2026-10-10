//! Southstar — DataTransfer objects for clipboard and drag events, and the drag session the shell fills with data and dropped files.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, Pointer, set};

pub(crate) struct DragSession {
    js: Js,
    data_transfer: Value,
}

fn new_data_transfer(scope: &mut Scope<'_>) -> Value {
    let global = scope.global();
    let ctor = scope
        .get(&global, "DataTransfer")
        .unwrap_or_else(|_| Value::undefined());
    if !scope.is_function(&ctor) {
        return Value::null();
    }
    scope
        .construct(&ctor, &[])
        .unwrap_or_else(|_| Value::null())
}

fn set_data(scope: &mut Scope<'_>, data_transfer: &Value, kind: &str, data: Option<&str>) {
    let Ok(set_data) = scope.get(data_transfer, "setData") else {
        return;
    };
    if !scope.is_function(&set_data) {
        return;
    }
    let args = [scope.string(kind), scope.string(data.unwrap_or(""))];
    let _ = scope.call(&set_data, data_transfer, &args);
}

fn base_name(path: &[u8]) -> &[u8] {
    let is_separator = |b: &u8| *b == b'/' || (cfg!(windows) && *b == b'\\');
    path.iter()
        .rposition(is_separator)
        .map_or(path, |at| &path[at + 1..])
}

fn file_from_path(scope: &mut Scope<'_>, path: &CStr) -> Option<Value> {
    if path.is_empty() {
        return None;
    }
    let file = ffi::read_file(path)?;
    let global = scope.global();
    let ctor = scope.get(&global, "File").ok()?;
    if !scope.is_constructor(&ctor) {
        return None;
    }
    let buffer = scope.new_array_buffer(&file.contents).ok()?;
    let parts = scope.new_array();
    let _ = scope.set_index(&parts, 0, buffer);
    let options = scope.new_object();
    let mime = file
        .mime
        .as_deref()
        .filter(|mime| !mime.is_empty())
        .unwrap_or("application/octet-stream");
    let mime = scope.string(mime);
    set(scope, &options, "type", mime);
    let name = scope.string(&String::from_utf8_lossy(base_name(path.to_bytes())));
    scope.construct(&ctor, &[parts, name, options]).ok()
}

impl DragSession {
    pub(crate) fn new(js: Js) -> Option<DragSession> {
        if js.halted() {
            return None;
        }
        let data_transfer = js.scope(new_data_transfer)?;
        data_transfer
            .is_object()
            .then_some(DragSession { js, data_transfer })
    }

    pub(crate) fn release(self) {
        if self.js.has_context() {
            drop(self.data_transfer);
        } else {
            core::mem::forget(self.data_transfer);
        }
    }

    pub(crate) fn set_data(&self, kind: &str, data: Option<&str>) {
        self.js
            .scope(|scope| set_data(scope, &self.data_transfer, kind, data));
    }

    pub(crate) fn add_file(&self, path: &CStr) {
        self.js.scope(|scope| {
            let Some(file) = file_from_path(scope, path) else {
                return;
            };
            if file.is_null() || file.is_undefined() {
                return;
            }
            let Ok(items) = scope.get(&self.data_transfer, "items") else {
                return;
            };
            if !items.is_object() {
                return;
            }
            let Ok(add) = scope.get(&items, "add") else {
                return;
            };
            if scope.is_function(&add) {
                let _ = scope.call(&add, &items, &[file]);
            }
        });
    }
}

pub(crate) fn clipboard_event(
    js: Js,
    target: Element,
    kind: &str,
    text: Option<&str>,
) -> (bool, bool) {
    if js.blocked() {
        return (false, false);
    }
    let Some(event) = js.scope(|scope| {
        let data = new_data_transfer(scope);
        if let Some(text) = text
            && data.is_object()
        {
            set_data(scope, &data, "text/plain", Some(text));
        }
        let event = ffi::make_event(scope, kind, target);
        set(scope, &event, "bubbles", Value::boolean(true));
        set(scope, &event, "cancelable", Value::boolean(true));
        set(scope, &event, "clipboardData", data);
        event
    }) else {
        return (false, false);
    };
    js.dispatch_built(target, kind, event)
}

pub(crate) fn drag_event(
    js: Js,
    session: Option<&DragSession>,
    target: Element,
    kind: &str,
    input: &Pointer,
    related: Option<Element>,
) -> (bool, bool) {
    if js.blocked() {
        return (false, false);
    }
    if session.is_some_and(|session| session.js != js) {
        return (false, false);
    }
    let Some(event) = js.scope(|scope| {
        let event = ffi::make_event(scope, kind, target);
        let cancelable = matches!(kind, "dragstart" | "dragenter" | "dragover" | "drop");
        set(scope, &event, "cancelable", Value::boolean(cancelable));
        let (cx, cy) = input.client;
        let (px, py) = input.page;
        set(scope, &event, "clientX", Value::number(cx));
        set(scope, &event, "clientY", Value::number(cy));
        set(scope, &event, "x", Value::number(cx));
        set(scope, &event, "y", Value::number(cy));
        set(scope, &event, "pageX", Value::number(px));
        set(scope, &event, "pageY", Value::number(py));
        set(scope, &event, "screenX", Value::number(cx));
        set(scope, &event, "screenY", Value::number(cy));
        set(scope, &event, "offsetX", Value::number(cx));
        set(scope, &event, "offsetY", Value::number(cy));
        set(scope, &event, "movementX", Value::int(0));
        set(scope, &event, "movementY", Value::int(0));
        set(scope, &event, "button", Value::int(input.button));
        set(scope, &event, "buttons", Value::int(input.buttons));
        set(
            scope,
            &event,
            "which",
            Value::int(input.button.wrapping_add(1)),
        );
        set(scope, &event, "detail", Value::int(0));
        let m = input.modifiers;
        set(scope, &event, "shiftKey", Value::boolean(m.shift));
        set(scope, &event, "ctrlKey", Value::boolean(m.ctrl));
        set(scope, &event, "altKey", Value::boolean(m.alt));
        set(scope, &event, "metaKey", Value::boolean(m.meta));
        let related = match related {
            Some(related) => ffi::wrap(scope, related),
            None => Value::null(),
        };
        set(scope, &event, "relatedTarget", related);
        let data = session.map_or_else(Value::null, |session| session.data_transfer.clone());
        set(scope, &event, "dataTransfer", data);
        event
    }) else {
        return (false, false);
    };
    js.dispatch_built(target, kind, event)
}
