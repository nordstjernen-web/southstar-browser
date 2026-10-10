//! Southstar — the Boa backend of the JavaScript layer, over the boa_engine crate.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::any::Any;
use std::path::Path;
use std::rc::Rc;

use boa_engine::builtins::object::OrdinaryObject;
use boa_engine::builtins::promise::PromiseState as BoaPromiseState;
use boa_engine::builtins::typed_array::TypedArrayKind;
use boa_engine::job::PromiseJob;
use boa_engine::module::SimpleModuleLoader;
use boa_engine::object::builtins::{
    AlignedVec, JsArray, JsArrayBuffer, JsBigInt64Array, JsBigUint64Array, JsFloat32Array,
    JsFloat64Array, JsInt8Array, JsInt16Array, JsInt32Array, JsPromise, JsTypedArray, JsUint8Array,
    JsUint8ClampedArray, JsUint16Array, JsUint32Array,
};
use boa_engine::object::{FunctionObjectBuilder, IntegrityLevel};
use boa_engine::prelude::{Finalize, JsData, Trace as BoaTrace};
use boa_engine::property::{PropertyDescriptor as BoaPropertyDescriptor, PropertyKey};
use boa_engine::{
    Context, JsBigInt, JsError, JsNativeError, JsObject, JsString, JsSymbol, JsValue, Module,
    NativeFunction, Source,
};
use boa_gc::custom_trace;

use boa_engine::builtins::error::Error as BoaError;
use boa_engine::native_function::NativeFunctionObject;
use boa_engine::object::builtins::{JsDataView, JsDate, JsMap, JsRegExp, JsSet};

use crate::{
    Attributes, BoundFn, ElementType, Job, NativeFn, ObjectKind, PromiseState, PropertyDescriptor,
    RealmInit, Trace, TypedArrayBytes, TypedArrayView, int64_modulo,
};

pub const ENGINE_NAME: &str = "boa";

pub fn engine_version() -> String {
    "boa_engine 0.22".to_owned()
}

#[derive(Clone)]
pub struct Value(JsValue);

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct ObjectKey(JsObject);

impl ObjectKey {
    pub fn value(&self) -> Value {
        Value(self.0.clone().into())
    }
}

type HostTrace = fn(&dyn Any, &mut dyn FnMut(&Value));

#[derive(JsData)]
struct HostData {
    data: Box<dyn Any>,
    trace: Option<HostTrace>,
}

impl Finalize for HostData {}

unsafe impl BoaTrace for HostData {
    custom_trace!(this, mark, {
        if let Some(trace) = this.trace {
            trace(&*this.data, &mut |value: &Value| mark(&value.0));
        }
    });
}

fn trace_as<T: Any + Trace>(data: &dyn Any, visit: &mut dyn FnMut(&Value)) {
    if let Some(data) = data.downcast_ref::<T>() {
        data.trace(visit);
    }
}

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

    pub fn is_symbol(&self) -> bool {
        self.0.is_symbol()
    }

    pub fn object_key(&self) -> Option<ObjectKey> {
        self.0.as_object().map(|o| ObjectKey(o.clone()))
    }

    pub fn same_object(&self, other: &Value) -> bool {
        match (self.0.as_object(), other.0.as_object()) {
            (Some(a), Some(b)) => JsObject::equals(&a, &b),
            _ => false,
        }
    }

    pub fn with_host<T: Any, R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        let object = self.0.as_object()?;
        let data = object.downcast_ref::<HostData>()?;
        data.data.downcast_ref::<T>().map(f)
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

    pub fn eval_native_script(&mut self, source: &str, name: &str) -> Result<Value, Value> {
        self.eval_script(source, name)
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

    pub fn constructor_or_function(&mut self, name: &str, arity: u32, f: NativeFn) -> Value {
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

    pub fn bound_constructor(
        &mut self,
        name: &str,
        arity: u32,
        f: BoundFn,
        data: &[Value],
    ) -> Value {
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
            .constructor(true)
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

    pub fn define_accessor(
        &mut self,
        object: &Value,
        key: &str,
        getter: Option<&Value>,
        setter: Option<&Value>,
        attributes: Attributes,
    ) -> Result<(), Value> {
        let object = self.object(object)?;
        let accessor = |value: Option<&Value>| value.and_then(|v| v.0.as_object());
        let descriptor = BoaPropertyDescriptor::builder()
            .maybe_get(accessor(getter))
            .maybe_set(accessor(setter))
            .enumerable(attributes.enumerable)
            .configurable(attributes.configurable);
        object
            .define_property_or_throw(JsString::from(key), descriptor, self.ctx)
            .map(|_| ())
            .map_err(|e| self.error(e))
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

    pub fn delete(&mut self, object: &Value, key: &str) -> Result<bool, Value> {
        let object = self.object(object)?;
        object
            .delete_property_or_throw(JsString::from(key), self.ctx)
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
        self.host_object(prototype, data, None)
    }

    pub fn new_traced_host_object<T: Any + Trace>(
        &mut self,
        prototype: Option<&Value>,
        data: T,
    ) -> Value {
        self.host_object(prototype, data, Some(trace_as::<T>))
    }

    fn host_object<T: Any>(
        &mut self,
        prototype: Option<&Value>,
        data: T,
        trace: Option<HostTrace>,
    ) -> Value {
        let prototype = prototype.and_then(|p| p.0.as_object());
        let host = HostData {
            data: Box::new(data),
            trace,
        };
        let object = JsObject::from_proto_and_data(prototype, host);
        Value(object.upcast().into())
    }

    pub fn host_data<T: Any + Clone>(&mut self, value: &Value) -> Option<T> {
        let object = value.0.as_object()?;
        let data = object.downcast_ref::<HostData>()?;
        data.data.downcast_ref::<T>().cloned()
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

    pub fn syntax_error(&mut self, message: &str) -> Value {
        let error = JsNativeError::syntax().with_message(message.to_owned());
        Value(error.into_opaque(self.ctx).into())
    }

    pub fn freeze(&mut self, object: &Value) -> Result<(), Value> {
        let object = self.object(object)?;
        object
            .set_integrity_level(IntegrityLevel::Frozen, self.ctx)
            .map(|_| ())
            .map_err(|e| self.error(e))
    }

    pub fn get_key(&mut self, object: &Value, key: &Value) -> Result<Value, Value> {
        let object = self.object(object)?;
        let key = key.0.to_property_key(self.ctx).map_err(|e| self.error(e))?;
        object
            .get(key, self.ctx)
            .map(Value)
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

    pub unsafe fn external_array_buffer(
        &mut self,
        _data: *mut u8,
        _len: usize,
    ) -> Result<Value, Value> {
        Err(self.type_error("ArrayBuffers over external memory are not available on this engine"))
    }

    pub fn buffer_source_bytes(&mut self, value: &Value) -> Option<Vec<u8>> {
        self.with_buffer_bytes_mut(value, |bytes| bytes.to_vec())
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

    pub fn typed_array_element(&mut self, value: &Value) -> Option<ElementType> {
        let array = JsTypedArray::from_object(value.0.as_object()?.clone()).ok()?;
        Some(match array.kind()? {
            TypedArrayKind::Int8 => ElementType::Int8,
            TypedArrayKind::Uint8 => ElementType::Uint8,
            TypedArrayKind::Uint8Clamped => ElementType::Uint8Clamped,
            TypedArrayKind::Int16 => ElementType::Int16,
            TypedArrayKind::Uint16 => ElementType::Uint16,
            TypedArrayKind::Int32 => ElementType::Int32,
            TypedArrayKind::Uint32 => ElementType::Uint32,
            TypedArrayKind::BigInt64 => ElementType::BigInt64,
            TypedArrayKind::BigUint64 => ElementType::BigUint64,
            TypedArrayKind::Float32 => ElementType::Float32,
            TypedArrayKind::Float64 => ElementType::Float64,
            #[allow(unreachable_patterns)]
            _ => ElementType::Float16,
        })
    }

    pub fn with_buffer_bytes_mut<R>(
        &mut self,
        value: &Value,
        f: impl FnOnce(&mut [u8]) -> R,
    ) -> Option<R> {
        let object = value.0.as_object()?.clone();
        if let Ok(buffer) = JsArrayBuffer::from_object(object.clone()) {
            let mut data = buffer.data_mut()?;
            return Some(f(&mut data));
        }
        let array = JsTypedArray::from_object(object).ok()?;
        let byte_offset = array.byte_offset(self.ctx).ok()?;
        let length = array.byte_length(self.ctx).ok()?;
        let buffer =
            JsArrayBuffer::from_object(array.buffer(self.ctx).ok()?.as_object()?.clone()).ok()?;
        let mut data = buffer.data_mut()?;
        let bytes = data.get_mut(byte_offset..byte_offset.checked_add(length)?)?;
        Some(f(bytes))
    }

    pub fn new_typed_array(&mut self, kind: ElementType, bytes: &[u8]) -> Result<Value, Value> {
        let block = AlignedVec::from_iter(0, bytes.iter().copied());
        let buffer = JsArrayBuffer::from_byte_block(block, self.ctx).map_err(|e| self.error(e))?;
        let ctx = &mut *self.ctx;
        let array: Result<JsValue, JsError> = match kind {
            ElementType::Int8 => JsInt8Array::from_array_buffer(buffer, ctx).map(Into::into),
            ElementType::Uint8 => JsUint8Array::from_array_buffer(buffer, ctx).map(Into::into),
            ElementType::Uint8Clamped => {
                JsUint8ClampedArray::from_array_buffer(buffer, ctx).map(Into::into)
            }
            ElementType::Int16 => JsInt16Array::from_array_buffer(buffer, ctx).map(Into::into),
            ElementType::Uint16 => JsUint16Array::from_array_buffer(buffer, ctx).map(Into::into),
            ElementType::Int32 => JsInt32Array::from_array_buffer(buffer, ctx).map(Into::into),
            ElementType::Uint32 => JsUint32Array::from_array_buffer(buffer, ctx).map(Into::into),
            ElementType::BigInt64 => {
                JsBigInt64Array::from_array_buffer(buffer, ctx).map(Into::into)
            }
            ElementType::BigUint64 => {
                JsBigUint64Array::from_array_buffer(buffer, ctx).map(Into::into)
            }
            ElementType::Float32 => JsFloat32Array::from_array_buffer(buffer, ctx).map(Into::into),
            ElementType::Float64 => JsFloat64Array::from_array_buffer(buffer, ctx).map(Into::into),
            ElementType::Float16 => {
                return Err(self.type_error("Float16Array is not available on this engine"));
            }
        };
        array.map(Value).map_err(|e| self.error(e))
    }

    pub fn view_data(&mut self, value: &Value) -> Option<Vec<u8>> {
        let object = value.0.as_object()?.clone();
        let (buffer, byte_offset, length) = match JsTypedArray::from_object(object.clone()) {
            Ok(array) => (
                array.buffer(self.ctx).ok()?,
                array.byte_offset(self.ctx).ok()?,
                array.byte_length(self.ctx).ok()?,
            ),
            Err(_) => {
                let view = JsDataView::from_object(object).ok()?;
                (
                    view.buffer(self.ctx).ok()?,
                    usize::try_from(view.byte_offset(self.ctx).ok()?).ok()?,
                    usize::try_from(view.byte_length(self.ctx).ok()?).ok()?,
                )
            }
        };
        let buffer = JsArrayBuffer::from_object(buffer.as_object()?.clone()).ok()?;
        let data = buffer.data()?;
        Some(
            data.get(byte_offset..byte_offset.checked_add(length)?)?
                .to_vec(),
        )
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
            accessor: self.has_property(&descriptor, "get")?,
        }))
    }

    pub fn object_kind(&mut self, value: &Value) -> Option<ObjectKind> {
        let object = value.0.as_object()?.clone();
        if JsArrayBuffer::from_object(object.clone()).is_ok() {
            return Some(ObjectKind::ArrayBuffer);
        }
        if let Some(element) = self.typed_array_element(value) {
            return Some(ObjectKind::TypedArray(element));
        }
        Some(if JsDataView::from_object(object.clone()).is_ok() {
            ObjectKind::DataView
        } else if JsDate::from_object(object.clone()).is_ok() {
            ObjectKind::Date
        } else if JsRegExp::from_object(object.clone()).is_ok() {
            ObjectKind::RegExp
        } else if JsMap::from_object(object.clone()).is_ok() {
            ObjectKind::Map
        } else if JsSet::from_object(object.clone()).is_ok() {
            ObjectKind::Set
        } else if object.is::<BoaError>() {
            ObjectKind::Error
        } else if object.is::<f64>() {
            ObjectKind::Number
        } else if object.is::<JsString>() {
            ObjectKind::String
        } else if object.is::<bool>() {
            ObjectKind::Boolean
        } else if object.is::<JsBigInt>() {
            ObjectKind::BigInt
        } else {
            ObjectKind::Other
        })
    }

    pub fn typed_array_view(&mut self, value: &Value) -> Result<Option<TypedArrayView>, Value> {
        let Some(element) = self.typed_array_element(value) else {
            return Ok(None);
        };
        let object = self.object(value)?;
        let array = JsTypedArray::from_object(object).map_err(|e| self.error(e))?;
        let buffer = array.buffer(self.ctx).map_err(|e| self.error(e))?;
        let byte_offset = array.byte_offset(self.ctx).map_err(|e| self.error(e))?;
        let length = array.length(self.ctx).map_err(|e| self.error(e))?;
        Ok(Some(TypedArrayView {
            buffer: Value(buffer),
            byte_offset,
            length,
            element,
        }))
    }

    pub fn new_array_buffer(&mut self, bytes: &[u8]) -> Result<Value, Value> {
        let block = AlignedVec::from_iter(0, bytes.iter().copied());
        JsArrayBuffer::from_byte_block(block, self.ctx)
            .map(|buffer| Value(buffer.into()))
            .map_err(|e| self.error(e))
    }

    pub fn array_buffer_bytes(&mut self, value: &Value) -> Option<Vec<u8>> {
        let buffer = JsArrayBuffer::from_object(value.0.as_object()?.clone()).ok()?;
        let data = buffer.data()?;
        Some(data.to_vec())
    }

    pub fn own_enumerable_keys(&mut self, object: &Value) -> Result<Vec<Value>, Value> {
        let keys = OrdinaryObject::keys(
            &JsValue::undefined(),
            std::slice::from_ref(&object.0),
            self.ctx,
        )
        .map_err(|e| self.error(e))?;
        let keys = Value(keys);
        let count = self.get(&keys, "length")?;
        let count = self.to_number(&count)? as u32;
        (0..count).map(|i| self.get_index(&keys, i)).collect()
    }

    pub fn set_key(&mut self, object: &Value, key: &Value, value: Value) -> Result<(), Value> {
        let target = self.object(object)?;
        let key = key.0.to_property_key(self.ctx).map_err(|e| self.error(e))?;
        target
            .set(key, value.0, true, self.ctx)
            .map(|_| ())
            .map_err(|e| self.error(e))
    }

    pub fn define_with_key(
        &mut self,
        object: &Value,
        key: &Value,
        value: Value,
        attributes: Attributes,
    ) -> Result<(), Value> {
        let key = key.0.to_property_key(self.ctx).map_err(|e| self.error(e))?;
        self.define_key(object, key, value, attributes)
    }

    pub fn delete_key(&mut self, object: &Value, key: &Value) -> Result<bool, Value> {
        let target = self.object(object)?;
        let key = key.0.to_property_key(self.ctx).map_err(|e| self.error(e))?;
        target
            .delete_property_or_throw(key, self.ctx)
            .map_err(|e| self.error(e))
    }

    pub fn define_entry(&mut self, object: &Value, key: &Value, value: Value) -> Result<(), Value> {
        let target = self.object(object)?;
        let key = key.0.to_property_key(self.ctx).map_err(|e| self.error(e))?;
        let descriptor = BoaPropertyDescriptor::builder()
            .value(value.0)
            .writable(true)
            .enumerable(true)
            .configurable(true);
        target
            .define_property_or_throw(key, descriptor, self.ctx)
            .map(|_| ())
            .map_err(|e| self.error(e))
    }

    pub fn is_native_function(&mut self, value: &Value) -> bool {
        value
            .0
            .as_object()
            .is_some_and(|o| o.is::<NativeFunctionObject>())
    }

    pub fn to_object(&mut self, value: &Value) -> Result<Value, Value> {
        value
            .0
            .to_object(self.ctx)
            .map(|o| Value(o.into()))
            .map_err(|e| self.error(e))
    }

    pub fn set_prototype(&mut self, object: &Value, prototype: &Value) -> Result<(), Value> {
        let object = self.object(object)?;
        object.set_prototype(prototype.0.as_object());
        Ok(())
    }

    pub fn enqueue_job(&mut self, job: Job) -> Result<(), Value> {
        let native = PromiseJob::new(move |ctx| {
            job(&mut Scope { ctx });
            Ok(JsValue::undefined())
        });
        self.ctx.enqueue_job(native.into());
        Ok(())
    }

    pub fn is_error(&mut self, value: &Value) -> bool {
        value
            .0
            .as_object()
            .is_some_and(|object| object.is::<boa_engine::builtins::error::Error>())
    }

    pub fn json_stringify(&mut self, value: &Value) -> Result<Value, Value> {
        let json = self.ctx.intrinsics().objects().json();
        let stringify = json
            .get(boa_engine::js_string!("stringify"), self.ctx)
            .map_err(|e| self.error(e))?;
        self.call(
            &Value(stringify),
            &Value(json.into()),
            core::slice::from_ref(value),
        )
    }

    pub fn enqueue_call(&mut self, function: &Value, args: &[Value]) -> Result<(), Value> {
        let Some(callable) = function.0.as_callable() else {
            return Err(self.type_error("not a function"));
        };
        let args: Vec<JsValue> = args.iter().map(|arg| arg.0.clone()).collect();
        let native = PromiseJob::new(move |ctx| callable.call(&JsValue::undefined(), &args, ctx));
        self.ctx.enqueue_job(native.into());
        Ok(())
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

    pub fn to_string_value(&mut self, value: &Value) -> Result<Value, Value> {
        value
            .0
            .to_string(self.ctx)
            .map(|text| Value(text.into()))
            .map_err(|e| self.error(e))
    }

    pub fn is_constructor(&mut self, value: &Value) -> bool {
        value.0.is_constructor()
    }

    pub fn instance_of(&mut self, value: &Value, constructor: &Value) -> bool {
        constructor.0.is_object()
            && value
                .0
                .instance_of(&constructor.0, self.ctx)
                .unwrap_or(false)
    }

    pub fn new_error(&mut self) -> Value {
        Value(JsNativeError::error().into_opaque(self.ctx).into())
    }

    pub fn new_promise(&mut self) -> Result<(Value, Value, Value), Value> {
        let (promise, resolvers) = JsPromise::new_pending(self.ctx);
        Ok((
            Value(promise.into()),
            Value(resolvers.resolve.into()),
            Value(resolvers.reject.into()),
        ))
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
