//! Southstar — the wire graph worker messages cross runtimes in: each object becomes an array naming its kind, shared once, so references and cycles survive serialization.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use southstar_js_engine::{ElementType, ObjectKey, ObjectKind, Scope, Value};

use crate::{
    HIDDEN_PROPERTY, MAX_DEPTH, Result, call_method, element_index, error_constructor, fail, ffi,
    fill_buffer, get, global_get, instance_of, kind, length, new_view,
};

struct Node {
    array: Value,
    len: u32,
}

impl Node {
    fn new(scope: &mut Scope<'_>, kind: &str) -> Node {
        let array = scope.new_array();
        let kind = scope.string(kind);
        let _ = scope.set_index(&array, 0, kind);
        Node { array, len: 1 }
    }

    fn push(&mut self, scope: &mut Scope<'_>, item: Value) {
        let _ = scope.set_index(&self.array, self.len, item);
        self.len += 1;
    }

    fn done(self) -> Result {
        Ok(self.array)
    }
}

struct Encoder<'a> {
    memo: HashMap<ObjectKey, Value>,
    ports: &'a Value,
    blob: Value,
    file: Value,
    dom_exception: Value,
    depth: u32,
}

impl Encoder<'_> {
    fn remember(&mut self, original: &Value, node: &Value) {
        if let Some(key) = original.object_key() {
            self.memo.insert(key, node.clone());
        }
    }

    fn encode(&mut self, scope: &mut Scope<'_>, value: &Value) -> Result {
        if value.is_symbol() {
            return fail(scope);
        }
        let Some(id) = value.object_key() else {
            return Ok(value.clone());
        };
        if let Some(node) = self.memo.get(&id) {
            return Ok(node.clone());
        }
        if self.depth >= MAX_DEPTH {
            return fail(scope);
        }
        self.depth += 1;
        let node = self.encode_object(scope, value);
        self.depth -= 1;
        if let Ok(node) = &node {
            self.memo.entry(id).or_insert_with(|| node.clone());
        }
        node
    }

    fn encode_props(
        &mut self,
        scope: &mut Scope<'_>,
        value: &Value,
        node: &mut Node,
    ) -> Result<()> {
        for key in scope.own_enumerable_keys(value)? {
            let property = scope.get_key(value, &key)?;
            let encoded = self.encode(scope, &property)?;
            node.push(scope, key);
            node.push(scope, encoded);
        }
        Ok(())
    }

    fn encode_entries(
        &mut self,
        scope: &mut Scope<'_>,
        source: &Value,
        pairs: bool,
        node: &mut Node,
    ) -> Result<()> {
        let iter = call_method(scope, source, if pairs { "entries" } else { "values" })?;
        let next = get(scope, &iter, "next");
        loop {
            let step = scope.call(&next, &iter, &[])?;
            let done = get(scope, &step, "done");
            if scope.to_bool(&done) {
                return Ok(());
            }
            let item = get(scope, &step, "value");
            if pairs {
                for k in 0..2 {
                    let part = scope
                        .get_index(&item, k)
                        .unwrap_or_else(|_| Value::undefined());
                    let encoded = self.encode(scope, &part)?;
                    node.push(scope, encoded);
                }
            } else {
                let encoded = self.encode(scope, &item)?;
                node.push(scope, encoded);
            }
        }
    }

    fn encode_error(&mut self, scope: &mut Scope<'_>, value: &Value) -> Result {
        let name = scope.get(value, "name")?;
        let ctor = error_constructor(scope, &name);
        let mut node = Node::new(scope, "E");
        let ctor = scope.string(ctor);
        node.push(scope, ctor);
        self.remember(value, &node.array);
        let message = crate::own_message(scope, value)?;
        node.push(scope, message);
        let stack = get(scope, value, "stack");
        let stack = if stack.is_string() {
            stack
        } else {
            Value::undefined()
        };
        node.push(scope, stack);
        if let Ok(cause) = scope.get(value, "cause")
            && !cause.is_undefined()
        {
            let encoded = self.encode(scope, &cause)?;
            node.push(scope, encoded);
        }
        node.done()
    }

    fn encode_port(&mut self, scope: &mut Scope<'_>, value: &Value) -> Result {
        let count = if self.ports.is_array() {
            length(scope, self.ports)
        } else {
            0
        };
        for i in 0..count {
            let port = scope
                .get_index(self.ports, i)
                .unwrap_or_else(|_| Value::undefined());
            if port.same_object(value) {
                let mut node = Node::new(scope, "P");
                node.push(scope, Value::int(i as i32));
                return node.done();
            }
        }
        fail(scope)
    }

    fn encode_object(&mut self, scope: &mut Scope<'_>, value: &Value) -> Result {
        if scope.is_function(value) || ffi::is_element(value) {
            return fail(scope);
        }
        if ffi::is_port(scope, value) {
            return self.encode_port(scope, value);
        }
        let object_kind = kind(scope, value);
        if object_kind == ObjectKind::ArrayBuffer {
            if crate::buffer_detached(scope, value) {
                return fail(scope);
            }
            let mut node = Node::new(scope, "AB");
            let bytes = scope.array_buffer_bytes(value).unwrap_or_default();
            let copy = scope.new_array_buffer(&bytes)?;
            node.push(scope, copy);
            let resizable = get(scope, value, "resizable");
            if scope.to_bool(&resizable) {
                let max = get(scope, value, "maxByteLength");
                node.push(scope, max);
            }
            return node.done();
        }
        if let ObjectKind::TypedArray(_) = object_kind {
            let Ok(Some(view)) = scope.typed_array_view(value) else {
                return fail(scope);
            };
            let buffer = self.encode(scope, &view.buffer)?;
            let mut node = Node::new(scope, "TA");
            node.push(scope, Value::int(element_index(view.element)));
            node.push(scope, buffer);
            node.push(scope, Value::int64(view.byte_offset as i64));
            node.push(scope, Value::int64(view.length as i64));
            return node.done();
        }
        if object_kind == ObjectKind::DataView {
            let buffer = scope.get(value, "buffer")?;
            let buffer = self.encode(scope, &buffer)?;
            let mut node = Node::new(scope, "DV");
            node.push(scope, buffer);
            let offset = get(scope, value, "byteOffset");
            node.push(scope, offset);
            let length = get(scope, value, "byteLength");
            node.push(scope, length);
            return node.done();
        }
        if object_kind == ObjectKind::Date {
            let time = call_method(scope, value, "getTime")?;
            let mut node = Node::new(scope, "D");
            node.push(scope, time);
            return node.done();
        }
        if object_kind == ObjectKind::RegExp {
            let mut node = Node::new(scope, "RE");
            let source = get(scope, value, "source");
            node.push(scope, source);
            let flags = get(scope, value, "flags");
            node.push(scope, flags);
            return node.done();
        }
        let is_map = object_kind == ObjectKind::Map;
        if is_map || object_kind == ObjectKind::Set {
            let mut node = Node::new(scope, if is_map { "M" } else { "S" });
            self.remember(value, &node.array);
            self.encode_entries(scope, value, is_map, &mut node)?;
            return node.done();
        }
        let file = self.file.clone();
        let blob = self.blob.clone();
        let is_file = instance_of(scope, value, &file);
        if is_file || instance_of(scope, value, &blob) {
            let bytes = scope.get(value, "__ndBlobBytes")?;
            let bytes = self.encode(scope, &bytes)?;
            let mut node = Node::new(scope, if is_file { "FI" } else { "BL" });
            node.push(scope, bytes);
            let kind = get(scope, value, "type");
            node.push(scope, kind);
            if is_file {
                let name = get(scope, value, "name");
                node.push(scope, name);
                let modified = get(scope, value, "lastModified");
                node.push(scope, modified);
            }
            return node.done();
        }
        if let Some(kind) = crate::geometry_kind(scope, value) {
            let plain = crate::geometry_plain(scope, kind, value)?;
            let plain = self.encode(scope, &plain)?;
            let mut node = Node::new(scope, "GE");
            node.push(scope, Value::int(kind as i32));
            node.push(scope, plain);
            return node.done();
        }
        if ffi::is_image_data(value) {
            let data = ffi::hidden_get(scope, value, c"data");
            let data = self.encode(scope, &data)?;
            let mut node = Node::new(scope, "IM");
            node.push(scope, data);
            for key in [c"width", c"height", c"colorSpace"] {
                let field = ffi::hidden_get(scope, value, key);
                node.push(scope, field);
            }
            return node.done();
        }
        let dom_exception = self.dom_exception.clone();
        if instance_of(scope, value, &dom_exception) {
            let mut node = Node::new(scope, "DE");
            let message = get(scope, value, "message");
            node.push(scope, message);
            let name = get(scope, value, "name");
            node.push(scope, name);
            return node.done();
        }
        if object_kind == ObjectKind::Error {
            return self.encode_error(scope, value);
        }
        if matches!(
            object_kind,
            ObjectKind::Number | ObjectKind::String | ObjectKind::Boolean | ObjectKind::BigInt
        ) {
            let primitive = call_method(scope, value, "valueOf")?;
            let mut node = Node::new(scope, "BX");
            node.push(scope, primitive);
            return node.done();
        }
        let is_array = value.is_array();
        let mut node = Node::new(scope, if is_array { "A" } else { "O" });
        if is_array {
            let len = get(scope, value, "length");
            node.push(scope, len);
        }
        self.remember(value, &node.array);
        self.encode_props(scope, value, &mut node)?;
        node.done()
    }
}

pub(crate) fn encode_value(scope: &mut Scope<'_>, value: &Value, ports: &Value) -> Result {
    let mut encoder = Encoder {
        memo: HashMap::new(),
        ports,
        blob: global_get(scope, "Blob"),
        file: global_get(scope, "File"),
        dom_exception: global_get(scope, "DOMException"),
        depth: 0,
    };
    encoder.encode(scope, value)
}

struct Decoder<'a> {
    memo: HashMap<ObjectKey, Value>,
    ports: &'a Value,
    depth: u32,
}

fn construct(scope: &mut Scope<'_>, name: &str, args: &[Value]) -> Result {
    let ctor = global_get(scope, name);
    if scope.is_constructor(&ctor) {
        scope.construct(&ctor, args)
    } else {
        Ok(scope.new_object())
    }
}

fn options(scope: &mut Scope<'_>, fields: &[(&str, &Value)]) -> Result {
    let opts = scope.new_object();
    for (name, value) in fields {
        scope.set(&opts, name, (*value).clone())?;
    }
    Ok(opts)
}

impl Decoder<'_> {
    fn remember(&mut self, node: &Value, value: &Value) {
        if let Some(key) = node.object_key() {
            self.memo.insert(key, value.clone());
        }
    }

    fn decode(&mut self, scope: &mut Scope<'_>, node: &Value) -> Result {
        let Some(id) = node.object_key() else {
            return Ok(node.clone());
        };
        if let Some(value) = self.memo.get(&id) {
            return Ok(value.clone());
        }
        if !node.is_array() || self.depth >= MAX_DEPTH {
            return Ok(Value::null());
        }
        let kind = scope
            .get_index(node, 0)
            .unwrap_or_else(|_| Value::undefined());
        if !kind.is_string() {
            return Ok(Value::null());
        }
        let Ok(kind) = scope.to_string(&kind) else {
            return Ok(Value::null());
        };
        self.depth += 1;
        let value = self.decode_node(scope, node, &kind);
        self.depth -= 1;
        if let Ok(value) = &value {
            self.memo.entry(id).or_insert_with(|| value.clone());
        }
        value
    }

    fn decode_calls(
        &mut self,
        scope: &mut Scope<'_>,
        node: &Value,
        per: u32,
        target: &Value,
        method: &str,
    ) -> Result<()> {
        let function = get(scope, target, method);
        let n = length(scope, node);
        let mut i = 1;
        while i + per <= n {
            let mut args = Vec::with_capacity(per as usize);
            for k in 0..per {
                let raw = scope
                    .get_index(node, i + k)
                    .unwrap_or_else(|_| Value::undefined());
                args.push(self.decode(scope, &raw)?);
            }
            scope.call(&function, target, &args)?;
            i += per;
        }
        Ok(())
    }

    fn decode_props(
        &mut self,
        scope: &mut Scope<'_>,
        node: &Value,
        first: u32,
        target: &Value,
    ) -> Result<()> {
        let n = length(scope, node);
        let mut i = first;
        while i + 1 < n {
            let key = scope
                .get_index(node, i)
                .unwrap_or_else(|_| Value::undefined());
            let raw = scope
                .get_index(node, i + 1)
                .unwrap_or_else(|_| Value::undefined());
            let value = self.decode(scope, &raw)?;
            scope.define_entry(target, &key, value)?;
            i += 2;
        }
        Ok(())
    }

    fn decode_node(&mut self, scope: &mut Scope<'_>, node: &Value, kind: &str) -> Result {
        let [a1, a2, a3, a4] = [1, 2, 3, 4].map(|i| {
            scope
                .get_index(node, i)
                .unwrap_or_else(|_| Value::undefined())
        });
        match kind {
            "P" => {
                let index = scope.to_number(&a1).map_or(0, |n| n as i64 as u32);
                if self.ports.is_array() {
                    Ok(scope
                        .get_index(self.ports, index)
                        .unwrap_or_else(|_| Value::undefined()))
                } else {
                    Ok(Value::null())
                }
            }
            "AB" => {
                if a2.is_undefined() {
                    return Ok(a1);
                }
                let bytes = scope.array_buffer_bytes(&a1).unwrap_or_default();
                let opts = options(scope, &[("maxByteLength", &a2)])?;
                let out = construct(
                    scope,
                    "ArrayBuffer",
                    &[Value::int64(bytes.len() as i64), opts],
                )?;
                fill_buffer(scope, &out, &bytes);
                Ok(out)
            }
            "TA" => {
                let buffer = self.decode(scope, &a2)?;
                let typed = scope.to_int32(&a1).unwrap_or(-1);
                match usize::try_from(typed)
                    .ok()
                    .and_then(|i| ElementType::ALL.get(i))
                {
                    Some(element) => new_view(scope, *element, &[buffer, a3, a4]),
                    None => Ok(Value::null()),
                }
            }
            "DV" => {
                let buffer = self.decode(scope, &a1)?;
                construct(scope, "DataView", &[buffer, a2, a3])
            }
            "D" => construct(scope, "Date", &[a1]),
            "RE" => construct(scope, "RegExp", &[a1, a2]),
            "M" | "S" => {
                let is_map = kind == "M";
                let out = construct(scope, if is_map { "Map" } else { "Set" }, &[])?;
                self.remember(node, &out);
                let (per, method) = if is_map { (2, "set") } else { (1, "add") };
                self.decode_calls(scope, node, per, &out, method)?;
                Ok(out)
            }
            "BL" | "FI" => {
                let bytes = self.decode(scope, &a1)?;
                let parts = scope.new_array();
                scope.set_index(&parts, 0, bytes)?;
                if kind == "FI" {
                    let opts = options(scope, &[("type", &a2), ("lastModified", &a4)])?;
                    construct(scope, "File", &[parts, a3, opts])
                } else {
                    let opts = options(scope, &[("type", &a2)])?;
                    construct(scope, "Blob", &[parts, opts])
                }
            }
            "GE" => {
                let geometry = scope.to_int32(&a1).unwrap_or(0);
                let init = self.decode(scope, &a2)?;
                if geometry >= 0 && (geometry as usize) < crate::geometry_count() {
                    crate::geometry_construct(scope, geometry as usize, init)
                } else {
                    Ok(scope.new_object())
                }
            }
            "IM" => {
                let data = self.decode(scope, &a1)?;
                let opts = options(scope, &[("colorSpace", &a4)])?;
                construct(scope, "ImageData", &[data, a2, a3, opts])
            }
            "DE" => construct(scope, "DOMException", &[a1, a2]),
            "E" => {
                let ctor = scope
                    .to_string(&a1)
                    .unwrap_or_else(|_| String::from("Error"));
                let args = if a2.is_undefined() { vec![] } else { vec![a2] };
                let out = construct(scope, &ctor, &args)?;
                self.remember(node, &out);
                if a3.is_string() {
                    let _ = scope.define(&out, "stack", a3, HIDDEN_PROPERTY);
                }
                if length(scope, node) > 4 {
                    let cause = self.decode(scope, &a4)?;
                    let _ = scope.define(&out, "cause", cause, HIDDEN_PROPERTY);
                }
                Ok(out)
            }
            "BX" => scope.to_object(&a1),
            "A" | "O" => {
                let is_array = kind == "A";
                let out = if is_array {
                    let out = scope.new_array();
                    let _ = scope.set(&out, "length", a1);
                    out
                } else {
                    scope.new_object()
                };
                self.remember(node, &out);
                self.decode_props(scope, node, if is_array { 2 } else { 1 }, &out)?;
                Ok(out)
            }
            _ => Ok(Value::null()),
        }
    }
}

pub(crate) fn decode_value(scope: &mut Scope<'_>, wire: &Value, ports: &Value) -> Result {
    let mut decoder = Decoder {
        memo: HashMap::new(),
        ports,
        depth: 0,
    };
    decoder.decode(scope, wire)
}
