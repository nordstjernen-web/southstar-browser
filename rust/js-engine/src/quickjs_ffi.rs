//! Southstar — the QuickJS-ng backend of the JavaScript layer, over the in-tree fork's C API in src/quickjs/quickjs.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(not(target_pointer_width = "64"))]
compile_error!("the QuickJS-ng backend lays out JSValue for 64-bit targets only");

use core::ffi::{CStr, c_char, c_int, c_void};
use core::marker::PhantomData;
use core::{mem, ptr};
use std::cell::RefCell;
use std::ffi::CString;
use std::path::Path;

use crate::{NativeFn, PromiseState, RealmInit};

pub const ENGINE_NAME: &str = "quickjs-ng";

#[repr(C)]
struct JSRuntime {
    _private: [u8; 0],
}

#[repr(C)]
struct JSContext {
    _private: [u8; 0],
}

#[repr(C)]
struct JSModuleDef {
    _private: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
union JSValueUnion {
    int32: i32,
    float64: f64,
    ptr: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct JSValue {
    u: JSValueUnion,
    tag: i64,
}

const TAG_UNDEFINED: i64 = 3;
const TAG_EXCEPTION: i64 = 6;
const EVAL_TYPE_GLOBAL: c_int = 0;
const EVAL_TYPE_MODULE: c_int = 1;
const EVAL_FLAG_COMPILE_ONLY: c_int = 1 << 5;
const PROMISE_PENDING: c_int = 0;
const PROMISE_FULFILLED: c_int = 1;
const PROMISE_REJECTED: c_int = 2;

type JSCFunctionData = unsafe extern "C" fn(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    magic: c_int,
    func_data: *mut JSValue,
) -> JSValue;

type JSModuleLoaderFunc = unsafe extern "C" fn(
    ctx: *mut JSContext,
    module_name: *const c_char,
    opaque: *mut c_void,
) -> *mut JSModuleDef;

type JSModuleNormalizeFunc = unsafe extern "C" fn(
    ctx: *mut JSContext,
    module_base_name: *const c_char,
    module_name: *const c_char,
    opaque: *mut c_void,
) -> *mut c_char;

unsafe extern "C" {
    fn JS_NewRuntime() -> *mut JSRuntime;
    fn JS_FreeRuntime(rt: *mut JSRuntime);
    fn JS_SetRuntimeOpaque(rt: *mut JSRuntime, opaque: *mut c_void);
    fn JS_GetRuntimeOpaque(rt: *mut JSRuntime) -> *mut c_void;
    fn JS_SetMaxStackSize(rt: *mut JSRuntime, stack_size: usize);
    fn JS_RunGC(rt: *mut JSRuntime);
    fn JS_NewContext(rt: *mut JSRuntime) -> *mut JSContext;
    fn JS_FreeContext(ctx: *mut JSContext);
    fn JS_GetRuntime(ctx: *mut JSContext) -> *mut JSRuntime;
    fn JS_Eval(
        ctx: *mut JSContext,
        input: *const c_char,
        input_len: usize,
        filename: *const c_char,
        eval_flags: c_int,
    ) -> JSValue;
    fn JS_GetException(ctx: *mut JSContext) -> JSValue;
    fn JS_HasException(ctx: *mut JSContext) -> bool;
    fn JS_Throw(ctx: *mut JSContext, obj: JSValue) -> JSValue;
    fn JS_ThrowTypeError(ctx: *mut JSContext, fmt: *const c_char, ...) -> JSValue;
    fn JS_FreeValue(ctx: *mut JSContext, v: JSValue);
    fn JS_DupValue(ctx: *mut JSContext, v: JSValue) -> JSValue;
    fn JS_ToCStringLen2(
        ctx: *mut JSContext,
        plen: *mut usize,
        val: JSValue,
        cesu8: bool,
    ) -> *const c_char;
    fn JS_FreeCString(ctx: *mut JSContext, ptr: *const c_char);
    fn JS_NewStringLen(ctx: *mut JSContext, str1: *const c_char, len1: usize) -> JSValue;
    fn JS_NewObject(ctx: *mut JSContext) -> JSValue;
    fn JS_GetGlobalObject(ctx: *mut JSContext) -> JSValue;
    fn JS_GetPropertyStr(ctx: *mut JSContext, this_obj: JSValue, prop: *const c_char) -> JSValue;
    fn JS_SetPropertyStr(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: *const c_char,
        val: JSValue,
    ) -> c_int;
    fn JS_NewCFunctionData2(
        ctx: *mut JSContext,
        func: JSCFunctionData,
        name: *const c_char,
        length: c_int,
        magic: c_int,
        data_len: c_int,
        data: *mut JSValue,
    ) -> JSValue;
    fn JS_DetachArrayBuffer(ctx: *mut JSContext, obj: JSValue);
    fn JS_ExecutePendingJob(rt: *mut JSRuntime, pctx: *mut *mut JSContext) -> c_int;
    fn JS_PromiseState(ctx: *mut JSContext, promise: JSValue) -> c_int;
    fn JS_PromiseResult(ctx: *mut JSContext, promise: JSValue) -> JSValue;
    fn JS_SetModuleLoaderFunc(
        rt: *mut JSRuntime,
        module_normalize: Option<JSModuleNormalizeFunc>,
        module_loader: Option<JSModuleLoaderFunc>,
        opaque: *mut c_void,
    );
    fn JS_GetVersion() -> *const c_char;
}

const UNDEFINED: JSValue = JSValue {
    u: JSValueUnion { int32: 0 },
    tag: TAG_UNDEFINED,
};

thread_local! {
    static NATIVES: RefCell<Vec<NativeFn>> = const { RefCell::new(Vec::new()) };
}

fn native_index(f: NativeFn) -> c_int {
    NATIVES.with(|natives| {
        let mut natives = natives.borrow_mut();
        let index = natives
            .iter()
            .position(|&known| ptr::fn_addr_eq(known, f))
            .unwrap_or_else(|| {
                natives.push(f);
                natives.len() - 1
            });
        index as c_int
    })
}

struct RuntimeState {
    realms: Vec<*mut JSContext>,
}

pub struct Value {
    ctx: *mut JSContext,
    raw: JSValue,
}

impl Value {
    fn own(ctx: *mut JSContext, raw: JSValue) -> Value {
        Value { ctx, raw }
    }

    fn into_raw(self) -> JSValue {
        let raw = self.raw;
        mem::forget(self);
        raw
    }

    pub fn undefined() -> Value {
        Value::own(ptr::null_mut(), UNDEFINED)
    }

    pub fn is_undefined(&self) -> bool {
        self.raw.tag == TAG_UNDEFINED
    }
}

impl Clone for Value {
    fn clone(&self) -> Value {
        if self.raw.tag < 0 && !self.ctx.is_null() {
            Value::own(self.ctx, unsafe { JS_DupValue(self.ctx, self.raw) })
        } else {
            Value::own(self.ctx, self.raw)
        }
    }
}

impl Drop for Value {
    fn drop(&mut self) {
        if self.raw.tag < 0 && !self.ctx.is_null() {
            unsafe { JS_FreeValue(self.ctx, self.raw) };
        }
    }
}

fn c_text(text: &str) -> CString {
    CString::new(text.replace('\0', "\u{fffd}")).unwrap_or_default()
}

fn nul_terminated(source: &str) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(source.len() + 1);
    bytes.extend_from_slice(source.as_bytes());
    bytes.push(0);
    bytes
}

unsafe extern "C" fn call_native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    magic: c_int,
    _func_data: *mut JSValue,
) -> JSValue {
    let Some(f) = NATIVES.with(|natives| natives.borrow().get(magic as usize).copied()) else {
        return UNDEFINED;
    };
    let raw_args = if argv.is_null() || argc <= 0 {
        &[][..]
    } else {
        unsafe { core::slice::from_raw_parts(argv, argc as usize) }
    };
    let args: Vec<Value> = raw_args
        .iter()
        .map(|&raw| Value::own(ctx, unsafe { JS_DupValue(ctx, raw) }))
        .collect();
    let this = Value::own(ctx, unsafe { JS_DupValue(ctx, this_val) });
    let mut scope = Scope::of(ctx);
    match f(&mut scope, &this, &args) {
        Ok(value) => value.into_raw(),
        Err(error) => unsafe { JS_Throw(ctx, error.into_raw()) },
    }
}

unsafe extern "C" fn load_module(
    ctx: *mut JSContext,
    module_name: *const c_char,
    _opaque: *mut c_void,
) -> *mut JSModuleDef {
    let name = unsafe { CStr::from_ptr(module_name) };
    let Ok(source) = std::fs::read_to_string(name.to_string_lossy().as_ref()) else {
        unsafe { JS_ThrowTypeError(ctx, c"could not load module '%s'".as_ptr(), module_name) };
        return ptr::null_mut();
    };
    let input = nul_terminated(&source);
    let compiled = unsafe {
        JS_Eval(
            ctx,
            input.as_ptr().cast(),
            source.len(),
            module_name,
            EVAL_TYPE_MODULE | EVAL_FLAG_COMPILE_ONLY,
        )
    };
    if compiled.tag == TAG_EXCEPTION {
        return ptr::null_mut();
    }
    let module = unsafe { compiled.u.ptr }.cast::<JSModuleDef>();
    unsafe { JS_FreeValue(ctx, compiled) };
    module
}

pub fn engine_version() -> String {
    let version = unsafe { CStr::from_ptr(JS_GetVersion()) };
    format!("quickjs-ng {}", version.to_string_lossy())
}

pub struct Engine {
    rt: *mut JSRuntime,
    main: *mut JSContext,
}

impl Engine {
    pub fn new(_module_root: &Path) -> Engine {
        unsafe {
            let rt = JS_NewRuntime();
            let state = Box::new(RuntimeState { realms: Vec::new() });
            JS_SetRuntimeOpaque(rt, Box::into_raw(state).cast());
            JS_SetMaxStackSize(rt, 4 * 1024 * 1024);
            JS_SetModuleLoaderFunc(rt, None, Some(load_module), ptr::null_mut());
            let main = JS_NewContext(rt);
            Engine { rt, main }
        }
    }

    pub fn enter<R>(&mut self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        f(&mut Scope {
            ctx: self.main,
            engine: PhantomData,
        })
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            let state = Box::from_raw(JS_GetRuntimeOpaque(self.rt).cast::<RuntimeState>());
            for realm in state.realms {
                JS_FreeContext(realm);
            }
            JS_FreeContext(self.main);
            JS_RunGC(self.rt);
            JS_FreeRuntime(self.rt);
        }
    }
}

pub struct Scope<'a> {
    ctx: *mut JSContext,
    engine: PhantomData<&'a mut Engine>,
}

impl Scope<'_> {
    fn of(ctx: *mut JSContext) -> Scope<'static> {
        Scope {
            ctx,
            engine: PhantomData,
        }
    }

    fn rt(&self) -> *mut JSRuntime {
        unsafe { JS_GetRuntime(self.ctx) }
    }

    fn take(&self, raw: JSValue) -> Result<Value, Value> {
        if raw.tag == TAG_EXCEPTION {
            Err(Value::own(self.ctx, unsafe { JS_GetException(self.ctx) }))
        } else {
            Ok(Value::own(self.ctx, raw))
        }
    }

    fn pending_exception(&self) -> Result<(), Value> {
        if unsafe { JS_HasException(self.ctx) } {
            Err(Value::own(self.ctx, unsafe { JS_GetException(self.ctx) }))
        } else {
            Ok(())
        }
    }

    pub fn eval_script(&mut self, source: &str, name: &str) -> Result<Value, Value> {
        let input = nul_terminated(source);
        let name = c_text(name);
        let raw = unsafe {
            JS_Eval(
                self.ctx,
                input.as_ptr().cast(),
                source.len(),
                name.as_ptr(),
                EVAL_TYPE_GLOBAL,
            )
        };
        self.take(raw)
    }

    pub fn eval_module(&mut self, source: &str, path: &Path) -> Result<Value, Value> {
        let input = nul_terminated(source);
        let name = c_text(&path.to_string_lossy().replace('\\', "/"));
        let raw = unsafe {
            JS_Eval(
                self.ctx,
                input.as_ptr().cast(),
                source.len(),
                name.as_ptr(),
                EVAL_TYPE_MODULE,
            )
        };
        self.take(raw)
    }

    pub fn run_jobs(&mut self) -> Result<(), Value> {
        loop {
            let mut job_ctx: *mut JSContext = ptr::null_mut();
            let status = unsafe { JS_ExecutePendingJob(self.rt(), &mut job_ctx) };
            if status == 0 {
                return Ok(());
            }
            if status < 0 {
                let ctx = if job_ctx.is_null() { self.ctx } else { job_ctx };
                return Err(Value::own(ctx, unsafe { JS_GetException(ctx) }));
            }
        }
    }

    pub fn promise_state(&mut self, value: &Value) -> PromiseState {
        match unsafe { JS_PromiseState(self.ctx, value.raw) } {
            PROMISE_PENDING => PromiseState::Pending,
            PROMISE_FULFILLED => PromiseState::Fulfilled(Value::own(self.ctx, unsafe {
                JS_PromiseResult(self.ctx, value.raw)
            })),
            PROMISE_REJECTED => PromiseState::Rejected(Value::own(self.ctx, unsafe {
                JS_PromiseResult(self.ctx, value.raw)
            })),
            _ => PromiseState::NotAPromise,
        }
    }

    pub fn global(&mut self) -> Value {
        Value::own(self.ctx, unsafe { JS_GetGlobalObject(self.ctx) })
    }

    pub fn new_object(&mut self) -> Value {
        Value::own(self.ctx, unsafe { JS_NewObject(self.ctx) })
    }

    pub fn string(&mut self, text: &str) -> Value {
        Value::own(self.ctx, unsafe {
            JS_NewStringLen(self.ctx, text.as_ptr().cast(), text.len())
        })
    }

    pub fn function(&mut self, name: &str, arity: u32, f: NativeFn) -> Value {
        let name = c_text(name);
        Value::own(self.ctx, unsafe {
            JS_NewCFunctionData2(
                self.ctx,
                call_native,
                name.as_ptr(),
                arity as c_int,
                native_index(f),
                0,
                ptr::null_mut(),
            )
        })
    }

    pub fn get(&mut self, object: &Value, key: &str) -> Result<Value, Value> {
        let key = c_text(key);
        let raw = unsafe { JS_GetPropertyStr(self.ctx, object.raw, key.as_ptr()) };
        self.take(raw)
    }

    pub fn set(&mut self, object: &Value, key: &str, value: Value) -> Result<(), Value> {
        let key = c_text(key);
        let raw = value.into_raw();
        let status = unsafe { JS_SetPropertyStr(self.ctx, object.raw, key.as_ptr(), raw) };
        if status < 0 {
            Err(Value::own(self.ctx, unsafe { JS_GetException(self.ctx) }))
        } else {
            Ok(())
        }
    }

    pub fn to_string(&mut self, value: &Value) -> Result<String, Value> {
        let mut len = 0usize;
        let text = unsafe { JS_ToCStringLen2(self.ctx, &mut len, value.raw, false) };
        if text.is_null() {
            return Err(Value::own(self.ctx, unsafe { JS_GetException(self.ctx) }));
        }
        let bytes = unsafe { core::slice::from_raw_parts(text.cast::<u8>(), len) };
        let owned = String::from_utf8_lossy(bytes).into_owned();
        unsafe { JS_FreeCString(self.ctx, text) };
        Ok(owned)
    }

    pub fn type_error(&mut self, message: &str) -> Value {
        let message = c_text(message);
        unsafe { JS_ThrowTypeError(self.ctx, c"%s".as_ptr(), message.as_ptr()) };
        Value::own(self.ctx, unsafe { JS_GetException(self.ctx) })
    }

    pub fn detach_array_buffer(&mut self, value: &Value) -> Result<(), Value> {
        unsafe { JS_DetachArrayBuffer(self.ctx, value.raw) };
        self.pending_exception()
    }

    pub fn gc(&mut self) {
        unsafe { JS_RunGC(self.rt()) };
    }

    pub fn new_realm(&mut self, init: RealmInit) -> Result<Value, Value> {
        let ctx = unsafe { JS_NewContext(self.rt()) };
        let state = unsafe { &mut *JS_GetRuntimeOpaque(self.rt()).cast::<RuntimeState>() };
        state.realms.push(ctx);
        let mut realm = Scope::of(ctx);
        init(&mut realm)?;
        Ok(realm.global())
    }
}
