//! Southstar — runtimes, contexts and their handles on QuickJS: a context handle is the JSContext, a runtime handle the JSRuntime, and the neutral hooks ride the runtime's opaque pointer.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use super::{
    JS_ExecutePendingJob, JS_FreeContext, JS_FreeRuntime, JS_GetException, JS_GetRuntime,
    JS_NewContext, JS_NewRuntime, JS_RunGC, JS_SetMaxStackSize, JS_Throw, JSContext, JSRuntime,
    JSValue, Scope, TAG_MODULE, Value,
};
use crate::{ContextHandle, JobOutcome, MemoryUsage, RuntimeHandle, RuntimeOptions};

type InterruptHandler = unsafe extern "C" fn(rt: *mut c_void, opaque: *mut c_void) -> c_int;
type RejectionTracker = unsafe extern "C" fn(
    ctx: *mut JSContext,
    promise: JSValue,
    reason: JSValue,
    is_handled: bool,
    opaque: *mut c_void,
);
type ModuleNormalize = unsafe extern "C" fn(
    ctx: *mut JSContext,
    base: *const c_char,
    name: *const c_char,
    opaque: *mut c_void,
) -> *mut c_char;
type ModuleLoader = unsafe extern "C" fn(
    ctx: *mut JSContext,
    name: *const c_char,
    opaque: *mut c_void,
    attributes: JSValue,
) -> *mut c_void;

#[repr(C)]
struct JSMemoryUsage {
    malloc_size: i64,
    malloc_limit: i64,
    memory_used_size: i64,
    rest: [i64; 61],
}

unsafe extern "C" {
    fn JS_SetInterruptHandler(rt: *mut c_void, cb: InterruptHandler, opaque: *mut c_void);
    fn JS_SetMemoryLimit(rt: *mut c_void, limit: usize);
    fn JS_SetHostPromiseRejectionTracker(
        rt: *mut c_void,
        cb: RejectionTracker,
        opaque: *mut c_void,
    );
    fn JS_SetRuntimeOpaque(rt: *mut c_void, opaque: *mut c_void);
    fn JS_GetRuntimeOpaque(rt: *mut JSRuntime) -> *mut c_void;
    fn JS_SetContextOpaque(ctx: *mut JSContext, opaque: *mut c_void);
    fn JS_GetContextOpaque(ctx: *mut JSContext) -> *mut c_void;
    fn JS_SetModuleLoaderFunc2(
        rt: *mut c_void,
        normalize: ModuleNormalize,
        loader: ModuleLoader,
        check_attributes: *const c_void,
        opaque: *mut c_void,
    );
    fn JS_GetPendingJobRealm(rt: *mut JSRuntime) -> *mut JSContext;
    fn JS_IsJobPending(rt: *mut JSRuntime) -> bool;
    fn JS_IsRunningScript(ctx: *mut JSContext) -> bool;
    fn JS_GetCallerRealm(ctx: *mut JSContext) -> *mut JSContext;
    fn JS_GetFunctionRealm(ctx: *mut JSContext, func_obj: JSValue) -> *mut JSContext;
    fn JS_ComputeMemoryUsage(rt: *mut JSRuntime, s: *mut JSMemoryUsage);
    fn js_strndup(ctx: *mut JSContext, s: *const c_char, n: usize) -> *mut c_char;
}

struct Hooks {
    options: RuntimeOptions,
    opaque: Cell<*mut c_void>,
}

fn hooks<'a>(rt: *mut JSRuntime) -> Option<&'a Hooks> {
    if rt.is_null() {
        return None;
    }
    unsafe { JS_GetRuntimeOpaque(rt).cast::<Hooks>().as_ref() }
}

unsafe extern "C" fn interrupt(_rt: *mut c_void, opaque: *mut c_void) -> c_int {
    let Some(hooks) = (unsafe { opaque.cast::<Hooks>().as_ref() }) else {
        return 0;
    };
    hooks
        .options
        .interrupt
        .map_or(0, |f| c_int::from(f(hooks.options.interrupt_opaque)))
}

unsafe extern "C" fn track_rejection(
    ctx: *mut JSContext,
    promise: JSValue,
    reason: JSValue,
    is_handled: bool,
    opaque: *mut c_void,
) {
    let Some(tracker) =
        (unsafe { opaque.cast::<Hooks>().as_ref() }).and_then(|h| h.options.rejection_tracker)
    else {
        return;
    };
    let mut scope = Scope::of(ctx);
    let promise = unsafe { super::quickjs::borrow_value(&scope, promise) };
    let reason = unsafe { super::quickjs::borrow_value(&scope, reason) };
    tracker(&mut scope, &promise, &reason, is_handled);
}

unsafe fn lossy<'a>(text: *const c_char) -> std::borrow::Cow<'a, str> {
    if text.is_null() {
        std::borrow::Cow::Borrowed("")
    } else {
        unsafe { CStr::from_ptr(text) }.to_string_lossy()
    }
}

unsafe extern "C" fn normalize_module(
    ctx: *mut JSContext,
    base: *const c_char,
    name: *const c_char,
    opaque: *mut c_void,
) -> *mut c_char {
    let normalize =
        (unsafe { opaque.cast::<Hooks>().as_ref() }).and_then(|h| h.options.module_normalize);
    let (base, name) = unsafe { (lossy(base), lossy(name)) };
    let mut scope = Scope::of(ctx);
    let normalized = match normalize {
        Some(normalize) => normalize(&mut scope, &base, &name),
        None => Ok(name.into_owned()),
    };
    match normalized {
        Ok(text) => {
            let text = text.replace('\0', "");
            unsafe { js_strndup(ctx, text.as_ptr().cast(), text.len()) }
        }
        Err(error) => {
            unsafe { JS_Throw(ctx, error.into_raw()) };
            ptr::null_mut()
        }
    }
}

unsafe extern "C" fn load_module(
    ctx: *mut JSContext,
    name: *const c_char,
    opaque: *mut c_void,
    attributes: JSValue,
) -> *mut c_void {
    let Some(load) =
        (unsafe { opaque.cast::<Hooks>().as_ref() }).and_then(|h| h.options.module_load)
    else {
        return ptr::null_mut();
    };
    let name = unsafe { lossy(name) };
    let mut scope = Scope::of(ctx);
    let attributes = unsafe { super::quickjs::borrow_value(&scope, attributes) };
    match load(&mut scope, &name, &attributes) {
        Ok(module) if module.raw.tag == TAG_MODULE => unsafe { module.raw.u.ptr },
        Ok(_) => {
            let error = scope.type_error("module loader did not return a module");
            unsafe { JS_Throw(ctx, error.into_raw()) };
            ptr::null_mut()
        }
        Err(error) => {
            unsafe { JS_Throw(ctx, error.into_raw()) };
            ptr::null_mut()
        }
    }
}

impl RuntimeHandle {
    fn rt(self) -> *mut JSRuntime {
        self.0.cast()
    }

    pub fn new(options: &RuntimeOptions) -> RuntimeHandle {
        let rt = unsafe { JS_NewRuntime() };
        if rt.is_null() {
            return RuntimeHandle::NULL;
        }
        let hooks = Box::into_raw(Box::new(Hooks {
            options: *options,
            opaque: Cell::new(ptr::null_mut()),
        }));
        let raw = rt.cast::<c_void>();
        unsafe {
            JS_SetRuntimeOpaque(raw, hooks.cast());
            if options.memory_limit != 0 {
                JS_SetMemoryLimit(raw, options.memory_limit);
            }
            if options.stack_size != 0 {
                JS_SetMaxStackSize(rt, options.stack_size);
            }
            if options.interrupt.is_some() {
                JS_SetInterruptHandler(raw, interrupt, hooks.cast());
            }
            if options.rejection_tracker.is_some() {
                JS_SetHostPromiseRejectionTracker(raw, track_rejection, hooks.cast());
            }
            if options.module_load.is_some() {
                JS_SetModuleLoaderFunc2(
                    raw,
                    normalize_module,
                    load_module,
                    ptr::null(),
                    hooks.cast(),
                );
            }
        }
        RuntimeHandle(rt.cast())
    }

    pub unsafe fn free(self) {
        let rt = self.rt();
        if rt.is_null() {
            return;
        }
        let hooks = unsafe { JS_GetRuntimeOpaque(rt) }.cast::<Hooks>();
        unsafe { JS_FreeRuntime(rt) };
        if !hooks.is_null() {
            drop(unsafe { Box::from_raw(hooks) });
        }
    }

    pub fn opaque(self) -> *mut c_void {
        hooks(self.rt()).map_or(ptr::null_mut(), |h| h.opaque.get())
    }

    pub fn set_opaque(self, opaque: *mut c_void) {
        if let Some(hooks) = hooks(self.rt()) {
            hooks.opaque.set(opaque);
        }
    }

    pub fn set_max_stack_size(self, bytes: usize) {
        if !self.is_null() {
            unsafe { JS_SetMaxStackSize(self.rt(), bytes) };
        }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn new_context(self, opaque: *mut c_void) -> ContextHandle {
        if self.is_null() {
            return ContextHandle::NULL;
        }
        let ctx = unsafe { JS_NewContext(self.rt()) };
        if !ctx.is_null() {
            unsafe { JS_SetContextOpaque(ctx, opaque) };
        }
        ContextHandle(ctx.cast())
    }

    pub fn has_pending_jobs(self) -> bool {
        !self.is_null() && unsafe { JS_IsJobPending(self.rt()) }
    }

    pub fn pending_job_context(self) -> ContextHandle {
        if self.is_null() {
            return ContextHandle::NULL;
        }
        ContextHandle(unsafe { JS_GetPendingJobRealm(self.rt()) }.cast())
    }

    pub fn run_pending_job(self) -> JobOutcome {
        if self.is_null() {
            return JobOutcome::Idle;
        }
        let mut ctx: *mut JSContext = ptr::null_mut();
        let status = unsafe { JS_ExecutePendingJob(self.rt(), &mut ctx) };
        let handle = ContextHandle(ctx.cast());
        match status {
            0 => JobOutcome::Idle,
            s if s > 0 => JobOutcome::Ran(handle),
            _ if ctx.is_null() => JobOutcome::Failed(handle, Value::undefined()),
            _ => {
                let exception = unsafe { JS_GetException(ctx) };
                JobOutcome::Failed(handle, Value::own(ctx, exception))
            }
        }
    }

    pub fn gc(self) {
        if !self.is_null() {
            unsafe { JS_RunGC(self.rt()) };
        }
    }

    pub fn memory_usage(self) -> MemoryUsage {
        if self.is_null() {
            return MemoryUsage::default();
        }
        let mut usage = JSMemoryUsage {
            malloc_size: 0,
            malloc_limit: 0,
            memory_used_size: 0,
            rest: [0; 61],
        };
        unsafe { JS_ComputeMemoryUsage(self.rt(), &mut usage) };
        MemoryUsage {
            malloc_size: usage.malloc_size,
            malloc_limit: usage.malloc_limit,
            memory_used_size: usage.memory_used_size,
            atom_count: usage.rest[2],
            obj_count: usage.rest[6],
            prop_count: usage.rest[8],
            c_func_count: usage.rest[17],
        }
    }
}

impl ContextHandle {
    fn ctx(self) -> *mut JSContext {
        self.0.cast()
    }

    pub fn with<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        if self.is_null() {
            return None;
        }
        Some(f(&mut Scope::of(self.ctx())))
    }

    pub fn opaque(self) -> *mut c_void {
        if self.is_null() {
            return ptr::null_mut();
        }
        unsafe { JS_GetContextOpaque(self.ctx()) }
    }

    #[allow(clippy::not_unsafe_ptr_arg_deref)]
    pub fn set_opaque(self, opaque: *mut c_void) {
        if !self.is_null() {
            unsafe { JS_SetContextOpaque(self.ctx(), opaque) };
        }
    }

    pub fn runtime(self) -> RuntimeHandle {
        if self.is_null() {
            return RuntimeHandle::NULL;
        }
        RuntimeHandle(unsafe { JS_GetRuntime(self.ctx()) }.cast())
    }

    pub unsafe fn free(self) {
        if !self.is_null() {
            unsafe { JS_FreeContext(self.ctx()) };
        }
    }
}

impl Scope<'_> {
    pub fn handle(&self) -> ContextHandle {
        ContextHandle(self.ctx.cast())
    }

    pub fn runtime(&self) -> RuntimeHandle {
        RuntimeHandle(self.rt().cast())
    }

    pub fn context_opaque(&self) -> *mut c_void {
        unsafe { JS_GetContextOpaque(self.ctx) }
    }

    pub fn caller_handle(&mut self) -> ContextHandle {
        ContextHandle(unsafe { JS_GetCallerRealm(self.ctx) }.cast())
    }

    pub fn function_handle(&mut self, function: &Value) -> Result<ContextHandle, Value> {
        let realm = unsafe { JS_GetFunctionRealm(self.ctx, function.raw) };
        if realm.is_null() {
            Err(self.exception())
        } else {
            Ok(ContextHandle(realm.cast()))
        }
    }

    pub fn is_running_script(&mut self) -> bool {
        unsafe { JS_IsRunningScript(self.ctx) }
    }
}
