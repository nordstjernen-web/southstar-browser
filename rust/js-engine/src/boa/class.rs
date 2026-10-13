//! Southstar — host classes on Boa: instances carrying Rust data and per-class prototypes; Boa 0.22 has no public custom internal methods, so a class's exotic hooks do not run.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;

use boa_engine::builtins::object::OrdinaryObject;
use boa_engine::property::{PropertyDescriptor as BoaPropertyDescriptor, PropertyKey as BoaKey};
use boa_engine::{JsString, JsValue};

use super::{Scope, Value};
use crate::{HostClass, OwnProperty, Trace};

thread_local! {
    static PROTOTYPES: RefCell<HashMap<usize, Value>> = RefCell::new(HashMap::new());
}

struct ClassInstance<T> {
    class: usize,
    data: T,
}

impl<T: Trace> Trace for ClassInstance<T> {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        self.data.trace(visit);
    }
}

struct NoData;

struct RawData(core::cell::Cell<*mut core::ffi::c_void>);

fn class_key(class: &HostClass) -> usize {
    core::ptr::from_ref(class) as usize
}

#[derive(Clone)]
pub struct PropertyKey(BoaKey);

impl PropertyKey {
    pub fn is_symbol(&self, _scope: &mut Scope<'_>) -> bool {
        matches!(self.0, BoaKey::Symbol(_))
    }

    pub fn to_value(&self, _scope: &mut Scope<'_>) -> Value {
        Value(match &self.0 {
            BoaKey::String(text) => JsValue::from(text.clone()),
            BoaKey::Symbol(symbol) => JsValue::from(symbol.clone()),
            BoaKey::Index(index) => JsValue::from(JsString::from(index.get().to_string())),
        })
    }

    pub fn array_index(&self, _scope: &mut Scope<'_>) -> Option<u32> {
        match &self.0 {
            BoaKey::Index(index) => Some(index.get()),
            _ => None,
        }
    }

    pub fn to_c_bytes(&self, _scope: &mut Scope<'_>) -> Option<Vec<u8>> {
        let text = match &self.0 {
            BoaKey::String(text) => text.to_std_string_escaped(),
            BoaKey::Symbol(symbol) => symbol.descriptive_string().to_std_string_escaped(),
            BoaKey::Index(index) => index.get().to_string(),
        };
        let bytes = text.into_bytes();
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        Some(bytes[..end].to_vec())
    }
}

#[derive(Clone, Copy)]
pub struct SetFlags;

#[derive(Clone)]
pub struct PropertyDefinition {
    pub value: Value,
    pub getter: Value,
    pub setter: Value,
    has_value: bool,
    has_get: bool,
    has_set: bool,
}

impl PropertyDefinition {
    pub fn throws(&self) -> bool {
        false
    }

    pub fn clear_configurable(&mut self) {}

    pub fn has_get(&self) -> bool {
        self.has_get
    }

    pub fn has_set(&self) -> bool {
        self.has_set
    }

    pub fn has_value(&self) -> bool {
        self.has_value
    }
}

impl Scope<'_> {
    pub fn register_class(&mut self, _class: &'static HostClass) {}

    pub fn set_class_prototype(&mut self, class: &HostClass, prototype: Value) {
        PROTOTYPES.with(|map| map.borrow_mut().insert(class_key(class), prototype));
    }

    pub fn class_prototype(&mut self, class: &HostClass) -> Value {
        PROTOTYPES
            .with(|map| map.borrow().get(&class_key(class)).cloned())
            .unwrap_or_else(Value::undefined)
    }

    fn instance_prototype(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
    ) -> Option<Value> {
        match prototype {
            Some(prototype) => Some(prototype.clone()),
            None => Some(self.class_prototype(class)).filter(Value::is_object),
        }
    }

    pub unsafe fn new_raw_instance(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: *mut core::ffi::c_void,
    ) -> Result<Value, Value> {
        self.new_instance_with(class, prototype, RawData(core::cell::Cell::new(data)))
    }

    pub fn new_instance(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
    ) -> Result<Value, Value> {
        self.new_instance_with(class, prototype, NoData)
    }

    pub fn new_instance_with<T: Any>(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: T,
    ) -> Result<Value, Value> {
        let prototype = self.instance_prototype(class, prototype);
        let instance = ClassInstance {
            class: class_key(class),
            data,
        };
        Ok(self.new_host_object(prototype.as_ref(), instance))
    }

    pub fn new_traced_instance<T: Any + Trace>(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: T,
    ) -> Result<Value, Value> {
        let prototype = self.instance_prototype(class, prototype);
        let instance = ClassInstance {
            class: class_key(class),
            data,
        };
        Ok(self.new_traced_host_object(prototype.as_ref(), instance))
    }

    pub fn property_key(&mut self, name: &[u8]) -> PropertyKey {
        PropertyKey(BoaKey::from(JsString::from(
            String::from_utf8_lossy(name).as_ref(),
        )))
    }

    pub fn index_key(&mut self, index: u32) -> PropertyKey {
        PropertyKey(BoaKey::from(index))
    }

    pub fn is_host_access(&mut self) -> bool {
        false
    }

    pub fn has_key(&mut self, object: &Value, key: &PropertyKey) -> Result<bool, Value> {
        let object = self.object(object)?;
        object
            .has_property(key.0.clone(), self.cx())
            .map_err(|error| self.error(error))
    }

    pub fn own_property_of(
        &mut self,
        object: &Value,
        key: &PropertyKey,
    ) -> Result<Option<OwnProperty>, Value> {
        let key = key.to_value(self);
        let descriptor = OrdinaryObject::get_own_property_descriptor(
            &JsValue::undefined(),
            &[object.0.clone(), key.0],
            self.cx(),
        )
        .map_err(|error| self.error(error))?;
        if descriptor.is_undefined() {
            return Ok(None);
        }
        let descriptor = Value(descriptor);
        let getter = self.get(&descriptor, "get")?;
        let setter = self.get(&descriptor, "set")?;
        let writable = self.get(&descriptor, "writable")?;
        let enumerable = self.get(&descriptor, "enumerable")?;
        let configurable = self.get(&descriptor, "configurable")?;
        Ok(Some(OwnProperty {
            value: self.get(&descriptor, "value")?,
            accessor: !getter.0.is_undefined() || !setter.0.is_undefined(),
            getter,
            setter,
            writable: self.to_bool(&writable),
            enumerable: self.to_bool(&enumerable),
            configurable: self.to_bool(&configurable),
        }))
    }

    pub fn own_keys_of(&mut self, object: &Value) -> Result<Vec<PropertyKey>, Value> {
        let object = self.object(object)?;
        let keys = object
            .own_property_keys(self.cx())
            .map_err(|error| self.error(error))?;
        Ok(keys.into_iter().map(PropertyKey).collect())
    }

    pub fn delete_key_of(&mut self, object: &Value, key: &PropertyKey) -> Result<bool, Value> {
        let object = self.object(object)?;
        object
            .delete_property_or_throw(key.0.clone(), self.cx())
            .map_err(|error| self.error(error))
    }

    pub fn define_forward(
        &mut self,
        target: &Value,
        key: &PropertyKey,
        definition: &PropertyDefinition,
    ) -> Result<bool, Value> {
        self.define_ordinary(target, key, definition)
    }

    pub fn define_value_key(
        &mut self,
        object: &Value,
        key: &PropertyKey,
        value: &Value,
    ) -> Result<bool, Value> {
        let object = self.object(object)?;
        let descriptor = BoaPropertyDescriptor::builder()
            .value(value.0.clone())
            .writable(true)
            .enumerable(true)
            .configurable(true)
            .build();
        object
            .define_property_or_throw(key.0.clone(), descriptor, self.cx())
            .map_err(|error| self.error(error))
    }

    pub fn set_with_receiver(
        &mut self,
        target: &Value,
        key: &PropertyKey,
        value: &Value,
        _receiver: &Value,
        _flags: SetFlags,
    ) -> Result<bool, Value> {
        let target = self.object(target)?;
        target
            .set(key.0.clone(), value.0.clone(), false, self.cx())
            .map_err(|error| self.error(error))
    }

    pub fn define_ordinary(
        &mut self,
        object: &Value,
        key: &PropertyKey,
        definition: &PropertyDefinition,
    ) -> Result<bool, Value> {
        let object = self.object(object)?;
        let mut descriptor = BoaPropertyDescriptor::builder();
        if definition.has_value {
            descriptor = descriptor.value(definition.value.0.clone());
        }
        if definition.has_get {
            descriptor = descriptor.maybe_get(definition.getter.0.as_object());
        }
        if definition.has_set {
            descriptor = descriptor.maybe_set(definition.setter.0.as_object());
        }
        object
            .define_property_or_throw(key.0.clone(), descriptor.build(), self.cx())
            .map_err(|error| self.error(error))
    }
}

impl Value {
    pub fn with_instance<T: Any, R>(
        &self,
        class: &HostClass,
        f: impl FnOnce(&T) -> R,
    ) -> Option<R> {
        let key = class_key(class);
        self.with_host::<ClassInstance<T>, _>(|instance| {
            (instance.class == key).then(|| f(&instance.data))
        })
        .flatten()
    }

    pub fn raw_instance(&self, class: &HostClass) -> *mut core::ffi::c_void {
        self.with_instance(class, |raw: &RawData| raw.0.get())
            .unwrap_or(core::ptr::null_mut())
    }

    pub unsafe fn set_raw_instance(&self, data: *mut core::ffi::c_void) {
        self.with_host::<ClassInstance<RawData>, _>(|instance| instance.data.0.set(data));
    }

    pub fn object_identity(&self) -> *mut core::ffi::c_void {
        self.0.as_object().map_or(core::ptr::null_mut(), |object| {
            core::ptr::from_ref(object.as_ref()).cast_mut().cast()
        })
    }

    pub fn is_instance_of_class(&self, class: &HostClass) -> bool {
        let key = class_key(class);
        self.with_host::<ClassInstance<NoData>, _>(|instance| instance.class == key)
            .unwrap_or(false)
    }
}
