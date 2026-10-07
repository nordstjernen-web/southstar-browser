//! Southstar — the Boa backend of the JavaScript layer, over the boa_engine crate.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::path::Path;
use std::rc::Rc;

use boa_engine::builtins::promise::PromiseState as BoaPromiseState;
use boa_engine::module::SimpleModuleLoader;
use boa_engine::object::FunctionObjectBuilder;
use boa_engine::object::builtins::{JsArrayBuffer, JsPromise};
use boa_engine::{
    Context, JsError, JsNativeError, JsObject, JsString, JsValue, Module, NativeFunction, Source,
};

use crate::{NativeFn, PromiseState, RealmInit};

pub const ENGINE_NAME: &str = "boa";

pub fn engine_version() -> String {
    "boa_engine 0.22".to_owned()
}

#[derive(Clone)]
pub struct Value(JsValue);

impl Value {
    pub fn undefined() -> Value {
        Value(JsValue::undefined())
    }

    pub fn is_undefined(&self) -> bool {
        self.0.is_undefined()
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

    pub fn enter<R>(&mut self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        f(&mut Scope { ctx: &mut self.ctx })
    }
}

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

    pub fn function(&mut self, name: &str, arity: u32, f: NativeFn) -> Value {
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
            .constructor(false)
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

    pub fn gc(&mut self) {
        boa_gc::force_collect();
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
