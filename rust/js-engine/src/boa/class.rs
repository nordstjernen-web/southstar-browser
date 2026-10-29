//! Southstar — host classes on Boa: instances carrying Rust data, per-realm class prototypes, raw-instance finalizers, and the class hooks run as the Boa fork's host exotic methods.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::any::Any;

use boa_engine::builtins::object::OrdinaryObject;
use boa_engine::object::{
    HostExoticHooks, ordinary_define_own_property_of, set_host_exotic_hooks, set_with_receiver,
};
use boa_engine::prelude::{Finalize, JsData, Trace as BoaTrace};
use boa_engine::property::{PropertyDescriptor as BoaPropertyDescriptor, PropertyKey as BoaKey};
use boa_engine::{Context, JsError, JsObject, JsResult, JsString, JsValue};
use boa_gc::custom_trace;

use super::{HostData, Scope, Value};
use crate::{ClassHooks, HostClass, OwnProperty, Trace};

pub(super) struct ClassInstance<T> {
    class: usize,
    data: T,
}

impl<T: Trace> Trace for ClassInstance<T> {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        self.data.trace(visit);
    }
}

struct NoData;

pub(super) struct RawData(core::cell::Cell<*mut core::ffi::c_void>);

#[derive(Default, Finalize, JsData)]
struct ClassPrototypes(Vec<(usize, JsValue)>);

unsafe impl BoaTrace for ClassPrototypes {
    custom_trace!(this, mark, {
        for (_, prototype) in &this.0 {
            mark(prototype);
        }
    });
}

fn class_key(class: &HostClass) -> usize {
    core::ptr::from_ref(class) as usize
}

pub(super) fn has_hooks(hooks: &ClassHooks) -> bool {
    hooks.get_own_property.is_some()
        || hooks.get_own_property_receiver.is_some()
        || hooks.own_keys.is_some()
        || hooks.define_own_property.is_some()
        || hooks.delete.is_some()
        || hooks.set.is_some()
}

pub(super) fn take_raw(data: &dyn Any) -> Option<*mut core::ffi::c_void> {
    let instance = data.downcast_ref::<ClassInstance<RawData>>()?;
    let raw = instance.data.0.replace(core::ptr::null_mut());
    (!raw.is_null()).then_some(raw)
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

    pub fn has_prefix(&self, prefix: &[u8]) -> bool {
        match &self.0 {
            BoaKey::String(text) => text.starts_with(boa_engine::string::JsStr::latin1(prefix)),
            _ => false,
        }
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
    descriptor: BoaPropertyDescriptor,
}

impl PropertyDefinition {
    fn of(descriptor: &BoaPropertyDescriptor) -> PropertyDefinition {
        PropertyDefinition {
            value: Value(descriptor.value().cloned().unwrap_or_default()),
            getter: Value(descriptor.get().cloned().unwrap_or_default()),
            setter: Value(descriptor.set().cloned().unwrap_or_default()),
            has_value: descriptor.value().is_some(),
            has_get: descriptor.get().is_some(),
            has_set: descriptor.set().is_some(),
            descriptor: descriptor.clone(),
        }
    }

    pub fn is_value_only(&self) -> bool {
        self.has_value
            && !self.has_get
            && !self.has_set
            && self.descriptor.writable().is_none()
            && self.descriptor.enumerable().is_none()
            && self.descriptor.configurable().is_none()
    }

    pub fn throws(&self) -> bool {
        false
    }

    pub fn clear_configurable(&mut self) {
        let mut builder = BoaPropertyDescriptor::builder();
        if let Some(value) = self.descriptor.value() {
            builder = builder.value(value.clone());
        }
        if let Some(writable) = self.descriptor.writable() {
            builder = builder.writable(writable);
        }
        if let Some(get) = self.descriptor.get() {
            builder = builder.get(get.clone());
        }
        if let Some(set) = self.descriptor.set() {
            builder = builder.set(set.clone());
        }
        if let Some(enumerable) = self.descriptor.enumerable() {
            builder = builder.enumerable(enumerable);
        }
        self.descriptor = builder.configurable(false).build();
    }

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

fn descriptor_of(property: OwnProperty) -> BoaPropertyDescriptor {
    let builder = if property.accessor {
        BoaPropertyDescriptor::builder()
            .get(property.getter.0)
            .set(property.setter.0)
    } else {
        BoaPropertyDescriptor::builder()
            .value(property.value.0)
            .writable(property.writable)
    };
    builder
        .enumerable(property.enumerable)
        .configurable(property.configurable)
        .build()
}

fn class_of(object: &JsObject) -> Option<&'static HostClass> {
    object.downcast_ref::<HostData>()?.class()
}

fn thrown(error: Value) -> JsError {
    JsError::from_opaque(error.0)
}

struct ExoticHooks;

static EXOTIC_HOOKS: ExoticHooks = ExoticHooks;

impl HostExoticHooks for ExoticHooks {
    fn get_own_property(
        &self,
        object: &JsObject,
        key: &BoaKey,
        receiver: &JsValue,
        context: &mut Context,
    ) -> JsResult<Option<BoaPropertyDescriptor>> {
        let Some(class) = class_of(object) else {
            return Ok(None);
        };
        let this = Value(object.clone().into());
        let key = PropertyKey(key.clone());
        let receiver = Value(receiver.clone());
        let mut scope = Scope::of(context);
        let own = match (
            class.hooks.get_own_property,
            class.hooks.get_own_property_receiver,
        ) {
            (Some(hook), _) if receiver.same_object(&this) => hook(&mut scope, &this, &key),
            (_, Some(hook)) => hook(&mut scope, &this, &key, &receiver),
            (Some(hook), None) => hook(&mut scope, &this, &key),
            (None, None) => return Ok(None),
        };
        own.map(|own| own.map(descriptor_of)).map_err(thrown)
    }

    fn define_own_property(
        &self,
        object: &JsObject,
        key: &BoaKey,
        desc: &BoaPropertyDescriptor,
        context: &mut Context,
    ) -> Option<JsResult<bool>> {
        let hook = class_of(object)?.hooks.define_own_property?;
        let this = Value(object.clone().into());
        let definition = PropertyDefinition::of(desc);
        let result = hook(
            &mut Scope::of(context),
            &this,
            &PropertyKey(key.clone()),
            &definition,
        );
        Some(result.map_err(thrown))
    }

    fn delete(
        &self,
        object: &JsObject,
        key: &BoaKey,
        context: &mut Context,
    ) -> Option<JsResult<bool>> {
        let hook = class_of(object)?.hooks.delete?;
        let this = Value(object.clone().into());
        match hook(&mut Scope::of(context), &this, &PropertyKey(key.clone())) {
            Ok(true) => None,
            result => Some(result.map_err(thrown)),
        }
    }

    fn own_property_keys(&self, object: &JsObject, context: &mut Context) -> JsResult<Vec<BoaKey>> {
        let Some(hook) = class_of(object).and_then(|class| class.hooks.own_keys) else {
            return Ok(Vec::new());
        };
        let this = Value(object.clone().into());
        hook(&mut Scope::of(context), &this)
            .map(|keys| keys.into_iter().map(|key| key.0).collect())
            .map_err(thrown)
    }

    fn set(
        &self,
        object: &JsObject,
        key: &BoaKey,
        value: &JsValue,
        receiver: &JsValue,
        context: &mut Context,
    ) -> Option<JsResult<bool>> {
        let hook = class_of(object)?.hooks.set?;
        let this = Value(object.clone().into());
        let result = hook(
            &mut Scope::of(context),
            &this,
            &PropertyKey(key.clone()),
            &Value(value.clone()),
            &Value(receiver.clone()),
            SetFlags,
        );
        Some(result.map_err(thrown))
    }
}

impl Scope<'_> {
    pub fn register_class(&mut self, class: &'static HostClass) {
        set_host_exotic_hooks(&EXOTIC_HOOKS);
        super::reflect::remember_class(class);
    }

    pub fn set_class_prototype(&mut self, class: &HostClass, prototype: Value) {
        let key = class_key(class);
        let realm = self.cx().realm().clone();
        let mut host = realm.host_defined_mut();
        if host.get::<ClassPrototypes>().is_none() {
            host.insert(ClassPrototypes::default());
        }
        if let Some(prototypes) = host.get_mut::<ClassPrototypes>() {
            prototypes.0.retain(|(known, _)| *known != key);
            prototypes.0.push((key, prototype.0));
        }
    }

    pub fn class_prototype(&mut self, class: &HostClass) -> Value {
        let key = class_key(class);
        let realm = self.cx().realm().clone();
        let host = realm.host_defined();
        let prototype = host.get::<ClassPrototypes>().and_then(|prototypes| {
            prototypes
                .0
                .iter()
                .find(|(known, _)| *known == key)
                .map(|(_, prototype)| prototype.clone())
        });
        prototype.map_or_else(Value::undefined, Value)
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

    fn class_instance<T: Any>(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: T,
        trace: Option<super::HostTrace>,
    ) -> Value {
        let prototype = self.instance_prototype(class, prototype);
        let instance = ClassInstance {
            class: class_key(class),
            data,
        };
        self.host_object(
            prototype.as_ref(),
            instance,
            trace,
            core::ptr::from_ref(class),
        )
    }

    pub unsafe fn new_raw_instance(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: *mut core::ffi::c_void,
    ) -> Result<Value, Value> {
        let raw = RawData(core::cell::Cell::new(data));
        Ok(self.class_instance(class, prototype, raw, None))
    }

    pub fn new_instance(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
    ) -> Result<Value, Value> {
        Ok(self.class_instance(class, prototype, NoData, None))
    }

    pub fn new_instance_with<T: Any>(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: T,
    ) -> Result<Value, Value> {
        Ok(self.class_instance(class, prototype, data, None))
    }

    pub fn new_traced_instance<T: Any + Trace>(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: T,
    ) -> Result<Value, Value> {
        let trace = Some(super::trace_as::<ClassInstance<T>> as super::HostTrace);
        Ok(self.class_instance(class, prototype, data, trace))
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
        super::access::is_host_access(self.cx())
    }

    pub fn is_host_caller(&mut self) -> bool {
        super::access::is_host_caller(self.cx())
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
        receiver: &Value,
        _flags: SetFlags,
    ) -> Result<bool, Value> {
        let target = self.object(target)?;
        set_with_receiver(
            &target,
            key.0.clone(),
            value.0.clone(),
            receiver.0.clone(),
            self.cx(),
        )
        .map_err(|error| self.error(error))
    }

    pub fn define_ordinary(
        &mut self,
        object: &Value,
        key: &PropertyKey,
        definition: &PropertyDefinition,
    ) -> Result<bool, Value> {
        let object = self.object(object)?;
        let descriptor = definition.descriptor.clone();
        ordinary_define_own_property_of(&object, &key.0, descriptor, self.cx())
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
        self.0
            .as_object()
            .map_or(core::ptr::null_mut(), |object| object.identity())
    }

    pub fn is_instance_of_class(&self, class: &HostClass) -> bool {
        self.0.as_object().is_some_and(|object| {
            object
                .downcast_ref::<HostData>()
                .is_some_and(|host| core::ptr::addr_eq(host.class, core::ptr::from_ref(class)))
        })
    }
}
