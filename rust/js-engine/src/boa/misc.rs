//! Southstar — the remaining neutral scope helpers on Boa: object identities and pins; engine-private names, brands and bytecode are not available, so they report nothing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;

use boa_engine::{JsObject, JsString};

use super::{Compiled, Scope, Value};
use crate::{BrandMode, MemoryUsage};

impl Scope<'_> {
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

    pub unsafe fn clear_identity_instance(&mut self, identity: *mut c_void) {
        let object = self.object_from_identity(identity);
        unsafe { object.set_raw_instance(core::ptr::null_mut()) };
    }

    pub fn add_engine_private_name(&mut self, _name: &str) {}

    pub fn is_engine_function(&mut self, _value: &Value) -> bool {
        false
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

    pub fn import_meta(&mut self, _module: &Value) -> Result<Value, Value> {
        Err(self.type_error("import.meta is not available on this engine"))
    }
}
