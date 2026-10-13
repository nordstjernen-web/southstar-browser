//! Southstar — the object-model calls that copy one realm's objects into another on Boa: every own key, full property definitions, native-function clones, forwarders and the plain-object test.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use boa_engine::native_function::NativeFunctionObject;
use boa_engine::object::FunctionObjectBuilder;
use boa_engine::property::PropertyDescriptor as BoaPropertyDescriptor;
use boa_engine::{JsObject, JsString, JsValue, NativeFunction};

use std::sync::Mutex;

use super::{Scope, Value};
use crate::{HostClass, PropertyDescriptor};

static CLASSES: Mutex<Vec<&'static HostClass>> = Mutex::new(Vec::new());

pub(super) fn remember_class(class: &'static HostClass) {
    if let Ok(mut classes) = CLASSES.lock()
        && !classes.iter().any(|known| core::ptr::eq(*known, class))
    {
        classes.push(class);
    }
}

impl Scope<'_> {
    pub fn own_property_keys_with_private(&mut self, object: &Value) -> Result<Vec<Value>, Value> {
        self.own_property_keys(object, true)
    }

    pub(super) fn descriptor_flag(
        &mut self,
        descriptor: &Value,
        name: &str,
    ) -> Result<bool, Value> {
        let flag = self.get(descriptor, name)?;
        Ok(flag.0.to_boolean())
    }

    pub fn define_property(
        &mut self,
        object: &Value,
        key: &Value,
        descriptor: &PropertyDescriptor,
    ) -> Result<(), Value> {
        let target = self.object(object)?;
        let key = key
            .0
            .to_property_key(self.cx())
            .map_err(|e| self.error(e))?;
        let builder = BoaPropertyDescriptor::builder()
            .configurable(descriptor.configurable)
            .enumerable(descriptor.enumerable);
        let builder = if descriptor.accessor {
            builder
                .get(descriptor.getter.0.clone())
                .set(descriptor.setter.0.clone())
        } else {
            builder
                .value(descriptor.value.0.clone())
                .writable(descriptor.writable)
        };
        target
            .define_property_or_throw(key, builder, self.cx())
            .map(|_| ())
            .map_err(|e| self.error(e))
    }

    pub fn clone_native_function(&mut self, function: &Value) -> Result<Value, Value> {
        let Some(object) = function.0.as_object() else {
            return Ok(Value(JsValue::undefined()));
        };
        let Some((native, name, constructor)) =
            object.downcast_ref::<NativeFunctionObject>().map(|data| {
                (
                    data.function().clone(),
                    data.name().clone(),
                    data.is_constructor(),
                )
            })
        else {
            return Ok(Value(JsValue::undefined()));
        };
        let length = object
            .get(JsString::from("length"), self.cx())
            .map_err(|e| self.error(e))?
            .to_length(self.cx())
            .map_err(|e| self.error(e))?;
        let realm = self.cx().realm().clone();
        let clone = FunctionObjectBuilder::new(&realm, native)
            .name(name)
            .length(length as usize)
            .constructor(constructor)
            .build();
        Ok(Value(clone.into()))
    }

    pub fn new_forwarder(
        &mut self,
        target: &Value,
        name: &str,
        length: i32,
        constructor: bool,
    ) -> Result<Value, Value> {
        let target = self.object(target)?;
        let native = NativeFunction::from_copy_closure_with_captures(
            move |this, args, target: &JsObject, ctx| {
                if constructor
                    && target.is_constructor()
                    && let Some(new_target) = this.as_constructor()
                {
                    return target
                        .construct(args, Some(&new_target), ctx)
                        .map(JsValue::from);
                }
                let this = if this.is_undefined() || this.is_null() {
                    JsValue::from(ctx.global_object())
                } else {
                    this.clone()
                };
                target.call(&this, args, ctx)
            },
            target,
        );
        let realm = self.cx().realm().clone();
        let function = FunctionObjectBuilder::new(&realm, native)
            .name(JsString::from(name.split('\0').next().unwrap_or_default()))
            .length(length.max(0) as usize)
            .constructor(constructor)
            .build();
        Ok(Value(function.into()))
    }

    pub fn registered_classes(&self) -> Vec<&'static HostClass> {
        CLASSES
            .lock()
            .map(|classes| classes.clone())
            .unwrap_or_default()
    }

    pub fn is_plain_object(&self, value: &Value) -> bool {
        value
            .0
            .as_object()
            .is_some_and(|object| object.is_ordinary())
    }
}
