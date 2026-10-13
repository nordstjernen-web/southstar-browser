//! Southstar — the remaining neutral scope helpers on Boa: object identities and pins; import.meta; engine-private names, brands and bytecode are not available, so they report nothing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;

use boa_engine::job::PromiseJob;
use boa_engine::{JsError, JsObject, JsString};

use super::{Compiled, Scope, Value};
use crate::{BrandMode, MemoryUsage, TaskFn};

pub unsafe fn clear_identity_instance(identity: *mut c_void) {
    if !identity.is_null() {
        let object = Value(unsafe { JsObject::from_identity(identity) }.into());
        unsafe { object.set_raw_instance(core::ptr::null_mut()) };
    }
}

impl Scope<'_> {
    pub fn enqueue_native(&mut self, f: TaskFn, args: &[Value]) -> Result<(), Value> {
        let args = args.to_vec();
        let realm = self.cx().realm().clone();
        let job = PromiseJob::with_realm(
            move |ctx| {
                let _host = super::access::enter(ctx);
                f(&mut Scope::of(ctx), &args)
                    .map(|value| value.0)
                    .map_err(|error| JsError::from_opaque(error.0))
            },
            realm,
        );
        self.cx().enqueue_job(job.into());
        Ok(())
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn object_from_identity(&mut self, identity: *mut c_void) -> Value {
        if identity.is_null() {
            return Value::undefined();
        }
        Value(unsafe { JsObject::from_identity(identity) }.into())
    }

    pub fn retain_identity(&mut self, object: &Value) -> *mut c_void {
        object
            .0
            .as_object()
            .map_or(core::ptr::null_mut(), |object| object.retain_identity())
    }

    pub unsafe fn release_identity(&mut self, identity: *mut c_void) {
        if !identity.is_null() {
            unsafe { JsObject::release_identity(identity) };
        }
    }

    pub fn add_engine_private_name(&mut self, _name: &str) {}

    pub fn is_engine_function(&mut self, value: &Value) -> bool {
        self.is_native_function(value)
    }

    pub fn array_buffer_ptr(&mut self, value: &Value) -> Option<(*mut u8, usize)> {
        let object = value.0.as_object()?.clone();
        let buffer = boa_engine::object::builtins::JsArrayBuffer::from_object(object).ok()?;
        let mut data = buffer.data_mut()?;
        Some((data.as_mut_ptr(), data.len()))
    }

    pub fn to_index(&mut self, value: &Value) -> Result<u64, Value> {
        value.0.to_index(self.cx()).map_err(|e| self.error(e))
    }

    pub fn new_function_brand(&mut self, _class_ids: &[u32]) -> i32 {
        0
    }

    pub fn set_function_brand(&mut self, _function: &Value, _brand: i32, _mode: BrandMode) {}

    pub fn memory_usage(&mut self) -> MemoryUsage {
        MemoryUsage::default()
    }

    pub fn write_bytecode(&mut self, _compiled: &Value) -> Option<Vec<u8>> {
        None
    }

    pub fn read_bytecode(&mut self, _bytes: &[u8]) -> Result<Value, Value> {
        Err(self.type_error("bytecode is not available on this engine"))
    }

    fn module_of(value: &Value) -> Option<boa_engine::Module> {
        let object = value.0.as_object()?;
        let compiled = object.downcast_ref::<Compiled>()?;
        match &*compiled {
            Compiled::Module(module) => Some(module.clone()),
            Compiled::Script(_) => None,
        }
    }

    pub fn is_module(&mut self, value: &Value) -> bool {
        Self::module_of(value).is_some()
    }

    pub fn module_name(&mut self, module: &Value) -> Result<Value, Value> {
        let Some(module) = Self::module_of(module) else {
            return Err(self.type_error("not a module"));
        };
        let name = module
            .path()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        Ok(Value(JsString::from(name.as_str()).into()))
    }

    pub fn import_meta(&mut self, module: &Value) -> Result<Value, Value> {
        let Some(module) = Self::module_of(module) else {
            return Err(self.type_error("not a module"));
        };
        match module.import_meta(self.cx()) {
            Some(meta) => Ok(Value(meta.into())),
            None => Err(self.type_error("a synthetic module has no import.meta")),
        }
    }
}
