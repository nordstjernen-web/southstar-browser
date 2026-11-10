//! Southstar — the object-model calls that copy one realm's objects into another on Boa: every own key, private elements, full property definitions, native-function clones, forwarders and the plain-object test.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use boa_engine::native_function::NativeFunctionObject;
use boa_engine::object::FunctionObjectBuilder;
use boa_engine::object::builtins::{JsFunction, JsPromise};
use boa_engine::property::PropertyDescriptor as BoaPropertyDescriptor;
use boa_engine::{Context, JsError, JsObject, JsResult, JsString, JsValue, NativeFunction};

use std::sync::Mutex;

use super::{Scope, Value};
use crate::{HostClass, PropertyDescriptor};

static CLASSES: Mutex<Vec<&'static HostClass>> = Mutex::new(Vec::new());

const REALM_CHAIN_LIMIT: usize = 64;

fn belongs_to_realm(value: &JsValue, ctx: &Context) -> bool {
    let Some(mut object) = value.as_object() else {
        return true;
    };
    let root = ctx.intrinsics().constructors().object().prototype();
    for _ in 0..REALM_CHAIN_LIMIT {
        if JsObject::equals(&object, &root) {
            return true;
        }
        match object.prototype() {
            Some(parent) => object = parent,
            None => return false,
        }
    }
    false
}

fn into_realm(value: JsValue, localize: &JsObject, ctx: &mut Context) -> JsValue {
    if belongs_to_realm(&value, ctx) {
        return value;
    }
    let foreign = value
        .as_object()
        .and_then(|object| JsPromise::from_object(object).ok());
    let Some(foreign) = foreign else {
        let copied = localize
            .call(&JsValue::undefined(), &[value], ctx)
            .unwrap_or_default();
        return if belongs_to_realm(&copied, ctx) {
            copied
        } else {
            JsValue::undefined()
        };
    };
    let (promise, resolvers) = JsPromise::new_pending(ctx);
    let fulfilled = settling(resolvers.resolve, localize.clone(), ctx);
    let rejected = settling(resolvers.reject, localize.clone(), ctx);
    match foreign.then(Some(fulfilled), Some(rejected), ctx) {
        Ok(_) => promise.into(),
        Err(_) => JsValue::undefined(),
    }
}

fn settling(settle: JsFunction, localize: JsObject, ctx: &Context) -> JsFunction {
    NativeFunction::from_copy_closure_with_captures(
        |_this, args, (settle, localize): &(JsFunction, JsObject), ctx| {
            let value = args.first().cloned().unwrap_or_default();
            let value = into_realm(value, localize, ctx);
            settle.call(&JsValue::undefined(), &[value], ctx)
        },
        (settle, localize),
    )
    .to_js_function(ctx.realm())
}

fn forward(
    target: &JsObject,
    this: &JsValue,
    args: &[JsValue],
    localize: Option<&JsObject>,
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let Some(localize) = localize else {
        return target.call(this, args, ctx);
    };
    match target.call(this, args, ctx) {
        Ok(result) => Ok(into_realm(result, localize, ctx)),
        Err(error) => match error.into_opaque(ctx) {
            Ok(thrown) => Err(JsError::from_opaque(into_realm(thrown, localize, ctx))),
            Err(limit) => Err(limit),
        },
    }
}

pub(super) fn remember_class(class: &'static HostClass) {
    if let Ok(mut classes) = CLASSES.lock()
        && !classes.iter().any(|known| core::ptr::eq(*known, class))
    {
        classes.push(class);
    }
}

impl Scope<'_> {
    pub fn copy_private_elements(&mut self, from: &Value, to: &Value) -> Result<(), Value> {
        let source = self.object(from)?;
        let target = self.object(to)?;
        source.copy_private_elements_to(&target);
        Ok(())
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
        let Some((native, name, constructor, host)) =
            object.downcast_ref::<NativeFunctionObject>().map(|data| {
                (
                    data.function().clone(),
                    data.name().clone(),
                    data.is_constructor(),
                    data.is_host(),
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
            .host(host)
            .build();
        Ok(Value(clone.into()))
    }

    pub fn new_forwarder(
        &mut self,
        target: &Value,
        name: &str,
        length: i32,
        constructor: bool,
        localize: Option<&Value>,
    ) -> Result<Value, Value> {
        let target = self.object(target)?;
        let localize = match localize {
            Some(localize) => Some(self.object(localize)?),
            None => None,
        };
        let native = NativeFunction::from_copy_closure_with_captures(
            move |this, args, (target, localize): &(JsObject, Option<JsObject>), ctx| {
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
                forward(target, &this, args, localize.as_ref(), ctx)
            },
            (target, localize),
        );
        let realm = self.cx().realm().clone();
        let function = FunctionObjectBuilder::new(&realm, native)
            .name(JsString::from(name.split('\0').next().unwrap_or_default()))
            .length(length.max(0) as usize)
            .constructor(constructor)
            .host(true)
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
