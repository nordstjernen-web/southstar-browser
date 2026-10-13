//! Southstar — engine-neutral handles to a runtime and to a context (one realm), the options a runtime is created with and the outcome of running one pending job.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_void;
use core::ptr;

use crate::{Scope, Value};

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ContextHandle(pub(crate) *mut c_void);

impl ContextHandle {
    pub const NULL: ContextHandle = ContextHandle(ptr::null_mut());

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn as_ptr(self) -> *mut c_void {
        self.0
    }

    pub fn from_ptr(ptr: *mut c_void) -> ContextHandle {
        ContextHandle(ptr)
    }

    pub fn non_null(self) -> Option<ContextHandle> {
        (!self.is_null()).then_some(self)
    }

    pub fn or(self, other: ContextHandle) -> ContextHandle {
        if self.is_null() { other } else { self }
    }
}

impl Default for ContextHandle {
    fn default() -> ContextHandle {
        ContextHandle::NULL
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct RuntimeHandle(pub(crate) *mut c_void);

impl RuntimeHandle {
    pub const NULL: RuntimeHandle = RuntimeHandle(ptr::null_mut());

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }

    pub fn as_ptr(self) -> *mut c_void {
        self.0
    }

    pub fn from_ptr(ptr: *mut c_void) -> RuntimeHandle {
        RuntimeHandle(ptr)
    }
}

impl Default for RuntimeHandle {
    fn default() -> RuntimeHandle {
        RuntimeHandle::NULL
    }
}

pub type InterruptFn = fn(opaque: *mut c_void) -> bool;

pub type RejectionTrackerFn =
    for<'a> fn(scope: &mut Scope<'a>, promise: &Value, reason: &Value, handled: bool);

pub type ModuleNormalizeFn =
    for<'a> fn(scope: &mut Scope<'a>, base: &str, name: &str) -> Result<String, Value>;

pub type ModuleLoadFn =
    for<'a> fn(scope: &mut Scope<'a>, name: &str, attributes: &Value) -> Result<Value, Value>;

#[derive(Clone, Copy)]
pub struct RuntimeOptions {
    pub memory_limit: usize,
    pub stack_size: usize,
    pub interrupt: Option<InterruptFn>,
    pub interrupt_opaque: *mut c_void,
    pub rejection_tracker: Option<RejectionTrackerFn>,
    pub module_normalize: Option<ModuleNormalizeFn>,
    pub module_load: Option<ModuleLoadFn>,
}

impl Default for RuntimeOptions {
    fn default() -> RuntimeOptions {
        RuntimeOptions {
            memory_limit: 0,
            stack_size: 0,
            interrupt: None,
            interrupt_opaque: ptr::null_mut(),
            rejection_tracker: None,
            module_normalize: None,
            module_load: None,
        }
    }
}

pub enum JobOutcome {
    Idle,
    Ran(ContextHandle),
    Failed(ContextHandle, Value),
}

#[derive(Clone, Copy, Default, Debug)]
pub struct MemoryUsage {
    pub malloc_size: i64,
    pub malloc_limit: i64,
    pub memory_used_size: i64,
    pub atom_count: i64,
    pub obj_count: i64,
    pub prop_count: i64,
    pub c_func_count: i64,
}
