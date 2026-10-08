//! Southstar — the QuickJS backend of the JavaScript layer, over the in-tree fork's C API (or Bellard's QuickJS through ns_quickjs.h).
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#[cfg(not(target_pointer_width = "64"))]
compile_error!("the QuickJS backend lays out JSValue for 64-bit targets only");

use core::any::Any;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::marker::PhantomData;
use core::{mem, ptr};
use std::cell::RefCell;
use std::ffi::CString;
use std::path::Path;

use crate::{Attributes, BoundFn, NativeFn, PromiseState, RealmInit, TypedArrayBytes};

pub const ENGINE_NAME: &str = "quickjs-ng";

#[repr(C)]
pub struct JSRuntime {
    _private: [u8; 0],
}

#[repr(C)]
pub struct JSContext {
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
pub struct JSValue {
    u: JSValueUnion,
    tag: i64,
}

#[repr(C)]
struct JSClassDef {
    class_name: *const c_char,
    finalizer: Option<unsafe extern "C" fn(rt: *mut JSRuntime, val: JSValue)>,
    gc_mark: *const c_void,
    call: *const c_void,
    exotic: *const c_void,
}

type JSAtom = u32;

const TAG_STRING: i64 = -7;
const TAG_OBJECT: i64 = -1;
const TAG_INT: i64 = 0;
const TAG_BOOL: i64 = 1;
const TAG_NULL: i64 = 2;
const TAG_UNDEFINED: i64 = 3;
const TAG_EXCEPTION: i64 = 6;
const TAG_FLOAT64: i64 = 8;
const EVAL_TYPE_GLOBAL: c_int = 0;
const EVAL_TYPE_MODULE: c_int = 1;
const EVAL_FLAG_COMPILE_ONLY: c_int = 1 << 5;
const PROMISE_PENDING: c_int = 0;
const PROMISE_FULFILLED: c_int = 1;
const PROMISE_REJECTED: c_int = 2;
const PROP_CONFIGURABLE: c_int = 1 << 0;
const PROP_WRITABLE: c_int = 1 << 1;
const PROP_ENUMERABLE: c_int = 1 << 2;
const ATOM_NULL: JSAtom = 0;
const HOST_CLASS_ID: u32 = 512;

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
    fn JS_Throw(ctx: *mut JSContext, obj: JSValue) -> JSValue;
    fn JS_ThrowTypeError(ctx: *mut JSContext, fmt: *const c_char, ...) -> JSValue;
    fn JS_ThrowRangeError(ctx: *mut JSContext, fmt: *const c_char, ...) -> JSValue;
    fn JS_ToCStringLen2(
        ctx: *mut JSContext,
        plen: *mut usize,
        val: JSValue,
        cesu8: c_int,
    ) -> *const c_char;
    fn JS_FreeCString(ctx: *mut JSContext, ptr: *const c_char);
    fn JS_NewStringLen(ctx: *mut JSContext, str1: *const c_char, len1: usize) -> JSValue;
    fn JS_NewObject(ctx: *mut JSContext) -> JSValue;
    fn JS_GetGlobalObject(ctx: *mut JSContext) -> JSValue;
    fn JS_GetPropertyStr(ctx: *mut JSContext, this_obj: JSValue, prop: *const c_char) -> JSValue;
    fn JS_GetPropertyUint32(ctx: *mut JSContext, this_obj: JSValue, idx: u32) -> JSValue;
    fn JS_ParseJSON(
        ctx: *mut JSContext,
        buf: *const c_char,
        buf_len: usize,
        filename: *const c_char,
    ) -> JSValue;
    fn JS_ToBool(ctx: *mut JSContext, val: JSValue) -> c_int;
    fn JS_ToFloat64(ctx: *mut JSContext, pres: *mut f64, val: JSValue) -> c_int;
    fn JS_NewArray(ctx: *mut JSContext) -> JSValue;
    fn JS_NewObjectProto(ctx: *mut JSContext, proto: JSValue) -> JSValue;
    fn JS_SetPropertyUint32(
        ctx: *mut JSContext,
        this_obj: JSValue,
        idx: u32,
        val: JSValue,
    ) -> c_int;
    fn JS_CallConstructor(
        ctx: *mut JSContext,
        func_obj: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn JS_SetPropertyStr(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: *const c_char,
        val: JSValue,
    ) -> c_int;
    fn JS_DefinePropertyValueStr(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: *const c_char,
        val: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_DefinePropertyValue(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: JSAtom,
        val: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_HasProperty(ctx: *mut JSContext, this_obj: JSValue, prop: JSAtom) -> c_int;
    fn JS_NewAtom(ctx: *mut JSContext, str: *const c_char) -> JSAtom;
    fn JS_FreeAtom(ctx: *mut JSContext, v: JSAtom);
    fn JS_ValueToAtom(ctx: *mut JSContext, val: JSValue) -> JSAtom;
    fn JS_NewCFunctionData(
        ctx: *mut JSContext,
        func: JSCFunctionData,
        length: c_int,
        magic: c_int,
        data_len: c_int,
        data: *mut JSValue,
    ) -> JSValue;
    fn JS_SetConstructorBit(ctx: *mut JSContext, func_obj: JSValue, val: c_int) -> bool;
    fn JS_SetConstructor(ctx: *mut JSContext, func_obj: JSValue, proto: JSValue) -> c_int;
    fn JS_Call(
        ctx: *mut JSContext,
        func_obj: JSValue,
        this_obj: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn JS_ToInt32(ctx: *mut JSContext, pres: *mut i32, val: JSValue) -> c_int;
    fn JS_ToInt64(ctx: *mut JSContext, pres: *mut i64, val: JSValue) -> c_int;
    fn JS_ToBigInt64(ctx: *mut JSContext, pres: *mut i64, val: JSValue) -> c_int;
    fn JS_NewBigInt64(ctx: *mut JSContext, v: i64) -> JSValue;
    fn JS_NewClass(rt: *mut JSRuntime, class_id: u32, class_def: *const JSClassDef) -> c_int;
    fn JS_IsRegisteredClass(rt: *mut JSRuntime, class_id: u32) -> bool;
    fn JS_NewObjectClass(ctx: *mut JSContext, class_id: c_int) -> JSValue;
    fn JS_NewObjectProtoClass(ctx: *mut JSContext, proto: JSValue, class_id: u32) -> JSValue;
    fn JS_GetOpaque(obj: JSValue, class_id: u32) -> *mut c_void;
    fn JS_SetOpaque(obj: JSValue, opaque: *mut c_void) -> c_int;
    fn JS_DetachArrayBuffer(ctx: *mut JSContext, obj: JSValue);
    fn JS_GetArrayBuffer(ctx: *mut JSContext, psize: *mut usize, obj: JSValue) -> *mut u8;
    fn JS_GetTypedArrayBuffer(
        ctx: *mut JSContext,
        obj: JSValue,
        pbyte_offset: *mut usize,
        pbyte_length: *mut usize,
        pbytes_per_element: *mut usize,
    ) -> JSValue;
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

#[cfg(not(feature = "quickjs-original"))]
unsafe extern "C" {
    fn JS_FreeValue(ctx: *mut JSContext, v: JSValue);
    fn JS_DupValue(ctx: *mut JSContext, v: JSValue) -> JSValue;
    fn JS_IsArray(val: JSValue) -> bool;
}

#[cfg(feature = "quickjs-original")]
unsafe extern "C" {
    #[link_name = "ns_quickjs_is_array"]
    fn JS_IsArray(val: JSValue) -> bool;
}

#[cfg(feature = "quickjs-original")]
unsafe extern "C" {
    fn __JS_FreeValue(ctx: *mut JSContext, v: JSValue);
}

#[cfg(feature = "quickjs-original")]
unsafe fn ref_count(v: JSValue) -> *mut c_int {
    unsafe { v.u.ptr.cast::<c_int>().sub(1) }
}

#[cfg(feature = "quickjs-original")]
#[allow(non_snake_case)]
unsafe fn JS_FreeValue(ctx: *mut JSContext, v: JSValue) {
    if v.tag < 0 {
        unsafe {
            let count = ref_count(v);
            *count -= 1;
            if *count <= 0 {
                __JS_FreeValue(ctx, v);
            }
        }
    }
}

#[cfg(feature = "quickjs-original")]
#[allow(non_snake_case)]
unsafe fn JS_DupValue(_ctx: *mut JSContext, v: JSValue) -> JSValue {
    if v.tag < 0 {
        unsafe { *ref_count(v) += 1 };
    }
    v
}

const fn mkval(tag: i64, int32: i32) -> JSValue {
    JSValue {
        u: JSValueUnion { int32 },
        tag,
    }
}

const UNDEFINED: JSValue = mkval(TAG_UNDEFINED, 0);
const NULL: JSValue = mkval(TAG_NULL, 0);

struct HostData(Box<dyn Any>);

thread_local! {
    static NATIVES: RefCell<Vec<NativeFn>> = const { RefCell::new(Vec::new()) };
    static BOUND: RefCell<Vec<(BoundFn, usize)>> = const { RefCell::new(Vec::new()) };
    static REALMS: RefCell<Vec<(*mut JSRuntime, *mut JSContext)>> = const { RefCell::new(Vec::new()) };
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

fn bound_index(f: BoundFn, captured: usize) -> c_int {
    BOUND.with(|bound| {
        let mut bound = bound.borrow_mut();
        let index = bound
            .iter()
            .position(|&(known, count)| ptr::fn_addr_eq(known, f) && count == captured)
            .unwrap_or_else(|| {
                bound.push((f, captured));
                bound.len() - 1
            });
        index as c_int
    })
}

unsafe extern "C" fn finalize_host(_rt: *mut JSRuntime, val: JSValue) {
    let data = unsafe { JS_GetOpaque(val, HOST_CLASS_ID) };
    if !data.is_null() {
        drop(unsafe { Box::from_raw(data.cast::<HostData>()) });
    }
}

fn register_host_class(rt: *mut JSRuntime) {
    unsafe {
        if !JS_IsRegisteredClass(rt, HOST_CLASS_ID) {
            let def = JSClassDef {
                class_name: c"HostObject".as_ptr(),
                finalizer: Some(finalize_host),
                gc_mark: ptr::null(),
                call: ptr::null(),
                exotic: ptr::null(),
            };
            JS_NewClass(rt, HOST_CLASS_ID, &def);
        }
    }
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

    pub fn null() -> Value {
        Value::own(ptr::null_mut(), NULL)
    }

    pub fn int(number: i32) -> Value {
        Value::own(ptr::null_mut(), mkval(TAG_INT, number))
    }

    pub fn number(number: f64) -> Value {
        let raw = JSValue {
            u: JSValueUnion { float64: number },
            tag: TAG_FLOAT64,
        };
        Value::own(ptr::null_mut(), raw)
    }

    pub fn int64(number: i64) -> Value {
        match i32::try_from(number) {
            Ok(small) => Value::int(small),
            Err(_) => Value::number(number as f64),
        }
    }

    pub fn boolean(value: bool) -> Value {
        Value::own(ptr::null_mut(), mkval(TAG_BOOL, value as i32))
    }

    pub fn is_undefined(&self) -> bool {
        self.raw.tag == TAG_UNDEFINED
    }

    pub fn is_null(&self) -> bool {
        self.raw.tag == TAG_NULL
    }

    pub fn is_object(&self) -> bool {
        self.raw.tag == TAG_OBJECT
    }

    pub fn is_string(&self) -> bool {
        self.raw.tag == TAG_STRING
    }

    pub fn is_array(&self) -> bool {
        unsafe { JS_IsArray(self.raw) }
    }

    pub fn is_number(&self) -> bool {
        self.raw.tag == TAG_INT || self.raw.tag == TAG_FLOAT64
    }

    pub fn is_bool(&self) -> bool {
        self.raw.tag == TAG_BOOL
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

fn prop_flags(attributes: Attributes) -> c_int {
    (if attributes.writable {
        PROP_WRITABLE
    } else {
        0
    }) | (if attributes.enumerable {
        PROP_ENUMERABLE
    } else {
        0
    }) | (if attributes.configurable {
        PROP_CONFIGURABLE
    } else {
        0
    })
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

unsafe extern "C" fn call_bound(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    magic: c_int,
    func_data: *mut JSValue,
) -> JSValue {
    let Some((f, captured)) = BOUND.with(|bound| bound.borrow().get(magic as usize).copied())
    else {
        return UNDEFINED;
    };
    let owned = |raw: &[JSValue]| -> Vec<Value> {
        raw.iter()
            .map(|&value| Value::own(ctx, unsafe { JS_DupValue(ctx, value) }))
            .collect()
    };
    let args = if argv.is_null() || argc <= 0 {
        Vec::new()
    } else {
        owned(unsafe { core::slice::from_raw_parts(argv, argc as usize) })
    };
    let data = if func_data.is_null() || captured == 0 {
        Vec::new()
    } else {
        owned(unsafe { core::slice::from_raw_parts(func_data, captured) })
    };
    let this = Value::own(ctx, unsafe { JS_DupValue(ctx, this_val) });
    let mut scope = Scope::of(ctx);
    match f(&mut scope, &this, &args, &data) {
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
    format!("{ENGINE_NAME} {}", version.to_string_lossy())
}

pub struct Engine {
    rt: *mut JSRuntime,
    main: *mut JSContext,
}

impl Engine {
    pub fn new(_module_root: &Path) -> Engine {
        unsafe {
            let rt = JS_NewRuntime();
            JS_SetMaxStackSize(rt, 4 * 1024 * 1024);
            JS_SetModuleLoaderFunc(rt, None, Some(load_module), ptr::null_mut());
            let main = JS_NewContext(rt);
            Engine { rt, main }
        }
    }

    pub fn set_max_stack_size(&mut self, bytes: usize) {
        unsafe { JS_SetMaxStackSize(self.rt, bytes) };
    }

    pub fn enter<R>(&mut self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        f(&mut Scope::of(self.main))
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let realms: Vec<*mut JSContext> = REALMS.with(|realms| {
            let mut realms = realms.borrow_mut();
            let (mine, others): (Vec<_>, Vec<_>) =
                realms.drain(..).partition(|&(rt, _)| rt == self.rt);
            *realms = others;
            mine.into_iter().map(|(_, ctx)| ctx).collect()
        });
        unsafe {
            for realm in realms {
                JS_FreeContext(realm);
            }
            JS_FreeContext(self.main);
            JS_RunGC(self.rt);
            JS_FreeRuntime(self.rt);
        }
    }
}

pub mod quickjs {
    pub use super::{JSContext, JSValue};
    use super::{Scope, Value};

    pub unsafe fn with_context<R>(ctx: *mut JSContext, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        f(&mut Scope::of(ctx))
    }

    pub unsafe fn borrow_value(scope: &Scope<'_>, raw: JSValue) -> Value {
        Value::own(scope.ctx, unsafe { super::JS_DupValue(scope.ctx, raw) })
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
            Err(self.exception())
        } else {
            Ok(Value::own(self.ctx, raw))
        }
    }

    fn exception(&self) -> Value {
        Value::own(self.ctx, unsafe { JS_GetException(self.ctx) })
    }

    fn status(&self, status: c_int) -> Result<(), Value> {
        if status < 0 {
            Err(self.exception())
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

    pub fn new_array(&mut self) -> Value {
        Value::own(self.ctx, unsafe { JS_NewArray(self.ctx) })
    }

    pub fn new_object_with_proto(&mut self, prototype: &Value) -> Value {
        Value::own(self.ctx, unsafe {
            JS_NewObjectProto(self.ctx, prototype.raw)
        })
    }

    pub fn string_from_bytes(&mut self, bytes: &[u8]) -> Value {
        Value::own(self.ctx, unsafe {
            JS_NewStringLen(self.ctx, bytes.as_ptr().cast(), bytes.len())
        })
    }

    pub fn parse_json(&mut self, text: &[u8], name: &str) -> Result<Value, Value> {
        let mut input = Vec::with_capacity(text.len() + 1);
        input.extend_from_slice(text);
        input.push(0);
        let name = c_text(name);
        let raw =
            unsafe { JS_ParseJSON(self.ctx, input.as_ptr().cast(), text.len(), name.as_ptr()) };
        self.take(raw)
    }

    pub fn bigint64(&mut self, number: i64) -> Value {
        Value::own(self.ctx, unsafe { JS_NewBigInt64(self.ctx, number) })
    }

    fn native_function(&mut self, name: &str, arity: u32, f: NativeFn, constructor: bool) -> Value {
        let raw = unsafe {
            JS_NewCFunctionData(
                self.ctx,
                call_native,
                arity as c_int,
                native_index(f),
                0,
                ptr::null_mut(),
            )
        };
        let function = Value::own(self.ctx, raw);
        if constructor {
            unsafe { JS_SetConstructorBit(self.ctx, function.raw, 1) };
        }
        let name = self.string(name);
        let _ = self.define(&function, "name", name, Attributes::CONFIGURABLE);
        function
    }

    pub fn function(&mut self, name: &str, arity: u32, f: NativeFn) -> Value {
        self.native_function(name, arity, f, false)
    }

    pub fn constructor(&mut self, name: &str, arity: u32, f: NativeFn) -> Value {
        self.native_function(name, arity, f, true)
    }

    pub fn bound_function(&mut self, name: &str, arity: u32, f: BoundFn, data: &[Value]) -> Value {
        let mut raw_data: Vec<JSValue> = data.iter().map(|value| value.raw).collect();
        let raw = unsafe {
            JS_NewCFunctionData(
                self.ctx,
                call_bound,
                arity as c_int,
                bound_index(f, data.len()),
                data.len() as c_int,
                raw_data.as_mut_ptr(),
            )
        };
        let function = Value::own(self.ctx, raw);
        let name = self.string(name);
        let _ = self.define(&function, "name", name, Attributes::CONFIGURABLE);
        function
    }

    pub fn set_constructor(&mut self, function: &Value, prototype: &Value) -> Result<(), Value> {
        let status = unsafe { JS_SetConstructor(self.ctx, function.raw, prototype.raw) };
        self.status(status)
    }

    pub fn get(&mut self, object: &Value, key: &str) -> Result<Value, Value> {
        let key = c_text(key);
        let raw = unsafe { JS_GetPropertyStr(self.ctx, object.raw, key.as_ptr()) };
        self.take(raw)
    }

    pub fn get_index(&mut self, object: &Value, index: u32) -> Result<Value, Value> {
        let raw = unsafe { JS_GetPropertyUint32(self.ctx, object.raw, index) };
        self.take(raw)
    }

    pub fn set_index(&mut self, object: &Value, index: u32, value: Value) -> Result<(), Value> {
        let raw = value.into_raw();
        let status = unsafe { JS_SetPropertyUint32(self.ctx, object.raw, index, raw) };
        self.status(status)
    }

    pub fn set(&mut self, object: &Value, key: &str, value: Value) -> Result<(), Value> {
        let key = c_text(key);
        let raw = value.into_raw();
        let status = unsafe { JS_SetPropertyStr(self.ctx, object.raw, key.as_ptr(), raw) };
        self.status(status)
    }

    pub fn define(
        &mut self,
        object: &Value,
        key: &str,
        value: Value,
        attributes: Attributes,
    ) -> Result<(), Value> {
        let key = c_text(key);
        let raw = value.into_raw();
        let status = unsafe {
            JS_DefinePropertyValueStr(
                self.ctx,
                object.raw,
                key.as_ptr(),
                raw,
                prop_flags(attributes),
            )
        };
        self.status(status)
    }

    pub fn define_to_string_tag(&mut self, object: &Value, tag: &str) -> Result<(), Value> {
        let global = self.global();
        let symbol_constructor = self.get(&global, "Symbol")?;
        let symbol = self.get(&symbol_constructor, "toStringTag")?;
        let atom = unsafe { JS_ValueToAtom(self.ctx, symbol.raw) };
        if atom == ATOM_NULL {
            return Err(self.exception());
        }
        let value = self.string(tag).into_raw();
        let status =
            unsafe { JS_DefinePropertyValue(self.ctx, object.raw, atom, value, PROP_CONFIGURABLE) };
        unsafe { JS_FreeAtom(self.ctx, atom) };
        self.status(status)
    }

    pub fn has_property(&mut self, object: &Value, key: &str) -> Result<bool, Value> {
        let key = c_text(key);
        let atom = unsafe { JS_NewAtom(self.ctx, key.as_ptr()) };
        let status = unsafe { JS_HasProperty(self.ctx, object.raw, atom) };
        unsafe { JS_FreeAtom(self.ctx, atom) };
        self.status(status).map(|()| status > 0)
    }

    pub fn call(&mut self, function: &Value, this: &Value, args: &[Value]) -> Result<Value, Value> {
        let mut raw_args: Vec<JSValue> = args.iter().map(|arg| arg.raw).collect();
        let raw = unsafe {
            JS_Call(
                self.ctx,
                function.raw,
                this.raw,
                raw_args.len() as c_int,
                raw_args.as_mut_ptr(),
            )
        };
        self.take(raw)
    }

    pub fn construct(&mut self, constructor: &Value, args: &[Value]) -> Result<Value, Value> {
        let mut raw_args: Vec<JSValue> = args.iter().map(|arg| arg.raw).collect();
        let raw = unsafe {
            JS_CallConstructor(
                self.ctx,
                constructor.raw,
                raw_args.len() as c_int,
                raw_args.as_mut_ptr(),
            )
        };
        self.take(raw)
    }

    pub fn to_number(&mut self, value: &Value) -> Result<f64, Value> {
        let mut out = 0f64;
        let status = unsafe { JS_ToFloat64(self.ctx, &mut out, value.raw) };
        self.status(status).map(|()| out)
    }

    pub fn to_string(&mut self, value: &Value) -> Result<String, Value> {
        let mut len = 0usize;
        let text = unsafe { JS_ToCStringLen2(self.ctx, &mut len, value.raw, 0) };
        if text.is_null() {
            return Err(self.exception());
        }
        let bytes = unsafe { core::slice::from_raw_parts(text.cast::<u8>(), len) };
        let owned = String::from_utf8_lossy(bytes).into_owned();
        unsafe { JS_FreeCString(self.ctx, text) };
        Ok(owned)
    }

    pub fn to_bytes(&mut self, value: &Value) -> Result<Vec<u8>, Value> {
        let mut len = 0usize;
        let text = unsafe { JS_ToCStringLen2(self.ctx, &mut len, value.raw, 0) };
        if text.is_null() {
            return Err(self.exception());
        }
        let bytes = unsafe { core::slice::from_raw_parts(text.cast::<u8>(), len) }.to_vec();
        unsafe { JS_FreeCString(self.ctx, text) };
        Ok(bytes)
    }

    pub fn to_bool(&mut self, value: &Value) -> bool {
        unsafe { JS_ToBool(self.ctx, value.raw) > 0 }
    }

    pub fn to_int32(&mut self, value: &Value) -> Result<i32, Value> {
        let mut out = 0i32;
        let status = unsafe { JS_ToInt32(self.ctx, &mut out, value.raw) };
        self.status(status).map(|()| out)
    }

    pub fn to_int64(&mut self, value: &Value) -> Result<i64, Value> {
        let mut out = 0i64;
        let status = unsafe { JS_ToInt64(self.ctx, &mut out, value.raw) };
        self.status(status).map(|()| out)
    }

    pub fn to_bigint64(&mut self, value: &Value) -> Result<i64, Value> {
        let mut out = 0i64;
        let status = unsafe { JS_ToBigInt64(self.ctx, &mut out, value.raw) };
        self.status(status).map(|()| out)
    }

    pub fn type_error(&mut self, message: &str) -> Value {
        let message = c_text(message);
        unsafe { JS_ThrowTypeError(self.ctx, c"%s".as_ptr(), message.as_ptr()) };
        self.exception()
    }

    pub fn range_error(&mut self, message: &str) -> Value {
        let message = c_text(message);
        unsafe { JS_ThrowRangeError(self.ctx, c"%s".as_ptr(), message.as_ptr()) };
        self.exception()
    }

    pub fn new_host_object<T: Any>(&mut self, prototype: Option<&Value>, data: T) -> Value {
        register_host_class(self.rt());
        let raw = unsafe {
            match prototype {
                Some(prototype) => JS_NewObjectProtoClass(self.ctx, prototype.raw, HOST_CLASS_ID),
                None => JS_NewObjectClass(self.ctx, HOST_CLASS_ID as c_int),
            }
        };
        if raw.tag == TAG_OBJECT {
            let boxed = Box::into_raw(Box::new(HostData(Box::new(data))));
            unsafe { JS_SetOpaque(raw, boxed.cast()) };
        }
        Value::own(self.ctx, raw)
    }

    pub fn host_data<T: Any + Clone>(&mut self, value: &Value) -> Option<T> {
        if value.raw.tag != TAG_OBJECT {
            return None;
        }
        let data = unsafe { JS_GetOpaque(value.raw, HOST_CLASS_ID) }.cast::<HostData>();
        unsafe { data.as_ref() }.and_then(|data| data.0.downcast_ref::<T>().cloned())
    }

    pub fn detach_array_buffer(&mut self, value: &Value) -> Result<(), Value> {
        unsafe { JS_DetachArrayBuffer(self.ctx, value.raw) };
        Ok(())
    }

    pub fn with_typed_array<R>(
        &mut self,
        value: &Value,
        f: impl FnOnce(TypedArrayBytes<'_>) -> R,
    ) -> Option<R> {
        let (mut byte_offset, mut length, mut element_size) = (0usize, 0usize, 0usize);
        let buffer = unsafe {
            JS_GetTypedArrayBuffer(
                self.ctx,
                value.raw,
                &mut byte_offset,
                &mut length,
                &mut element_size,
            )
        };
        if buffer.tag == TAG_EXCEPTION {
            drop(self.exception());
            return None;
        }
        let mut total = 0usize;
        let base = unsafe { JS_GetArrayBuffer(self.ctx, &mut total, buffer) };
        unsafe { JS_FreeValue(self.ctx, buffer) };
        if base.is_null()
            || byte_offset
                .checked_add(length)
                .is_none_or(|end| end > total)
        {
            return None;
        }
        let bytes = unsafe { core::slice::from_raw_parts(base.add(byte_offset), length) };
        Some(f(TypedArrayBytes {
            bytes,
            byte_offset,
            element_size,
        }))
    }

    pub fn gc(&mut self) {
        unsafe { JS_RunGC(self.rt()) };
    }

    pub fn new_realm(&mut self, init: RealmInit) -> Result<Value, Value> {
        let rt = self.rt();
        let ctx = unsafe { JS_NewContext(rt) };
        REALMS.with(|realms| realms.borrow_mut().push((rt, ctx)));
        let mut realm = Scope::of(ctx);
        init(&mut realm)?;
        Ok(realm.global())
    }
}
