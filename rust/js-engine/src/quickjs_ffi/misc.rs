//! Southstar — the remaining neutral scope helpers on QuickJS: object identities and pins, engine-private names, function brands, indexes and the bytecode and module records.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;

use super::quickjs;
use super::{JS_DupValue, JS_FreeValue, Scope, Value};
use crate::{BrandMode, MemoryUsage};

impl Scope<'_> {
    pub fn object_from_identity(&mut self, identity: *mut c_void) -> Value {
        if identity.is_null() {
            return Value::undefined();
        }
        quickjs::object_from_identity(self, identity)
    }

    pub fn retain_identity(&mut self, object: &Value) -> *mut c_void {
        let identity = object.object_identity();
        if !identity.is_null() {
            unsafe { JS_DupValue(self.ctx, quickjs::identity_raw(identity)) };
        }
        identity
    }

    pub unsafe fn release_identity(&mut self, identity: *mut c_void) {
        if !identity.is_null() {
            unsafe { JS_FreeValue(self.ctx, quickjs::identity_raw(identity)) };
        }
    }

    pub unsafe fn clear_identity_instance(&mut self, identity: *mut c_void) {
        if !identity.is_null() {
            unsafe { quickjs::set_identity_opaque(identity, core::ptr::null_mut()) };
        }
    }

    pub fn add_engine_private_name(&mut self, name: &str) {
        quickjs::add_engine_private_name(self, name);
    }

    pub fn is_engine_function(&mut self, value: &Value) -> bool {
        quickjs::is_engine_function(value)
    }

    pub fn to_index(&mut self, value: &Value) -> Result<u64, Value> {
        quickjs::to_index(self, value)
    }

    pub fn new_function_brand(&mut self, class_ids: &[u32]) -> i32 {
        quickjs::new_function_brand(self, class_ids)
    }

    pub fn set_function_brand(&mut self, function: &Value, brand: i32, mode: BrandMode) {
        quickjs::set_function_brand(self, function, brand, mode);
    }

    pub fn memory_usage(&mut self) -> MemoryUsage {
        self.runtime().memory_usage()
    }

    pub fn write_bytecode(&mut self, compiled: &Value) -> Option<Vec<u8>> {
        quickjs::write_bytecode(self, compiled)
    }

    pub fn read_bytecode(&mut self, bytes: &[u8]) -> Result<Value, Value> {
        quickjs::read_bytecode(self, bytes)
    }

    pub fn is_module(&mut self, value: &Value) -> bool {
        quickjs::is_module(value)
    }

    pub fn module_name(&mut self, module: &Value) -> Result<Value, Value> {
        if !quickjs::is_module(module) {
            return Err(self.type_error("not a module"));
        }
        quickjs::module_name(self, module)
    }

    pub fn import_meta(&mut self, module: &Value) -> Result<Value, Value> {
        if !quickjs::is_module(module) {
            return Err(self.type_error("not a module"));
        }
        quickjs::import_meta(self, module)
    }
}
