//! Southstar — the Boa backend of the JavaScript layer, over the boa_engine crate.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::any::Any;
use std::path::Path;
use std::rc::Rc;

use boa_engine::builtins::object::OrdinaryObject;
use boa_engine::builtins::promise::PromiseState as BoaPromiseState;
use boa_engine::builtins::typed_array::TypedArrayKind;
use boa_engine::module::SimpleModuleLoader;
use boa_engine::object::FunctionObjectBuilder;
use boa_engine::object::builtins::{JsArray, JsArrayBuffer, JsPromise, JsTypedArray};
use boa_engine::prelude::{Finalize, JsData, Trace};
use boa_engine::property::{PropertyDescriptor as BoaPropertyDescriptor, PropertyKey};
use boa_engine::{
    Context, JsBigInt, JsError, JsNativeError, JsObject, JsString, JsSymbol, JsValue, Module,
    NativeFunction, Source,
};

use crate::{
    Attributes, BoundFn, NativeFn, PromiseState, PropertyDescriptor, RealmInit, TypedArrayBytes,
    int64_modulo,
};

pub const ENGINE_NAME: &str = "boa";

pub fn engine_version() -> String {
    "boa_engine 0.22".to_owned()
}

#[derive(Clone)]
pub struct Value(JsValue);

#[derive(Trace, Finalize, JsData)]
#[boa_gc(unsafe_empty_trace)]
struct HostData(Box<dyn Any>);

impl Value {
    pub fn undefined() -> Value {
        Value(JsValue::undefined())
    }

    pub fn null() -> Value {
        Value(JsValue::null())
    }

    pub fn int(number: i32) -> Value {
        Value(JsValue::from(number))
    }

    pub fn number(number: f64) -> Value {
        Value(JsValue::from(number))
    }

    pub fn int64(number: i64) -> Value {
        match i32::try_from(number) {
            Ok(small) => Value::int(small),
            Err(_) => Value::number(number as f64),
        }
    }

    pub fn boolean(value: bool) -> Value {
        Value(JsValue::from(value))
    }

    pub fn is_undefined(&self) -> bool {
        self.0.is_undefined()
    }

    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    pub fn is_object(&self) -> bool {
        self.0.is_object()
    }

    pub fn is_string(&self) -> bool {
        self.0.is_string()
    }

    pub fn is_array(&self) -> bool {
        self.0.as_object().is_some_and(|object| object.is_array())
    }

    pub fn is_number(&self) -> bool {
        self.0.is_number()
    }

    pub fn is_bool(&self) -> bool {
        self.0.is_boolean()
    }

    pub fn same_object(&self, other: &Value) -> bool {
        match (self.0.as_object(), other.0.as_object()) {
            (Some(a), Some(b)) => JsObject::equals(&a, &b),
            _ => false,
        }
    }
}

pub struct Engine {
    ctx: Context,
}

impl Engine {
    pub fn new(module_root: &Path) -> Engine {
        let ctx = SimpleModuleLoader::new(module_root)
            .ok()
            .and_then(|loader| {
                Context::builder()
                    .module_loader(Rc::new(loader))
                    .build()
                    .ok()
            })
            .unwrap_or_default();
        Engine { ctx }
    }

    pub fn set_max_stack_size(&mut self, _bytes: usize) {}

    pub fn enter<R>(&mut self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        f(&mut Scope { ctx: &mut self.ctx })
    }
}

pub struct Realm(boa_engine::realm::Realm);

pub struct Scope<'a> {
    ctx: &'a mut Context,
}

impl Scope<'_> {
    fn error(&mut self, error: JsError) -> Value {
        match error.into_opaque(self.ctx) {
            Ok(value) => Value(value),
            Err(uncatchable) => Value(JsString::from(uncatchable.to_string().as_str()).into()),
        }
    }

    fn object(&mut self, value: &Value) -> Result<JsObject, Value> {
        value.0.to_object(self.ctx).map_err(|e| self.error(e))
    }

    pub fn eval_script(&mut self, source: &str, name: &str) -> Result<Value, Value> {
        let path = Path::new(name);
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let source = Source::from_bytes(source).with_path(&path);
        self.ctx.eval(source).map(Value).map_err(|e| self.error(e))
    }

    pub fn eval_module(&mut self, source: &str, path: &Path) -> Result<Value, Value> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let source = Source::from_bytes(source).with_path(&path);
        let module = Module::parse(source, None, self.ctx).map_err(|e| self.error(e))?;
        let promise = module.load_link_evaluate(self.ctx);
        Ok(Value(promise.into()))
    }

    pub fn run_jobs(&mut self) -> Result<(), Value> {
        self.ctx.run_jobs().map_err(|e| self.error(e))
    }

    pub fn promise_state(&mut self, value: &Value) -> PromiseState {
        let Some(promise) = value
            .0
            .as_object()
            .and_then(|object| JsPromise::from_object(object.clone()).ok())
        else {
            return PromiseState::NotAPromise;
        };
        match promise.state() {
            BoaPromiseState::Pending => PromiseState::Pending,
            BoaPromiseState::Fulfilled(value) => PromiseState::Fulfilled(Value(value)),
            BoaPromiseState::Rejected(value) => PromiseState::Rejected(Value(value)),
        }
    }

    pub fn global(&mut self) -> Value {
        Value(self.ctx.global_object().into())
    }

    pub fn new_object(&mut self) -> Value {
        Value(JsObject::with_object_proto(self.ctx.intrinsics()).into())
    }

    pub fn string(&mut self, text: &str) -> Value {
        Value(JsString::from(text).into())
    }

    pub fn new_array(&mut self) -> Value {
        match JsArray::new(self.ctx) {
            Ok(array) => Value(array.into()),
            Err(error) => self.error(error),
        }
    }

    pub fn new_object_with_proto(&mut self, prototype: &Value) -> Value {
        let object = JsObject::with_object_proto(self.ctx.intrinsics());
        object.set_prototype(prototype.0.as_object());
        Value(object.into())
    }

    pub fn string_from_bytes(&mut self, bytes: &[u8]) -> Value {
        self.string(&String::from_utf8_lossy(bytes))
    }

    pub fn parse_json(&mut self, text: &[u8], _name: &str) -> Result<Value, Value> {
        let global = self.global();
        let json = self.get(&global, "JSON")?;
        let parse = self.get(&json, "parse")?;
        let text = self.string_from_bytes(text);
        self.call(&parse, &json, &[text])
    }

    pub fn bigint64(&mut self, number: i64) -> Value {
        Value(JsBigInt::from(number).into())
    }

    pub fn constructor(&mut self, name: &str, arity: u32, f: NativeFn) -> Value {
        self.native_function(name, arity, f, true)
    }

    pub fn function(&mut self, name: &str, arity: u32, f: NativeFn) -> Value {
        self.native_function(name, arity, f, false)
    }

    pub fn bound_function(&mut self, name: &str, arity: u32, f: BoundFn, data: &[Value]) -> Value {
        let captures: Vec<JsValue> = data.iter().map(|value| value.0.clone()).collect();
        let native = NativeFunction::from_copy_closure_with_captures(
            move |this, args, captures: &Vec<JsValue>, ctx| {
                let this = Value(this.clone());
                let args: Vec<Value> = args.iter().cloned().map(Value).collect();
                let data: Vec<Value> = captures.iter().cloned().map(Value).collect();
                f(&mut Scope { ctx }, &this, &args, &data)
                    .map(|value| value.0)
                    .map_err(|error| JsError::from_opaque(error.0))
            },
            captures,
        );
        let function = FunctionObjectBuilder::new(self.ctx.realm(), native)
            .name(JsString::from(name))
            .length(arity as usize)
            .build();
        Value(function.into())
    }

    fn native_function(&mut self, name: &str, arity: u32, f: NativeFn, constructor: bool) -> Value {
        let native = NativeFunction::from_copy_closure(move |this, args, ctx| {
            let this = Value(this.clone());
            let args: Vec<Value> = args.iter().cloned().map(Value).collect();
            f(&mut Scope { ctx }, &this, &args)
                .map(|value| value.0)
                .map_err(|error| JsError::from_opaque(error.0))
        });
        let function = FunctionObjectBuilder::new(self.ctx.realm(), native)
            .name(JsString::from(name))
            .length(arity as usize)
            .constructor(constructor)
            .build();
        Value(function.into())
    }

    pub fn get(&mut self, object: &Value, key: &str) -> Result<Value, Value> {
        let object = self.object(object)?;
        object
            .get(JsString::from(key), self.ctx)
            .map(Value)
            .map_err(|e| self.error(e))
    }

    pub fn set_index(&mut self, object: &Value, index: u32, value: Value) -> Result<(), Value> {
        let object = self.object(object)?;
        object
            .set(index, value.0, true, self.ctx)
            .map(|_| ())
            .map_err(|e| self.error(e))
    }

    pub fn get_index(&mut self, object: &Value, index: u32) -> Result<Value, Value> {
        let object = self.object(object)?;
        object
            .get(index, self.ctx)
            .map(Value)
            .map_err(|e| self.error(e))
    }

    fn define_key(
        &mut self,
        object: &Value,
        key: impl Into<boa_engine::property::PropertyKey>,
        value: Value,
        attributes: Attributes,
    ) -> Result<(), Value> {
        let object = self.object(object)?;
        let descriptor = BoaPropertyDescriptor::builder()
            .value(value.0)
            .writable(attributes.writable)
            .enumerable(attributes.enumerable)
            .configurable(attributes.configurable);
        object
            .define_property_or_throw(key, descriptor, self.ctx)
            .map(|_| ())
            .map_err(|e| self.error(e))
    }

    pub fn define(
        &mut self,
        object: &Value,
        key: &str,
        value: Value,
        attributes: Attributes,
    ) -> Result<(), Value> {
        self.define_key(object, JsString::from(key), value, attributes)
    }

    pub fn define_to_string_tag(&mut self, object: &Value, tag: &str) -> Result<(), Value> {
        let value = self.string(tag);
        self.define_key(
            object,
            JsSymbol::to_string_tag(),
            value,
            Attributes::CONFIGURABLE,
        )
    }

    pub fn set_constructor(&mut self, function: &Value, prototype: &Value) -> Result<(), Value> {
        let hidden = Attributes {
            writable: false,
            enumerable: false,
            configurable: false,
        };
        self.define(function, "prototype", prototype.clone(), hidden)?;
        self.define(
            prototype,
            "constructor",
            function.clone(),
            Attributes::METHOD,
        )
    }

    pub fn has_property(&mut self, object: &Value, key: &str) -> Result<bool, Value> {
        let object = self.object(object)?;
        object
            .has_property(JsString::from(key), self.ctx)
            .map_err(|e| self.error(e))
    }

    pub fn call(&mut self, function: &Value, this: &Value, args: &[Value]) -> Result<Value, Value> {
        let Some(callable) = function.0.as_callable() else {
            return Err(self.type_error("not a function"));
        };
        let args: Vec<JsValue> = args.iter().map(|arg| arg.0.clone()).collect();
        callable
            .call(&this.0, &args, self.ctx)
            .map(Value)
            .map_err(|e| self.error(e))
    }

    pub fn construct(&mut self, constructor: &Value, args: &[Value]) -> Result<Value, Value> {
        let Some(constructor) = constructor.0.as_constructor() else {
            return Err(self.type_error("not a constructor"));
        };
        let args: Vec<JsValue> = args.iter().map(|arg| arg.0.clone()).collect();
        constructor
            .construct(&args, None, self.ctx)
            .map(|object| Value(object.into()))
            .map_err(|e| self.error(e))
    }

    pub fn to_number(&mut self, value: &Value) -> Result<f64, Value> {
        value.0.to_number(self.ctx).map_err(|e| self.error(e))
    }

    pub fn to_bytes(&mut self, value: &Value) -> Result<Vec<u8>, Value> {
        value
            .0
            .to_string(self.ctx)
            .map(|text| text.to_std_string_lossy().into_bytes())
            .map_err(|e| self.error(e))
    }

    pub fn to_bool(&mut self, value: &Value) -> bool {
        value.0.to_boolean()
    }

    pub fn to_int32(&mut self, value: &Value) -> Result<i32, Value> {
        value.0.to_i32(self.ctx).map_err(|e| self.error(e))
    }

    pub fn to_int64(&mut self, value: &Value) -> Result<i64, Value> {
        value
            .0
            .to_number(self.ctx)
            .map(int64_modulo)
            .map_err(|e| self.error(e))
    }

    pub fn to_bigint64(&mut self, value: &Value) -> Result<i64, Value> {
        value.0.to_big_int64(self.ctx).map_err(|e| self.error(e))
    }

    pub fn range_error(&mut self, message: &str) -> Value {
        let error = JsNativeError::range().with_message(message.to_owned());
        Value(error.into_opaque(self.ctx).into())
    }

    pub fn new_host_object<T: Any>(&mut self, prototype: Option<&Value>, data: T) -> Value {
        let prototype = prototype.and_then(|p| p.0.as_object());
        let object = JsObject::from_proto_and_data(prototype, HostData(Box::new(data)));
        Value(object.upcast().into())
    }

    pub fn host_data<T: Any + Clone>(&mut self, value: &Value) -> Option<T> {
        let object = value.0.as_object()?;
        let data = object.downcast_ref::<HostData>()?;
        data.0.downcast_ref::<T>().cloned()
    }

    pub fn set(&mut self, object: &Value, key: &str, value: Value) -> Result<(), Value> {
        let object = self.object(object)?;
        object
            .set(JsString::from(key), value.0, true, self.ctx)
            .map(|_| ())
            .map_err(|e| self.error(e))
    }

    pub fn to_string(&mut self, value: &Value) -> Result<String, Value> {
        value
            .0
            .to_string(self.ctx)
            .map(|text| text.to_std_string_escaped())
            .map_err(|e| self.error(e))
    }

    pub fn type_error(&mut self, message: &str) -> Value {
        let error = JsNativeError::typ().with_message(message.to_owned());
        Value(error.into_opaque(self.ctx).into())
    }

    pub fn dom_exception(&mut self, name: &str, message: &str) -> Value {
        let error = JsNativeError::error()
            .with_message(message.to_owned())
            .into_opaque(self.ctx);
        let _ = error.set(
            JsString::from("name"),
            JsString::from(name),
            false,
            self.ctx,
        );
        Value(error.into())
    }

    pub fn write_object(&mut self, _value: &Value) -> Result<Vec<u8>, Option<Value>> {
        Err(Some(self.type_error(
            "object serialization is not available on this engine",
        )))
    }

    pub fn read_object(&mut self, _bytes: &[u8]) -> Result<Value, Value> {
        Err(self.type_error("object serialization is not available on this engine"))
    }

    pub fn detach_array_buffer(&mut self, value: &Value) -> Result<(), Value> {
        let buffer = value
            .0
            .as_object()
            .and_then(|object| JsArrayBuffer::from_object(object.clone()).ok());
        match buffer {
            Some(buffer) => buffer
                .detach(&JsValue::undefined())
                .map(|_| ())
                .map_err(|e| self.error(e)),
            None => Err(self.type_error("not an ArrayBuffer")),
        }
    }

    pub fn with_typed_array<R>(
        &mut self,
        value: &Value,
        f: impl FnOnce(TypedArrayBytes<'_>) -> R,
    ) -> Option<R> {
        let array = JsTypedArray::from_object(value.0.as_object()?.clone()).ok()?;
        let element_size = match array.kind()? {
            TypedArrayKind::Int8 | TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => 1,
            TypedArrayKind::Int16 | TypedArrayKind::Uint16 => 2,
            TypedArrayKind::Int32 | TypedArrayKind::Uint32 | TypedArrayKind::Float32 => 4,
            _ => 8,
        };
        let byte_offset = array.byte_offset(self.ctx).ok()?;
        let length = array.byte_length(self.ctx).ok()?;
        let buffer =
            JsArrayBuffer::from_object(array.buffer(self.ctx).ok()?.as_object()?.clone()).ok()?;
        let data = buffer.data()?;
        let bytes = data.get(byte_offset..byte_offset.checked_add(length)?)?;
        Some(f(TypedArrayBytes {
            bytes,
            byte_offset,
            element_size,
        }))
    }

    pub fn gc(&mut self) {
        boa_gc::force_collect();
    }

    pub fn own_property_keys(
        &mut self,
        object: &Value,
        symbols: bool,
    ) -> Result<Vec<Value>, Value> {
        let object = self.object(object)?;
        let keys = object
            .own_property_keys(self.ctx)
            .map_err(|e| self.error(e))?;
        Ok(keys
            .into_iter()
            .filter(|key| symbols || !matches!(key, PropertyKey::Symbol(_)))
            .map(|key| Value(key.into()))
            .collect())
    }

    pub fn own_property(
        &mut self,
        object: &Value,
        key: &Value,
    ) -> Result<Option<PropertyDescriptor>, Value> {
        let descriptor = OrdinaryObject::get_own_property_descriptor(
            &JsValue::undefined(),
            &[object.0.clone(), key.0.clone()],
            self.ctx,
        )
        .map_err(|e| self.error(e))?;
        if descriptor.is_undefined() {
            return Ok(None);
        }
        let descriptor = Value(descriptor);
        Ok(Some(PropertyDescriptor {
            value: self.get(&descriptor, "value")?,
            getter: self.get(&descriptor, "get")?,
            setter: self.get(&descriptor, "set")?,
        }))
    }

    pub fn get_prototype(&mut self, object: &Value) -> Result<Value, Value> {
        let object = self.object(object)?;
        Ok(match object.prototype() {
            Some(prototype) => Value(prototype.into()),
            None => Value::null(),
        })
    }

    pub fn is_function(&mut self, value: &Value) -> bool {
        value.0.is_callable()
    }

    pub fn new_error(&mut self) -> Value {
        Value(JsNativeError::error().into_opaque(self.ctx).into())
    }

    pub fn rejected_promise(&mut self, reason: &Value) -> Result<Value, Value> {
        let error = JsError::from_opaque(reason.0.clone());
        JsPromise::reject(error, self.ctx)
            .map(|promise| Value(promise.into()))
            .map_err(|e| self.error(e))
    }

    pub fn new_detached_realm(&mut self) -> Option<Realm> {
        self.ctx.create_realm().ok().map(Realm)
    }

    pub fn in_realm<R>(&mut self, realm: &Realm, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        let previous = self.ctx.enter_realm(realm.0.clone());
        let result = f(&mut Scope {
            ctx: &mut *self.ctx,
        });
        self.ctx.enter_realm(previous);
        result
    }

    pub fn new_realm(&mut self, init: RealmInit) -> Result<Value, Value> {
        let realm = self.ctx.create_realm().map_err(|e| self.error(e))?;
        let previous = self.ctx.enter_realm(realm);
        let result = {
            let mut scope = Scope {
                ctx: &mut *self.ctx,
            };
            init(&mut scope).map(|()| scope.global())
        };
        self.ctx.enter_realm(previous);
        result
    }
}
