//! Southstar — a frame realm handle and the object-model calls the realm cloner makes through it: property keys, full descriptors, class prototypes, native-function clones and forwarders.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_js_engine::{Attributes, ContextHandle, HostClass, PropertyDescriptor, Scope, Value};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Ctx(ContextHandle);

pub(crate) struct Key {
    value: Value,
    text: Option<String>,
}

impl Key {
    pub fn new(ctx: Ctx, name: &CStr) -> Key {
        let text = name.to_string_lossy().into_owned();
        Key {
            value: ctx
                .try_enter(|scope| scope.string(&text))
                .unwrap_or_else(Value::undefined),
            text: Some(text),
        }
    }

    fn of(ctx: Ctx, value: Value) -> Key {
        let text = if value.is_string() {
            ctx.to_string(&value)
        } else {
            None
        };
        Key { value, text }
    }

    pub fn same(&self, other: &Key) -> bool {
        self.text.is_some() && self.text == other.text
    }

    pub fn name(&self) -> Option<CString> {
        self.text.as_deref().map(|text| {
            CString::new(text.split('\0').next().unwrap_or_default()).unwrap_or_default()
        })
    }
}

pub(crate) enum PropertyKey {
    Index(u32),
    Name(CString),
}

pub(crate) type Descriptor = PropertyDescriptor;

pub(crate) enum Keys {
    Strings,
    StringsAndSymbols,
    EnumerableStrings,
}

fn trimmed(text: &str) -> &str {
    text.split('\0').next().unwrap_or_default()
}

fn array_index(text: &str) -> Option<u32> {
    let index: u32 = text.parse().ok()?;
    (index != u32::MAX && index.to_string() == text).then_some(index)
}

impl Ctx {
    pub const NULL: Ctx = Ctx(ContextHandle::NULL);

    pub fn from_handle(handle: ContextHandle) -> Option<Ctx> {
        handle.non_null().map(Ctx)
    }

    pub fn handle(self) -> ContextHandle {
        self.0
    }

    pub fn page(self) -> Option<southstar_js_runtime::api::Page> {
        southstar_js_runtime::api::Page::of_context(self.0)
    }

    pub fn enter<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        self.0.with(f).expect("a live realm")
    }

    pub fn try_enter<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        self.0.with(f)
    }

    pub fn global(self) -> Value {
        self.try_enter(|scope| scope.global())
            .unwrap_or_else(Value::undefined)
    }

    pub fn get(self, object: &Value, name: &str) -> Value {
        self.try_enter(|scope| scope.get(object, name).ok())
            .flatten()
            .unwrap_or_else(Value::undefined)
    }

    pub fn get_key(self, object: &Value, key: &Key) -> Value {
        self.try_enter(|scope| scope.get_key(object, &key.value).ok())
            .flatten()
            .unwrap_or_else(Value::undefined)
    }

    pub fn set(self, object: &Value, name: &str, value: Value) {
        self.try_enter(|scope| scope.set(object, name, value).ok());
    }

    pub fn delete(self, object: &Value, name: &CStr) -> c_int {
        let name = name.to_string_lossy();
        match self.try_enter(|scope| scope.delete(object, &name)) {
            Some(Ok(true)) => 1,
            Some(Ok(false)) => 0,
            _ => -1,
        }
    }

    pub fn own_keys(self, object: &Value, keys: Keys) -> Option<Vec<Key>> {
        let values = self
            .try_enter(|scope| match keys {
                Keys::Strings => scope.own_property_keys(object, false),
                Keys::StringsAndSymbols => scope.own_property_keys(object, true),
                Keys::EnumerableStrings => scope.own_enumerable_keys(object),
            })?
            .ok()?;
        Some(
            values
                .into_iter()
                .map(|value| Key::of(self, value))
                .collect(),
        )
    }

    pub fn own_property(self, object: &Value, key: &Key) -> Option<Descriptor> {
        self.try_enter(|scope| scope.own_property(object, &key.value).ok())
            .flatten()
            .flatten()
    }

    pub fn has_own(self, object: &Value, key: &Key) -> Option<bool> {
        self.try_enter(|scope| scope.own_property(object, &key.value).ok())
            .flatten()
            .map(|found| found.is_some())
    }

    pub fn copy_private_elements(self, from: &Value, to: &Value) {
        self.try_enter(|scope| scope.copy_private_elements(from, to).ok());
    }

    pub fn define(self, object: &Value, key: &Key, descriptor: &Descriptor) {
        self.try_enter(|scope| scope.define_property(object, &key.value, descriptor).ok());
    }

    pub fn define_value(self, object: &Value, key: &Key, value: Value, attributes: Attributes) {
        self.try_enter(|scope| {
            scope
                .define_with_key(object, &key.value, value, attributes)
                .ok()
        });
    }

    pub fn define_value_str(
        self,
        object: &Value,
        name: &str,
        value: Value,
        attributes: Attributes,
    ) {
        let key = Key::new(self, &CString::new(trimmed(name)).unwrap_or_default());
        self.define_value(object, &key, value, attributes);
    }

    pub fn prototype(self, value: &Value) -> Value {
        self.try_enter(|scope| scope.get_prototype(value).ok())
            .flatten()
            .unwrap_or_else(Value::undefined)
    }

    pub fn set_prototype(self, object: &Value, prototype: &Value) {
        self.try_enter(|scope| scope.set_prototype(object, prototype).ok());
    }

    pub fn is_function(self, value: &Value) -> bool {
        self.try_enter(|scope| scope.is_function(value))
            .unwrap_or(false)
    }

    pub fn is_constructor(self, value: &Value) -> bool {
        self.try_enter(|scope| scope.is_constructor(value))
            .unwrap_or(false)
    }

    pub fn is_plain_object(self, value: &Value) -> bool {
        self.try_enter(|scope| scope.is_plain_object(value))
            .unwrap_or(false)
    }

    pub fn to_bool(self, value: &Value) -> bool {
        self.try_enter(|scope| scope.to_bool(value))
            .unwrap_or(false)
    }

    pub fn to_int32(self, value: &Value) -> i32 {
        self.try_enter(|scope| scope.to_int32(value).ok())
            .flatten()
            .unwrap_or(0)
    }

    pub fn to_string(self, value: &Value) -> Option<String> {
        self.try_enter(|scope| scope.to_string(value).ok())
            .flatten()
    }

    pub fn clone_native_function(self, function: &Value) -> Option<Value> {
        self.try_enter(|scope| scope.clone_native_function(function).ok())
            .flatten()
    }

    pub fn new_forwarder(
        self,
        target: &Value,
        name: &str,
        length: i32,
        constructor: bool,
    ) -> Option<Value> {
        self.try_enter(|scope| scope.new_forwarder(target, name, length, constructor).ok())
            .flatten()
    }

    pub fn registered_classes(self) -> Vec<&'static HostClass> {
        self.try_enter(|scope| scope.registered_classes())
            .unwrap_or_default()
    }

    pub fn class_proto(self, class: &HostClass) -> Value {
        self.try_enter(|scope| scope.class_prototype(class))
            .unwrap_or_else(Value::undefined)
    }

    pub fn set_class_proto(self, class: &HostClass, prototype: Value) {
        self.try_enter(|scope| scope.set_class_prototype(class, prototype));
    }

    pub fn new_object_proto(self, prototype: &Value) -> Option<Value> {
        self.try_enter(|scope| scope.new_object_with_proto(prototype))
    }

    pub fn new_object(self) -> Value {
        self.try_enter(|scope| scope.new_object())
            .unwrap_or_else(Value::undefined)
    }

    pub fn eval_hidden(self, source: &str, name: &CStr) -> Option<Value> {
        let name = name.to_string_lossy();
        self.try_enter(|scope| scope.eval_native_script(source, &name).ok())
            .flatten()
    }

    pub fn call(self, function: &Value, this: &Value, args: &[Value]) -> Option<Value> {
        self.try_enter(|scope| scope.call(function, this, args).ok())
            .flatten()
    }
}

pub(crate) fn property_key(scope: &mut Scope<'_>, key: &Value) -> Result<PropertyKey, Value> {
    let text = if key.is_symbol() {
        let description = scope.get(key, "description")?;
        if description.is_string() {
            scope.to_string(&description)?
        } else {
            String::new()
        }
    } else {
        scope.to_string(key)?
    };
    if let Some(index) = array_index(&text) {
        return Ok(PropertyKey::Index(index));
    }
    Ok(PropertyKey::Name(
        CString::new(trimmed(&text)).unwrap_or_default(),
    ))
}
