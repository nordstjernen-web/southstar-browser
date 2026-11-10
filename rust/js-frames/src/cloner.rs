//! Southstar — the realm cloner: gives a frame realm its own copies of the page realm's platform objects, functions and prototypes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use southstar_js_engine::{Attributes, Value};

use crate::ffi::ctx::{Ctx, Descriptor, Key, Keys};
use crate::ffi::{self, Js};
use crate::page;
use crate::pristine::{self, Pristine};

const INTRINSIC_NAMES: &[&str] = &[
    "Object",
    "Function",
    "Array",
    "Number",
    "Boolean",
    "String",
    "Symbol",
    "BigInt",
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "AggregateError",
    "Promise",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "WeakRef",
    "FinalizationRegistry",
    "ArrayBuffer",
    "SharedArrayBuffer",
    "DataView",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float16Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    "Date",
    "RegExp",
    "Proxy",
    "Reflect",
    "JSON",
    "Math",
    "Atomics",
    "Iterator",
    "DOMException",
];

const INTRINSIC_PROBE: &str = "(function(){ var a = [][Symbol.iterator](); var g = function*(){}; var ag = async function*(){}; var af = async function(){}; return [Object.getPrototypeOf(a), Object.getPrototypeOf(Object.getPrototypeOf(a)), Object.getPrototypeOf(g), Object.getPrototypeOf(ag), Object.getPrototypeOf(af), Object.getPrototypeOf(new Map()[Symbol.iterator]()), Object.getPrototypeOf(new Set()[Symbol.iterator]()), Object.getPrototypeOf(''[Symbol.iterator]())]; })()";

const SINGLETON_NAMES: &[&str] = &[
    "navigator",
    "screen",
    "crypto",
    "localStorage",
    "sessionStorage",
    "caches",
    "indexedDB",
    "cookieStore",
    "trustedTypes",
    "visualViewport",
    "scheduler",
    "navigation",
    "external",
    "locationbar",
    "menubar",
    "personalbar",
    "scrollbars",
    "statusbar",
    "toolbar",
    "console",
    "speechSynthesis",
    "styleMedia",
    "customElements",
];

const ALL: Attributes = Attributes {
    writable: true,
    enumerable: true,
    configurable: true,
};
const MAX_DEPTH: u32 = 32;
const LOCALIZED_DEPTH: u32 = 6;
const LOCALIZED_ITEMS: i32 = 1 << 20;
const SHELL_CHAIN: usize = 16;

pub(crate) type Shared = Rc<RefCell<Cloner>>;

pub(crate) struct Cloner {
    src: Ctx,
    dst: Ctx,
    memo: HashMap<usize, (Value, Value)>,
    pristine: Option<Rc<Pristine>>,
    isolated: bool,
}

fn identity(value: &Value) -> usize {
    value.object_identity() as usize
}

fn data_descriptor(value: Value, like: &Descriptor) -> Descriptor {
    Descriptor {
        value,
        getter: Value::undefined(),
        setter: Value::undefined(),
        accessor: false,
        configurable: like.configurable,
        enumerable: like.enumerable,
        writable: like.writable,
    }
}

impl Cloner {
    fn new(
        src: Ctx,
        main: Ctx,
        dst: Ctx,
        pristine: Option<Rc<Pristine>>,
        isolated: bool,
    ) -> Cloner {
        let mut cloner = Cloner {
            src,
            dst,
            memo: HashMap::new(),
            pristine,
            isolated,
        };
        cloner.seed(src);
        if main != src {
            cloner.seed(main);
        }
        if ffi::debug_realm() {
            cloner.debug_report(main);
        }
        cloner
    }

    fn debug_report(&self, main: Ctx) {
        let src = self.src;
        let global = src.global();
        let object = src.get(&global, "Object");
        let object_proto = src.get(&object, "prototype");
        let error = src.get(&global, "Error");
        let error_proto = src.get(&error, "prototype");
        eprintln!(
            "NSREALM memo={} objproto={} errproto={} src==main:{}",
            self.memo.len(),
            self.memo.contains_key(&identity(&object_proto)) as i32,
            self.memo.contains_key(&identity(&error_proto)) as i32,
            (src == main) as i32
        );
    }

    fn put(&mut self, from: &Value, to: &Value) {
        if !from.is_object() || !to.is_object() {
            return;
        }
        self.memo
            .entry(identity(from))
            .or_insert_with(|| (from.clone(), to.clone()));
    }

    fn lookup(&self, from: &Value) -> Option<&Value> {
        if !from.is_object() {
            return None;
        }
        self.memo.get(&identity(from)).map(|(_, to)| to)
    }

    fn map_pair(&mut self, from: &Value, to: &Value) {
        self.put(from, to);
        if !from.is_object() || !to.is_object() {
            return;
        }
        let from_proto = self.src.get(from, "prototype");
        let to_proto = self.dst.get(to, "prototype");
        self.put(&from_proto, &to_proto);
    }

    fn map_proto_of(&mut self, from: &Value, to: &Value) {
        let from_proto = self.src.prototype(from);
        let to_proto = self.dst.prototype(to);
        self.map_pair(&from_proto, &to_proto);
    }

    fn seed(&mut self, src: Ctx) {
        let dst = self.dst;
        let src_global = src.global();
        let dst_global = dst.global();
        for name in INTRINSIC_NAMES {
            let from = src.get(&src_global, name);
            let to = dst.get(&dst_global, name);
            self.map_pair(&from, &to);
        }
        let from_u8 = src.get(&src_global, "Uint8Array");
        let to_u8 = dst.get(&dst_global, "Uint8Array");
        if from_u8.is_object() && to_u8.is_object() {
            self.map_proto_of(&from_u8, &to_u8);
        }
        let from_list = src.eval_hidden(INTRINSIC_PROBE, c"<realm-intrinsics>");
        let to_list = dst.eval_hidden(INTRINSIC_PROBE, c"<realm-intrinsics>");
        if let (Some(from_list), Some(to_list)) = (from_list, to_list)
            && from_list.is_array()
            && to_list.is_array()
        {
            let length = src.get(&from_list, "length");
            let count = src.to_int32(&length).max(0) as u32;
            for index in 0..count {
                let from = src
                    .try_enter(|scope| scope.get_index(&from_list, index).ok())
                    .flatten()
                    .unwrap_or_else(Value::undefined);
                let to = dst
                    .try_enter(|scope| scope.get_index(&to_list, index).ok())
                    .flatten()
                    .unwrap_or_else(Value::undefined);
                self.map_pair(&from, &to);
            }
        }
    }

    fn shared(&self, value: &Value) -> Value {
        if self.isolated {
            Value::undefined()
        } else {
            value.clone()
        }
    }

    fn clone_value(&mut self, value: &Value, depth: u32) -> Value {
        if !value.is_object() {
            return value.clone();
        }
        if let Some(hit) = self.lookup(value) {
            return hit.clone();
        }
        if depth > MAX_DEPTH {
            return self.shared(value);
        }
        let Some(mut out) = self.dst.clone_native_function(value) else {
            return self.shared(value);
        };
        if out.is_undefined() {
            if self.src.is_function(value) {
                return self.clone_js_function(value, depth);
            }
            match self.new_shell(value) {
                Some(shell) => out = shell,
                None => return self.shared(value),
            }
        }
        self.put(value, &out);
        self.clone_prototype(value, &out, depth);
        self.clone_own_properties(value, &out, depth);
        out
    }

    fn clone_descriptor_into(&mut self, to: &Value, key: &Key, desc: &Descriptor, depth: u32) {
        let copy = if desc.accessor {
            Descriptor {
                value: Value::undefined(),
                getter: self.clone_value(&desc.getter, depth + 1),
                setter: self.clone_value(&desc.setter, depth + 1),
                accessor: true,
                configurable: desc.configurable,
                enumerable: desc.enumerable,
                writable: false,
            }
        } else {
            data_descriptor(self.clone_value(&desc.value, depth + 1), desc)
        };
        self.dst.define(to, key, &copy);
    }

    fn clone_own_properties(&mut self, from: &Value, to: &Value, depth: u32) {
        if let Some(pristine) = self.pristine.clone()
            && let Some(properties) = pristine.own_properties(from)
        {
            for (key, desc) in properties {
                self.clone_descriptor_into(to, key, desc, depth);
            }
            return;
        }
        let src = self.src;
        let Some(keys) = src.own_keys(from, Keys::StringsAndSymbols) else {
            return;
        };
        for key in &keys {
            if let Some(desc) = src.own_property(from, key) {
                self.clone_descriptor_into(to, key, &desc, depth);
            }
        }
    }

    fn object_is_shape(&self, value: &Value) -> bool {
        let src = self.src;
        let proto = src.prototype(value);
        let mut plain_namespace = proto.is_null();
        if proto.is_object() {
            let dst = self.dst;
            let dst_global = dst.global();
            let dst_object = dst.get(&dst_global, "Object");
            let dst_object_proto = dst.get(&dst_object, "prototype");
            plain_namespace = self
                .lookup(&proto)
                .is_some_and(|mapped| mapped.same_object(&dst_object_proto));
        }
        if plain_namespace {
            return true;
        }
        let constructor = Key::new(src, c"constructor");
        src.has_own(value, &constructor).unwrap_or(false)
    }

    fn adopt_prototype(&mut self, object: &Value) {
        let proto = self.src.prototype(object);
        if proto.is_object() {
            let cloned = self.clone_value(&proto, 0);
            self.dst.set_prototype(object, &cloned);
        }
    }

    fn adopt_property(&mut self, object: &Value, key: &Key) {
        let Some(desc) = self.src.own_property(object, key) else {
            return;
        };
        let accessor = desc.accessor;
        let function = !accessor && self.src.is_function(&desc.value);
        if desc.configurable && (accessor || function) {
            self.clone_descriptor_into(object, key, &desc, 0);
        }
    }

    fn adopt_in_place(&mut self, object: &Value) {
        if !object.is_object() {
            return;
        }
        self.adopt_prototype(object);
        let Some(keys) = self.src.own_keys(object, Keys::StringsAndSymbols) else {
            return;
        };
        for key in &keys {
            self.adopt_property(object, key);
        }
    }

    fn fn_is_interface(&self, function: &Value) -> bool {
        let src = self.src;
        if !src.is_constructor(function) {
            return false;
        }
        let name = src.get(function, "name");
        if name.is_string()
            && src
                .to_string(&name)
                .is_some_and(|name| name.as_bytes().first().is_some_and(u8::is_ascii_uppercase))
        {
            return true;
        }
        let proto = src.get(function, "prototype");
        proto.is_object()
            && src
                .own_keys(&proto, Keys::StringsAndSymbols)
                .is_some_and(|keys| keys.len() > 1)
    }

    fn forwarder_for(&self, function: &Value, constructor: bool) -> Option<Value> {
        let src = self.src;
        let name_value = src.get(function, "name");
        let name = if name_value.is_string() {
            src.to_string(&name_value).unwrap_or_default()
        } else {
            String::new()
        };
        let length_value = src.get(function, "length");
        let length = if length_value.is_number() {
            src.to_int32(&length_value)
        } else {
            0
        };
        let localize = self.isolated.then(|| ffi::localize_function(self.dst));
        self.dst
            .new_forwarder(function, &name, length, constructor, localize.as_ref())
    }

    fn ctor_owns_proto(&self, constructor: &Value) -> bool {
        let marker = self.src.get(constructor, "__ndRealmProto");
        self.src.to_bool(&marker)
    }

    fn forwarder_property(
        &mut self,
        function: &Value,
        out: &Value,
        key: &Key,
        is_proto: bool,
        depth: u32,
    ) {
        let Some(desc) = self.src.own_property(function, key) else {
            return;
        };
        if !self.isolated
            && !desc.accessor
            && is_proto
            && desc.value.is_object()
            && !self.ctor_owns_proto(function)
        {
            self.put(&desc.value, &desc.value);
            self.dst.define(out, key, &desc);
            return;
        }
        self.clone_descriptor_into(out, key, &desc, depth);
    }

    fn clone_js_function(&mut self, function: &Value, depth: u32) -> Value {
        let constructor = self.fn_is_interface(function);
        let Some(out) = self.forwarder_for(function, constructor) else {
            return self.shared(function);
        };
        self.put(function, &out);
        let src = self.src;
        let Some(keys) = src.own_keys(function, Keys::StringsAndSymbols) else {
            return out;
        };
        let name = Key::new(src, c"name");
        let length = Key::new(src, c"length");
        let prototype = Key::new(src, c"prototype");
        for key in &keys {
            if key.same(&name) || key.same(&length) {
                continue;
            }
            let is_proto = key.same(&prototype);
            if is_proto && !constructor {
                continue;
            }
            self.forwarder_property(function, &out, key, is_proto, depth);
        }
        out
    }

    fn clone_prototype(&mut self, value: &Value, out: &Value, depth: u32) {
        let proto = self.src.prototype(value);
        if proto.is_object() {
            let cloned = self.clone_value(&proto, depth + 1);
            self.dst.set_prototype(out, &cloned);
        } else if proto.is_null() {
            self.dst.set_prototype(out, &Value::null());
        }
    }

    fn new_shell(&self, value: &Value) -> Option<Value> {
        if ffi::is_window_named(value) {
            return ffi::new_window_named(self.dst);
        }
        if !self.src.is_plain_object(value) || !self.object_is_shape(value) {
            return None;
        }
        self.dst.new_object_proto(&Value::null())
    }

    fn clone_class_protos(&mut self) {
        for class in self.dst.registered_classes() {
            if self.dst.class_proto(class).is_object() {
                continue;
            }
            let src_proto = self.src.class_proto(class);
            if src_proto.is_object() {
                let cloned = self.clone_value(&src_proto, 0);
                self.dst.set_class_proto(class, cloned);
            }
        }
    }

    fn clone_storage(&mut self, value: &Value, area: i32) -> Value {
        let Some(out) = ffi::new_storage(self.dst, area) else {
            return self.shared(value);
        };
        self.put(value, &out);
        out
    }

    fn instance_is_plain(&self, value: &Value, depth: u32) -> bool {
        self.src.is_plain_object(value) && depth <= 3 && !value.is_array()
    }

    fn clone_instance_property(&mut self, out: &Value, key: &Key, desc: &Descriptor, depth: u32) {
        if desc.accessor {
            self.clone_descriptor_into(out, key, desc, depth);
        } else {
            let cloned = self.clone_instance(&desc.value, depth + 1);
            self.dst.define(out, key, &data_descriptor(cloned, desc));
        }
    }

    fn clone_instance_properties(&mut self, value: &Value, out: &Value, depth: u32) {
        let src = self.src;
        src.copy_private_elements(value, out);
        if self.isolated
            && let Some(pristine) = self.pristine.clone()
            && let Some(properties) = pristine.own_properties(value)
        {
            for (key, desc) in properties {
                self.clone_instance_property(out, key, desc, depth);
            }
            return;
        }
        let Some(keys) = src.own_keys(value, Keys::StringsAndSymbols) else {
            return;
        };
        for key in &keys {
            if let Some(desc) = src.own_property(value, key) {
                self.clone_instance_property(out, key, &desc, depth);
            }
        }
    }

    fn clone_plain_instance(&mut self, value: &Value, depth: u32) -> Value {
        let proto = self.src.prototype(value);
        let cloned_proto = if proto.is_object() {
            self.clone_value(&proto, 0)
        } else {
            proto
        };
        let Some(out) = self.dst.new_object_proto(&cloned_proto) else {
            return self.shared(value);
        };
        self.put(value, &out);
        self.clone_instance_properties(value, &out, depth);
        out
    }

    fn clone_instance(&mut self, value: &Value, depth: u32) -> Value {
        if !value.is_object() || self.src.is_function(value) {
            return self.clone_value(value, depth);
        }
        if let Some(hit) = self.lookup(value) {
            return hit.clone();
        }
        if let Some(area) = ffi::storage_area(value) {
            return self.clone_storage(value, area);
        }
        if !self.instance_is_plain(value, depth) {
            return if self.isolated {
                self.localize(value, depth)
            } else {
                value.clone()
            };
        }
        self.clone_plain_instance(value, depth)
    }

    fn localized_buffer(&mut self, value: &Value) -> Option<Value> {
        let src = self.src;
        let element = src.try_enter(|scope| scope.typed_array_element(value))?;
        let bytes = src
            .try_enter(|scope| scope.with_buffer_bytes_mut(value, |bytes| bytes.to_vec()))
            .flatten()?;
        self.dst
            .try_enter(|scope| match element {
                Some(element) => scope.new_typed_array(element, &bytes).ok(),
                None => scope.new_array_buffer(&bytes).ok(),
            })
            .flatten()
    }

    fn localized_array(&mut self, value: &Value, depth: u32) -> Value {
        let src = self.src;
        let length = src
            .to_int32(&src.get(value, "length"))
            .clamp(0, LOCALIZED_ITEMS) as u32;
        let out = self.dst.enter(|scope| scope.new_array());
        for index in 0..length {
            let item = src
                .try_enter(|scope| scope.get_index(value, index).ok())
                .flatten()
                .unwrap_or_else(Value::undefined);
            let item = self.localize(&item, depth + 1);
            self.dst
                .try_enter(|scope| scope.set_index(&out, index, item).ok());
        }
        out
    }

    fn localized_error(&mut self, value: &Value) -> Value {
        let (src, dst) = (self.src, self.dst);
        let out = dst.enter(|scope| scope.new_error());
        for field in ["name", "message"] {
            let text = src.get(value, field);
            if text.is_string() {
                let key = Key::new(dst, &std::ffi::CString::new(field).unwrap_or_default());
                dst.define_value(&out, &key, text, ALL);
            }
        }
        out
    }

    fn bound(&mut self, function: &Value, this: &Value) -> Value {
        let own = self.clone_value(function, 0);
        let dst = self.dst;
        let bind = dst.get(&own, "bind");
        if !dst.is_function(&own) || !dst.is_function(&bind) {
            return Value::undefined();
        }
        dst.call(&bind, &own, core::slice::from_ref(this))
            .unwrap_or_else(Value::undefined)
    }

    fn holder_properties(&self, holder: &Value) -> Vec<(Key, Descriptor)> {
        let src = self.src;
        src.own_keys(holder, Keys::StringsAndSymbols)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|key| {
                let descriptor = src.own_property(holder, &key)?;
                Some((key, descriptor))
            })
            .collect()
    }

    fn bind_member(&mut self, out: &Value, host: &Value, key: &Key, desc: &Descriptor, depth: u32) {
        let src = self.src;
        let copy = if desc.accessor {
            Descriptor {
                value: Value::undefined(),
                getter: self.bound(&desc.getter, host),
                setter: self.bound(&desc.setter, host),
                accessor: true,
                configurable: true,
                enumerable: desc.enumerable,
                writable: false,
            }
        } else if src.is_function(&desc.value) {
            data_descriptor(self.bound(&desc.value, host), desc)
        } else {
            data_descriptor(self.localize(&desc.value, depth + 1), desc)
        };
        self.dst.define(out, key, &copy);
    }

    fn bound_shell(&mut self, host: &Value, depth: u32) -> Value {
        let (src, dst) = (self.src, self.dst);
        let Some(out) = dst.new_object_proto(&dst.prototype(&dst.new_object_literal())) else {
            return Value::undefined();
        };
        let constructor = Key::new(src, c"constructor");
        let mut holder = host.clone();
        for _ in 0..SHELL_CHAIN {
            if !holder.is_object() || src.is_function(&holder) {
                break;
            }
            let pristine = self.pristine.clone();
            let live;
            let properties = match pristine
                .as_ref()
                .and_then(|pristine| pristine.own_properties(&holder))
            {
                Some(recorded) => recorded,
                None => {
                    live = self.holder_properties(&holder);
                    &live
                }
            };
            for (key, desc) in properties {
                if key.same(&constructor) || dst.has_own(&out, key) == Some(true) {
                    continue;
                }
                self.bind_member(&out, host, key, desc, depth);
            }
            holder = src.prototype(&holder);
            if self.lookup(&holder).is_some() {
                break;
            }
        }
        out
    }

    fn localized_object(&mut self, value: &Value, depth: u32) -> Value {
        let src = self.src;
        let proto = src.prototype(value);
        let proto = if proto.is_object() {
            self.clone_value(&proto, 0)
        } else {
            proto
        };
        let Some(out) = self.dst.new_object_proto(&proto) else {
            return Value::undefined();
        };
        for (key, desc) in self.holder_properties(value) {
            if desc.accessor {
                self.clone_descriptor_into(&out, &key, &desc, depth);
            } else {
                let item = self.localize(&desc.value, depth + 1);
                self.dst.define(&out, &key, &data_descriptor(item, &desc));
            }
        }
        out
    }

    fn localize(&mut self, value: &Value, depth: u32) -> Value {
        if !value.is_object() {
            return value.clone();
        }
        if let Some(hit) = self.lookup(value) {
            return hit.clone();
        }
        if depth > LOCALIZED_DEPTH {
            return Value::undefined();
        }
        let src = self.src;
        if src.is_function(value) {
            return self.clone_value(value, 0);
        }
        if let Some(buffer) = self.localized_buffer(value) {
            return buffer;
        }
        if value.is_array() {
            return self.localized_array(value, depth);
        }
        if src.try_enter(|scope| scope.is_error(value)) == Some(true) {
            return self.localized_error(value);
        }
        if src.is_plain_object(value) {
            return self.localized_object(value, depth);
        }
        self.bound_shell(value, depth)
    }

    fn adopt_navigator_value(&mut self, mine: &Value, key: &Key, values: &Value) {
        let value = self.src.get_key(mine, key);
        if !value.is_object() {
            return;
        }
        let own = self.clone_instance(&value, 0);
        if let Some(name) = key.name()
            && own.is_object()
        {
            self.dst.set(values, &name.to_string_lossy(), own);
        }
    }

    fn adopt_navigator_objects(&mut self, brand: &Value, frame_nav: &Value) {
        let src = self.src;
        let get = src.get(brand, "navigatorObjects");
        let adopt = src.get(brand, "adoptNavigatorObjects");
        let mine = if src.is_function(&get) {
            src.call(&get, brand, &[]).unwrap_or_else(Value::undefined)
        } else {
            Value::undefined()
        };
        if !mine.is_object() || !src.is_function(&adopt) {
            return;
        }
        let Some(keys) = src.own_keys(&mine, Keys::EnumerableStrings) else {
            return;
        };
        let Some(values) = self.dst.new_object_proto(&Value::null()) else {
            return;
        };
        for key in &keys {
            self.adopt_navigator_value(&mine, key, &values);
        }
        let _ = src.call(&adopt, brand, &[frame_nav.clone(), values]);
    }

    fn install_singleton(&mut self, parent_global: &Value, frame_global: &Value, name: &str) {
        let dst = self.dst;
        let key = Key::new(dst, &std::ffi::CString::new(name).unwrap_or_default());
        let Some(desc) = dst.own_property(frame_global, &key) else {
            return;
        };
        if desc.accessor || !desc.value.is_object() {
            return;
        }
        let parent_value = self.src.get(parent_global, name);
        if !parent_value.is_object() || !parent_value.same_object(&desc.value) {
            return;
        }
        let own = self.clone_instance(&desc.value, 0);
        dst.define(frame_global, &key, &data_descriptor(own, &desc));
    }

    fn register_navigator(&mut self, nav: &Value) {
        let js = Js::of_ctx(self.dst);
        if js.is_null() || !nav.is_object() {
            return;
        }
        let brand = ffi::navigator_brand(js, self.src);
        if !brand.is_object() {
            return;
        }
        let src = self.src;
        let add = src.get(&brand, "add");
        if src.is_function(&add) {
            let _ = src.call(&add, &brand, core::slice::from_ref(nav));
        }
        self.adopt_navigator_objects(&brand, nav);
    }

    fn client_information(&self, frame_global: &Value, nav: &Value) {
        let dst = self.dst;
        let key = Key::new(dst, c"clientInformation");
        if dst.has_own(frame_global, &key) == Some(true) && nav.is_object() {
            dst.define_value(frame_global, &key, nav.clone(), ALL);
        }
    }

    fn install_pristine_singleton(&mut self, frame_global: &Value, name: &str) {
        let dst = self.dst;
        let key = Key::new(dst, &std::ffi::CString::new(name).unwrap_or_default());
        if dst
            .own_property(frame_global, &key)
            .is_some_and(|desc| desc.accessor || !desc.value.is_undefined())
        {
            return;
        }
        let Some(pristine) = self.pristine.clone() else {
            return;
        };
        let source = pristine.global_value(self.src, name);
        if !source.is_object() {
            return;
        }
        let own = self.clone_instance(&source, 0);
        if own.is_object() {
            dst.define_value(frame_global, &key, own, ALL);
        }
    }

    fn install_singletons(&mut self, parent_global: &Value, frame_global: &Value) {
        for name in SINGLETON_NAMES {
            if self.isolated {
                self.install_pristine_singleton(frame_global, name);
            } else {
                self.install_singleton(parent_global, frame_global, name);
            }
        }
        let nav = self.dst.get(frame_global, "navigator");
        self.register_navigator(&nav);
        self.client_information(frame_global, &nav);
    }
}

fn lookup(js: Js, dst: Ctx) -> Option<Shared> {
    if js.is_null() {
        return None;
    }
    page::peek(js, |page| page.cloners.get(&dst).cloned()).flatten()
}

fn cloner_for(js: Js, dst: Ctx) -> Shared {
    cloner_with(js, dst, false)
}

fn cloner_with(js: Js, dst: Ctx, isolated: bool) -> Shared {
    if let Some(found) = lookup(js, dst) {
        return found;
    }
    let main = ffi::main_realm(js);
    let cloner = Rc::new(RefCell::new(Cloner::new(
        main,
        main,
        dst,
        pristine::for_frame(js, isolated),
        isolated,
    )));
    let replaced = page::with(js, |page| {
        page.cloners_made = true;
        page.cloners.insert(dst, cloner.clone())
    });
    drop(replaced);
    cloner
}

pub(crate) fn prepare_frame_realm(js: Js, dst: Ctx, document: &Value, isolated: bool) {
    let cloner = cloner_with(js, dst, isolated);
    if let Ok(mut cloner) = cloner.try_borrow_mut() {
        cloner.clone_class_protos();
        cloner.adopt_in_place(document);
    }
}

pub(crate) fn install_singletons(js: Js, dst: Ctx, parent_global: &Value, frame_global: &Value) {
    let cloner = cloner_for(js, dst);
    if let Ok(mut cloner) = cloner.try_borrow_mut() {
        cloner.install_singletons(parent_global, frame_global);
    }
}

pub(crate) fn localize_into(js: Js, dst: Ctx, value: &Value) -> Value {
    let Some(cloner) = lookup(js, dst) else {
        return Value::undefined();
    };
    match cloner.try_borrow_mut() {
        Ok(mut cloner) => cloner.localize(value, 0),
        Err(_) => Value::undefined(),
    }
}

pub(crate) fn clone_into(js: Js, dst: Ctx, value: &Value) -> Value {
    let Some(cloner) = lookup(js, dst) else {
        return value.clone();
    };
    match cloner.try_borrow_mut() {
        Ok(mut cloner) => cloner.clone_value(value, 0),
        Err(_) => value.clone(),
    }
}

pub(crate) fn isolated_proto_for(js: Js, realm: Ctx, proto: &Value) -> Option<Value> {
    let isolated = lookup(js, realm)?.try_borrow().ok()?.isolated;
    if isolated {
        proto_for(js, realm, proto)
    } else {
        None
    }
}

pub(crate) fn proto_for(js: Js, realm: Ctx, proto: &Value) -> Option<Value> {
    let cloner = lookup(js, realm)?;
    if !proto.is_object() {
        return None;
    }
    let mut cloner = cloner.try_borrow_mut().ok()?;
    if let Some(hit) = cloner.lookup(proto) {
        return Some(hit.clone());
    }
    drop(cloner.clone_value(proto, 0));
    cloner.lookup(proto).cloned()
}

pub(crate) fn clear(js: Js) -> Vec<Shared> {
    page::peek_mut(js, |page| page.cloners.drain().map(|(_, c)| c).collect()).unwrap_or_default()
}
