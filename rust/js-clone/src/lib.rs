//! Southstar — structured cloning of JavaScript values within a realm (structuredClone, window and port messages), with transfer lists and DataCloneError for what cannot be cloned.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod wire;

use std::collections::HashMap;

use southstar_js_engine::quickjs::{self, OwnSlot};
use southstar_js_engine::{Attributes, Scope, Value};

type Result<T = Value> = core::result::Result<T, Value>;

pub(crate) const MAX_DEPTH: u32 = 512;

pub(crate) const ERROR_CONSTRUCTORS: [&str; 7] = [
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
];

pub(crate) const HIDDEN_PROPERTY: Attributes = Attributes {
    writable: true,
    enumerable: false,
    configurable: true,
};

const GEOMETRY: [(&str, &str); 7] = [
    ("DOMRect", "fromRect"),
    ("DOMRectReadOnly", "fromRect"),
    ("DOMPoint", "fromPoint"),
    ("DOMPointReadOnly", "fromPoint"),
    ("DOMQuad", "fromQuad"),
    ("DOMMatrix", "fromMatrix"),
    ("DOMMatrixReadOnly", "fromMatrix"),
];

pub(crate) fn fail<T>(scope: &mut Scope<'_>) -> Result<T> {
    Err(ffi::data_clone_error(scope))
}

pub(crate) fn get(scope: &mut Scope<'_>, object: &Value, key: &str) -> Value {
    scope
        .get(object, key)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn global_get(scope: &mut Scope<'_>, key: &str) -> Value {
    let global = scope.global();
    get(scope, &global, key)
}

pub(crate) fn length(scope: &mut Scope<'_>, array: &Value) -> u32 {
    let len = get(scope, array, "length");
    scope.to_number(&len).map_or(0, |n| n as u32)
}

pub(crate) fn instance_of(scope: &mut Scope<'_>, value: &Value, constructor: &Value) -> bool {
    quickjs::instance_of(scope, value, constructor)
}

pub(crate) fn call_method(scope: &mut Scope<'_>, object: &Value, name: &str) -> Result {
    let method = scope.get(object, name)?;
    scope.call(&method, object, &[])
}

pub(crate) fn error_constructor(scope: &mut Scope<'_>, name: &Value) -> &'static str {
    let name = if name.is_string() {
        scope.to_bytes(name).ok()
    } else {
        None
    };
    ERROR_CONSTRUCTORS
        .iter()
        .find(|c| name.as_deref() == Some(c.as_bytes()))
        .copied()
        .unwrap_or("Error")
}

pub(crate) fn own_message(scope: &mut Scope<'_>, error: &Value) -> Result {
    match quickjs::own_slot(scope, error, "message")? {
        OwnSlot::Data(message) => quickjs::to_js_string(scope, &message),
        OwnSlot::Accessor | OwnSlot::Missing => Ok(Value::undefined()),
    }
}

pub(crate) fn buffer_detached(scope: &mut Scope<'_>, buffer: &Value) -> bool {
    match scope.get(buffer, "detached") {
        Ok(d) => scope.to_bool(&d),
        Err(_) => false,
    }
}

pub(crate) fn geometry_kind(scope: &mut Scope<'_>, value: &Value) -> Option<usize> {
    let proto = scope
        .get_prototype(value)
        .unwrap_or_else(|_| Value::undefined());
    let object = global_get(scope, "Object");
    let plain = if object.is_object() {
        get(scope, &object, "prototype")
    } else {
        Value::undefined()
    };
    if !proto.is_object() || (plain.is_object() && proto.same_object(&plain)) {
        return None;
    }
    GEOMETRY.iter().position(|(name, _)| {
        let ctor = global_get(scope, name);
        ctor.is_object() && instance_of(scope, value, &ctor)
    })
}

pub(crate) fn geometry_plain(scope: &mut Scope<'_>, kind: usize, value: &Value) -> Result {
    let ctor = global_get(scope, GEOMETRY[kind].0);
    let proto = if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    };
    let to_json = if proto.is_object() {
        get(scope, &proto, "toJSON")
    } else {
        Value::undefined()
    };
    if scope.is_function(&to_json) {
        scope.call(&to_json, value, &[])
    } else {
        Ok(scope.new_object())
    }
}

pub(crate) fn geometry_construct(scope: &mut Scope<'_>, kind: usize, init: Value) -> Result {
    let (name, from) = GEOMETRY.get(kind).copied().unwrap_or(GEOMETRY[0]);
    let ctor = global_get(scope, name);
    let from = if ctor.is_object() {
        get(scope, &ctor, from)
    } else {
        Value::undefined()
    };
    if scope.is_function(&from) {
        scope.call(&from, &ctor, &[init])
    } else {
        Ok(scope.new_object())
    }
}

pub(crate) fn geometry_count() -> usize {
    GEOMETRY.len()
}

fn own_constructor(scope: &mut Scope<'_>, proto: &Value) -> Value {
    match quickjs::own_slot(scope, proto, "constructor") {
        Ok(OwnSlot::Data(ctor)) => ctor,
        _ => Value::undefined(),
    }
}

fn is_platform_instance(scope: &mut Scope<'_>, value: &Value) -> bool {
    let proto = scope
        .get_prototype(value)
        .unwrap_or_else(|_| Value::undefined());
    if !proto.is_object() {
        return false;
    }
    let ctor = own_constructor(scope, &proto);
    if !scope.is_function(&ctor) || !quickjs::is_engine_function(&ctor) {
        return false;
    }
    let name = get(scope, &ctor, "name");
    if !name.is_string() {
        return false;
    }
    let Ok(name) = scope.to_string(&name) else {
        return false;
    };
    if name.is_empty() {
        return false;
    }
    let exposed = global_get(scope, &name);
    exposed.same_object(&ctor)
}

struct Constructors {
    date: Value,
    regexp: Value,
    map: Value,
    set: Value,
    blob: Value,
    file: Value,
    data_view: Value,
    number: Value,
    string: Value,
    boolean: Value,
    array_buffer: Value,
    dom_exception: Value,
}

impl Constructors {
    fn of(scope: &mut Scope<'_>) -> Constructors {
        Constructors {
            date: global_get(scope, "Date"),
            regexp: global_get(scope, "RegExp"),
            map: global_get(scope, "Map"),
            set: global_get(scope, "Set"),
            blob: global_get(scope, "Blob"),
            file: global_get(scope, "File"),
            data_view: global_get(scope, "DataView"),
            number: global_get(scope, "Number"),
            string: global_get(scope, "String"),
            boolean: global_get(scope, "Boolean"),
            array_buffer: global_get(scope, "ArrayBuffer"),
            dom_exception: global_get(scope, "DOMException"),
        }
    }
}

struct Cloner {
    memo: HashMap<usize, (Value, Value)>,
    in_place: Vec<usize>,
    depth: u32,
    ctors: Constructors,
}

impl Cloner {
    fn remember(&mut self, original: &Value, clone: &Value) {
        self.memo
            .entry(quickjs::identity(original))
            .or_insert_with(|| (original.clone(), clone.clone()));
    }

    fn kept(&mut self, original: &Value, clone: Result) -> Result {
        if let Ok(clone) = &clone {
            self.remember(original, clone);
        }
        clone
    }

    fn clone(&mut self, scope: &mut Scope<'_>, value: &Value) -> Result {
        if self.depth >= MAX_DEPTH {
            return fail(scope);
        }
        self.depth += 1;
        let result = self.clone_value(scope, value);
        self.depth -= 1;
        result
    }

    fn copy_array_buffer(&mut self, scope: &mut Scope<'_>, source: &Value) -> Result {
        if buffer_detached(scope, source) {
            return fail(scope);
        }
        let resizable = get(scope, source, "resizable");
        let resizable = scope.to_bool(&resizable);
        let bytes = quickjs::array_buffer_bytes(scope, source).unwrap_or_default();
        if !resizable {
            return quickjs::array_buffer_copy(scope, &bytes);
        }
        let opts = scope.new_object();
        let max = get(scope, source, "maxByteLength");
        scope.set(&opts, "maxByteLength", max)?;
        let ctor = self.ctors.array_buffer.clone();
        let clone = scope.construct(&ctor, &[Value::int64(bytes.len() as i64), opts])?;
        quickjs::fill_array_buffer(scope, &clone, &bytes);
        Ok(clone)
    }

    fn iterate(
        &mut self,
        scope: &mut Scope<'_>,
        source: &Value,
        target: &Value,
        pairs: bool,
    ) -> Result<()> {
        let (iter_method, add) = if pairs {
            ("entries", "set")
        } else {
            ("values", "add")
        };
        let add = scope.get(target, add)?;
        let iter = call_method(scope, source, iter_method)?;
        let next = get(scope, &iter, "next");
        loop {
            let step = scope.call(&next, &iter, &[])?;
            let done = get(scope, &step, "done");
            if scope.to_bool(&done) {
                return Ok(());
            }
            let item = get(scope, &step, "value");
            let args = if pairs {
                let key = scope
                    .get_index(&item, 0)
                    .unwrap_or_else(|_| Value::undefined());
                let value = scope
                    .get_index(&item, 1)
                    .unwrap_or_else(|_| Value::undefined());
                let key = self.clone(scope, &key);
                let value = self.clone(scope, &value);
                vec![key?, value?]
            } else {
                vec![self.clone(scope, &item)?]
            };
            let _ = scope.call(&add, target, &args);
        }
    }

    fn clone_typed_array(&mut self, scope: &mut Scope<'_>, value: &Value, kind: i32) -> Result {
        let parts = match quickjs::typed_array_parts(scope, value) {
            Ok(parts) => parts,
            Err(_) => return fail(scope),
        };
        let buffer = if quickjs::is_array_buffer(&parts.buffer) {
            self.clone(scope, &parts.buffer)?
        } else {
            match quickjs::array_buffer_bytes(scope, &parts.buffer) {
                Some(bytes) => quickjs::array_buffer_copy(scope, &bytes)?,
                None => return fail(scope),
            }
        };
        let elements = parts
            .byte_length
            .checked_div(parts.bytes_per_element)
            .unwrap_or(0);
        let args = [
            buffer,
            Value::int64(parts.byte_offset as i64),
            Value::int64(elements as i64),
        ];
        let clone = quickjs::new_typed_array(scope, &args, kind);
        self.kept(value, clone)
    }

    fn clone_blob(&mut self, scope: &mut Scope<'_>, value: &Value, is_file: bool) -> Result {
        let bytes = scope.get(value, "__ndBlobBytes")?;
        let bytes = self.clone(scope, &bytes)?;
        let parts = scope.new_array();
        scope.set_index(&parts, 0, bytes)?;
        let opts = scope.new_object();
        let kind = get(scope, value, "type");
        scope.set(&opts, "type", kind)?;
        let clone = if is_file {
            let modified = get(scope, value, "lastModified");
            scope.set(&opts, "lastModified", modified)?;
            let name = get(scope, value, "name");
            let ctor = self.ctors.file.clone();
            scope.construct(&ctor, &[parts, name, opts])
        } else {
            let ctor = self.ctors.blob.clone();
            scope.construct(&ctor, &[parts, opts])
        };
        self.kept(value, clone)
    }

    fn clone_data_view(&mut self, scope: &mut Scope<'_>, value: &Value) -> Result {
        let Ok(offset) = scope.get(value, "byteOffset") else {
            return fail(scope);
        };
        let Ok(length) = scope.get(value, "byteLength") else {
            return fail(scope);
        };
        let buffer = get(scope, value, "buffer");
        let buffer = self.clone(scope, &buffer)?;
        let ctor = self.ctors.data_view.clone();
        let clone = scope.construct(&ctor, &[buffer, offset, length]);
        self.kept(value, clone)
    }

    fn clone_error(&mut self, scope: &mut Scope<'_>, value: &Value) -> Result {
        let name = get(scope, value, "name");
        let ctor_name = error_constructor(scope, &name);
        let message = own_message(scope, value)?;
        let ctor = global_get(scope, ctor_name);
        let clone = if !quickjs::is_constructor(scope, &ctor) {
            let clone = scope.new_error();
            if !message.is_undefined() {
                scope.define(&clone, "message", message, HIDDEN_PROPERTY)?;
            }
            clone
        } else if message.is_undefined() {
            scope.construct(&ctor, &[])?
        } else {
            scope.construct(&ctor, &[message])?
        };
        let stack = get(scope, value, "stack");
        if stack.is_string() {
            let _ = scope.set(&clone, "stack", stack);
        }
        if let Ok(cause) = scope.get(value, "cause")
            && !cause.is_undefined()
            && let Ok(cause) = self.clone(scope, &cause)
        {
            let _ = scope.set(&clone, "cause", cause);
        }
        self.remember(value, &clone);
        Ok(clone)
    }

    fn clone_boxed(&mut self, scope: &mut Scope<'_>, value: &Value, kind: i32) -> Result {
        let clone = match kind {
            quickjs::BOXED_NUMBER => {
                let n = scope.to_number(value).unwrap_or(0.0);
                let ctor = self.ctors.number.clone();
                scope.construct(&ctor, &[Value::number(n)])
            }
            quickjs::BOXED_STRING => {
                let primitive = quickjs::to_js_string(scope, value)?;
                let ctor = self.ctors.string.clone();
                scope.construct(&ctor, &[primitive])
            }
            quickjs::BOXED_BIGINT => {
                let primitive = call_method(scope, value, "valueOf")?;
                quickjs::to_object(scope, &primitive)
            }
            _ => {
                let primitive =
                    call_method(scope, value, "valueOf").unwrap_or_else(|_| Value::undefined());
                let truth = scope.to_bool(&primitive);
                let ctor = self.ctors.boolean.clone();
                scope.construct(&ctor, &[Value::boolean(truth)])
            }
        };
        self.kept(value, clone)
    }

    fn clone_plain(&mut self, scope: &mut Scope<'_>, value: &Value) -> Result {
        let is_array = value.is_array();
        let clone = if is_array {
            let clone = scope.new_array();
            let len = get(scope, value, "length");
            scope.set(&clone, "length", len)?;
            clone
        } else {
            scope.new_object()
        };
        self.remember(value, &clone);
        for key in quickjs::own_enumerable_string_keys(scope, value)? {
            let property = quickjs::get_by_key(scope, value, &key)?;
            let property = self.clone(scope, &property)?;
            quickjs::set_by_key(scope, &clone, &key, property)?;
        }
        Ok(clone)
    }

    fn clone_value(&mut self, scope: &mut Scope<'_>, value: &Value) -> Result {
        if quickjs::is_symbol(value) {
            return fail(scope);
        }
        if !value.is_object() {
            return Ok(value.clone());
        }
        if scope.is_function(value) {
            return fail(scope);
        }
        let id = quickjs::identity(value);
        if let Some((_, clone)) = self.memo.get(&id) {
            return Ok(clone.clone());
        }
        if self.in_place.contains(&id) {
            self.remember(value, value);
            return Ok(value.clone());
        }
        if ffi::is_host_object(value) || ffi::is_element(value) {
            return fail(scope);
        }
        if quickjs::is_array_buffer(value) {
            let clone = self.copy_array_buffer(scope, value);
            return self.kept(value, clone);
        }
        let typed = quickjs::typed_array_type(value);
        if typed >= 0 {
            return self.clone_typed_array(scope, value, typed);
        }
        if quickjs::is_date(value) {
            let time = call_method(scope, value, "getTime")?;
            let ctor = self.ctors.date.clone();
            let clone = scope.construct(&ctor, &[time]);
            return self.kept(value, clone);
        }
        if quickjs::is_regexp(value) {
            let source = get(scope, value, "source");
            let flags = get(scope, value, "flags");
            let ctor = self.ctors.regexp.clone();
            let clone = scope.construct(&ctor, &[source, flags]);
            return self.kept(value, clone);
        }
        let is_map = quickjs::is_map(value);
        if is_map || quickjs::is_set(value) {
            let ctor = if is_map {
                self.ctors.map.clone()
            } else {
                self.ctors.set.clone()
            };
            let clone = scope.construct(&ctor, &[])?;
            self.remember(value, &clone);
            self.iterate(scope, value, &clone, is_map)?;
            return Ok(clone);
        }
        let file = self.ctors.file.clone();
        let blob = self.ctors.blob.clone();
        if instance_of(scope, value, &file) {
            return self.clone_blob(scope, value, true);
        }
        if instance_of(scope, value, &blob) {
            return self.clone_blob(scope, value, false);
        }
        if quickjs::is_data_view(value) {
            return self.clone_data_view(scope, value);
        }
        let dom_exception = self.ctors.dom_exception.clone();
        if instance_of(scope, value, &dom_exception) {
            let message = get(scope, value, "message");
            let name = get(scope, value, "name");
            let clone = scope.construct(&dom_exception, &[message, name]);
            return self.kept(value, clone);
        }
        if quickjs::is_error(value) {
            return self.clone_error(scope, value);
        }
        let boxed = quickjs::boxed_kind(value);
        if matches!(
            boxed,
            quickjs::BOXED_NUMBER
                | quickjs::BOXED_STRING
                | quickjs::BOXED_BOOLEAN
                | quickjs::BOXED_BIGINT
        ) {
            return self.clone_boxed(scope, value, boxed);
        }
        if let Some(kind) = geometry_kind(scope, value) {
            let clone = geometry_plain(scope, kind, value)
                .and_then(|plain| geometry_construct(scope, kind, plain));
            return self.kept(value, clone);
        }
        if let Some(clone) = ffi::clone_canvas_object(scope, value) {
            return self.kept(value, clone);
        }
        if is_platform_instance(scope, value) {
            return fail(scope);
        }
        self.clone_plain(scope, value)
    }
}

fn run(
    scope: &mut Scope<'_>,
    value: &Value,
    in_place: Vec<usize>,
    seeds: (&Value, &Value),
) -> Result {
    let mut cloner = Cloner {
        memo: HashMap::new(),
        in_place,
        depth: 0,
        ctors: Constructors::of(scope),
    };
    let (from, to) = seeds;
    if from.is_array() && to.is_array() {
        for i in 0..length(scope, from) {
            let original = scope
                .get_index(from, i)
                .unwrap_or_else(|_| Value::undefined());
            let clone = scope
                .get_index(to, i)
                .unwrap_or_else(|_| Value::undefined());
            if original.is_object() && clone.is_object() {
                cloner.remember(&original, &clone);
            }
        }
    }
    cloner.clone(scope, value)
}

fn transferred_in_place(scope: &Scope<'_>, value: &Value) -> bool {
    ffi::is_port(scope, value) || ffi::is_image_bitmap(value)
}

pub(crate) fn clone_transfer(
    scope: &mut Scope<'_>,
    value: &Value,
    transfer: &Value,
    seeds: (&Value, &Value),
) -> Result {
    let len = if transfer.is_array() {
        length(scope, transfer)
    } else {
        0
    };
    let mut in_place = Vec::new();
    let mut seen = Vec::new();
    let mut buffers = Vec::new();
    for i in 0..len {
        let item = scope
            .get_index(transfer, i)
            .unwrap_or_else(|_| Value::undefined());
        if !item.is_object() {
            return Err(scope.type_error("structuredClone: transfer list entry is not an object"));
        }
        let id = quickjs::identity(&item);
        let fresh = !seen.contains(&id);
        seen.push(id);
        let is_buffer = quickjs::is_array_buffer(&item);
        if fresh && is_buffer && !buffer_detached(scope, &item) {
            buffers.push(item);
            continue;
        }
        if fresh && !is_buffer && transferred_in_place(scope, &item) {
            in_place.push(id);
        } else {
            return fail(scope);
        }
    }
    let clone = run(scope, value, in_place, seeds)?;
    for buffer in &buffers {
        let _ = scope.detach_array_buffer(buffer);
    }
    Ok(clone)
}
