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

use crate::{
    Attributes, BoundFn, ElementType, Job, NativeFn, ObjectKind, PromiseState, PropertyDescriptor,
    RealmInit, Trace, TypedArrayBytes, TypedArrayView,
};

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

type GcMark = unsafe extern "C" fn(rt: *mut JSRuntime, val: JSValue, mark_func: *const c_void);

type JobFunc =
    unsafe extern "C" fn(ctx: *mut JSContext, argc: c_int, argv: *mut JSValue) -> JSValue;

#[repr(C)]
struct JSClassDef {
    class_name: *const c_char,
    finalizer: Option<unsafe extern "C" fn(rt: *mut JSRuntime, val: JSValue)>,
    gc_mark: Option<GcMark>,
    call: *const c_void,
    exotic: *const c_void,
}

type JSAtom = u32;

#[repr(C)]
struct JSPropertyEnum {
    is_enumerable: c_int,
    atom: JSAtom,
}

#[repr(C)]
struct JSPropertyDescriptor {
    flags: c_int,
    value: JSValue,
    getter: JSValue,
    setter: JSValue,
}

const GPN_STRING_MASK: c_int = 1 << 0;
const GPN_SYMBOL_MASK: c_int = 1 << 1;

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
const EVAL_FLAG_HIDE_SOURCE: c_int = 1 << 8;
const PROMISE_PENDING: c_int = 0;
const PROMISE_FULFILLED: c_int = 1;
const PROMISE_REJECTED: c_int = 2;
const PROP_CONFIGURABLE: c_int = 1 << 0;
const PROP_WRITABLE: c_int = 1 << 1;
const PROP_ENUMERABLE: c_int = 1 << 2;
const PROP_GETSET: c_int = 1 << 4;
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

const OBJ_REFERENCE: c_int = 1 << 3;

unsafe extern "C" {
    fn JS_IsArrayBuffer(obj: JSValue) -> bool;
    fn JS_GetTypedArrayType(obj: JSValue) -> c_int;
    fn JS_NewArrayBufferCopy(ctx: *mut JSContext, buf: *const u8, len: usize) -> JSValue;
    fn JS_NewTypedArray(
        ctx: *mut JSContext,
        argc: c_int,
        argv: *mut JSValue,
        kind: c_int,
    ) -> JSValue;
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
    fn JS_ThrowSyntaxError(ctx: *mut JSContext, fmt: *const c_char, ...) -> JSValue;
    fn JS_FreezeObject(ctx: *mut JSContext, obj: JSValue) -> c_int;
    fn JS_ThrowDOMException(
        ctx: *mut JSContext,
        name: *const c_char,
        fmt: *const c_char,
        ...
    ) -> JSValue;
    fn JS_HasException(ctx: *mut JSContext) -> bool;
    fn JS_WriteObject(
        ctx: *mut JSContext,
        psize: *mut usize,
        obj: JSValue,
        flags: c_int,
    ) -> *mut u8;
    fn JS_ReadObject(ctx: *mut JSContext, buf: *const u8, buf_len: usize, flags: c_int) -> JSValue;
    fn js_free(ctx: *mut JSContext, ptr: *mut c_void);
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
    fn JS_DefinePropertyGetSet(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: JSAtom,
        getter: JSValue,
        setter: JSValue,
        flags: c_int,
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
    fn JS_DeleteProperty(ctx: *mut JSContext, obj: JSValue, prop: JSAtom, flags: c_int) -> c_int;
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
    fn JS_NewArrayBuffer(
        ctx: *mut JSContext,
        buf: *mut u8,
        len: usize,
        max_len: usize,
        realloc_func: *const c_void,
        opaque: *mut c_void,
        is_shared: bool,
    ) -> JSValue;
    fn JS_GetArrayBuffer(ctx: *mut JSContext, psize: *mut usize, obj: JSValue) -> *mut u8;
    fn JS_GetTypedArrayBuffer(
        ctx: *mut JSContext,
        obj: JSValue,
        pbyte_offset: *mut usize,
        pbyte_length: *mut usize,
        pbytes_per_element: *mut usize,
    ) -> JSValue;
    fn JS_GetArrayBufferViewBuffer(
        ctx: *mut JSContext,
        obj: JSValue,
        pbyte_offset: *mut usize,
        pbyte_length: *mut usize,
    ) -> JSValue;
    fn JS_ExecutePendingJob(rt: *mut JSRuntime, pctx: *mut *mut JSContext) -> c_int;
    fn JS_PromiseState(ctx: *mut JSContext, promise: JSValue) -> c_int;
    fn JS_PromiseResult(ctx: *mut JSContext, promise: JSValue) -> JSValue;
    fn JS_NewPromiseCapability(ctx: *mut JSContext, resolving_funcs: *mut JSValue) -> JSValue;
    fn JS_IsFunction(ctx: *mut JSContext, val: JSValue) -> bool;
    fn JS_NewError(ctx: *mut JSContext) -> JSValue;
    fn JS_GetOwnPropertyNames(
        ctx: *mut JSContext,
        ptab: *mut *mut JSPropertyEnum,
        plen: *mut u32,
        obj: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_FreePropertyEnum(ctx: *mut JSContext, tab: *mut JSPropertyEnum, len: u32);
    fn JS_GetOwnProperty(
        ctx: *mut JSContext,
        desc: *mut JSPropertyDescriptor,
        obj: JSValue,
        prop: JSAtom,
    ) -> c_int;
    fn JS_AtomToValue(ctx: *mut JSContext, atom: JSAtom) -> JSValue;
    fn JS_GetPrototype(ctx: *mut JSContext, val: JSValue) -> JSValue;
    fn JS_SetPrototype(ctx: *mut JSContext, obj: JSValue, proto: JSValue) -> c_int;
    fn JS_MarkValue(rt: *mut JSRuntime, val: JSValue, mark_func: *const c_void);
    fn JS_JSONStringify(
        ctx: *mut JSContext,
        obj: JSValue,
        replacer: JSValue,
        space0: JSValue,
    ) -> JSValue;
    fn JS_EnqueueJob(
        ctx: *mut JSContext,
        job_func: JobFunc,
        argc: c_int,
        argv: *mut JSValue,
    ) -> c_int;
    fn JS_SetModuleLoaderFunc(
        rt: *mut JSRuntime,
        module_normalize: Option<JSModuleNormalizeFunc>,
        module_loader: Option<JSModuleLoaderFunc>,
        opaque: *mut c_void,
    );
    fn JS_GetVersion() -> *const c_char;
}

unsafe extern "C" {
    fn JS_FreeValue(ctx: *mut JSContext, v: JSValue);
    fn JS_DupValue(ctx: *mut JSContext, v: JSValue) -> JSValue;
    fn JS_FreeValueRT(rt: *mut JSRuntime, v: JSValue);
    fn JS_DupValueRT(rt: *mut JSRuntime, v: JSValue) -> JSValue;
    fn JS_IsArray(val: JSValue) -> bool;
}

const fn mkval(tag: i64, int32: i32) -> JSValue {
    JSValue {
        u: JSValueUnion { int32 },
        tag,
    }
}

const UNDEFINED: JSValue = mkval(TAG_UNDEFINED, 0);
const NULL: JSValue = mkval(TAG_NULL, 0);

type HostTrace = fn(&dyn Any, &mut dyn FnMut(&Value));

struct HostData {
    data: Box<dyn Any>,
    trace: Option<HostTrace>,
}

fn trace_as<T: Any + Trace>(data: &dyn Any, visit: &mut dyn FnMut(&Value)) {
    if let Some(data) = data.downcast_ref::<T>() {
        data.trace(visit);
    }
}

thread_local! {
    static NATIVES: RefCell<Vec<NativeFn>> = const { RefCell::new(Vec::new()) };
    static BOUND: RefCell<Vec<(BoundFn, usize)>> = const { RefCell::new(Vec::new()) };
    static REALMS: RefCell<Vec<(*mut JSRuntime, *mut JSContext)>> = const { RefCell::new(Vec::new()) };
    static JOBS: RefCell<Vec<Job>> = const { RefCell::new(Vec::new()) };
}

fn job_index(f: Job) -> i32 {
    JOBS.with(|jobs| {
        let mut jobs = jobs.borrow_mut();
        let index = jobs
            .iter()
            .position(|&known| ptr::fn_addr_eq(known, f))
            .unwrap_or_else(|| {
                jobs.push(f);
                jobs.len() - 1
            });
        index as i32
    })
}

unsafe extern "C" fn call_job(ctx: *mut JSContext, argc: c_int, argv: *mut JSValue) -> JSValue {
    if argc < 1 || argv.is_null() {
        return UNDEFINED;
    }
    unsafe { JS_Call(ctx, *argv, UNDEFINED, argc - 1, argv.add(1)) }
}

unsafe extern "C" fn run_job(ctx: *mut JSContext, argc: c_int, argv: *mut JSValue) -> JSValue {
    if argc < 1 || argv.is_null() {
        return UNDEFINED;
    }
    let index = unsafe { (*argv).u.int32 } as usize;
    if let Some(f) = JOBS.with(|jobs| jobs.borrow().get(index).copied()) {
        f(&mut Scope::of(ctx));
    }
    UNDEFINED
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

unsafe extern "C" fn mark_host(rt: *mut JSRuntime, val: JSValue, mark_func: *const c_void) {
    let data = unsafe { JS_GetOpaque(val, HOST_CLASS_ID) }.cast::<HostData>();
    let Some(host) = (unsafe { data.as_ref() }) else {
        return;
    };
    if let Some(trace) = host.trace {
        trace(&*host.data, &mut |value| unsafe {
            JS_MarkValue(rt, value.raw, mark_func)
        });
    }
}

fn register_host_class(rt: *mut JSRuntime) {
    unsafe {
        if !JS_IsRegisteredClass(rt, HOST_CLASS_ID) {
            let def = JSClassDef {
                class_name: c"HostObject".as_ptr(),
                finalizer: Some(finalize_host),
                gc_mark: Some(mark_host),
                call: ptr::null(),
                exotic: ptr::null(),
            };
            JS_NewClass(rt, HOST_CLASS_ID, &def);
        }
    }
}

pub struct Value {
    rt: *mut JSRuntime,
    raw: JSValue,
}

#[derive(Clone)]
pub struct ObjectKey(usize, Value);

impl PartialEq for ObjectKey {
    fn eq(&self, other: &ObjectKey) -> bool {
        self.0 == other.0
    }
}

impl Eq for ObjectKey {}

impl ObjectKey {
    pub fn value(&self) -> Value {
        self.1.clone()
    }
}

impl core::hash::Hash for ObjectKey {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl Value {
    fn own(ctx: *mut JSContext, raw: JSValue) -> Value {
        let rt = if raw.tag < 0 && !ctx.is_null() {
            unsafe { JS_GetRuntime(ctx) }
        } else {
            ptr::null_mut()
        };
        Value { rt, raw }
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

    pub fn is_symbol(&self) -> bool {
        quickjs::is_symbol(self)
    }

    pub fn object_key(&self) -> Option<ObjectKey> {
        self.is_object()
            .then(|| ObjectKey(quickjs::identity(self), self.clone()))
    }

    pub fn same_object(&self, other: &Value) -> bool {
        self.is_object() && other.is_object() && unsafe { self.raw.u.ptr == other.raw.u.ptr }
    }

    pub fn with_host<T: Any, R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        if self.raw.tag != TAG_OBJECT {
            return None;
        }
        let data = unsafe { JS_GetOpaque(self.raw, HOST_CLASS_ID) }.cast::<HostData>();
        unsafe { data.as_ref() }.and_then(|host| host.data.downcast_ref::<T>().map(f))
    }
}

const ELEMENT_TYPES: [ElementType; 12] = [
    ElementType::Uint8Clamped,
    ElementType::Int8,
    ElementType::Uint8,
    ElementType::Int16,
    ElementType::Uint16,
    ElementType::Int32,
    ElementType::Uint32,
    ElementType::BigInt64,
    ElementType::BigUint64,
    ElementType::Float16,
    ElementType::Float32,
    ElementType::Float64,
];

impl Clone for Value {
    fn clone(&self) -> Value {
        let raw = if self.rt.is_null() {
            self.raw
        } else {
            unsafe { JS_DupValueRT(self.rt, self.raw) }
        };
        Value { rt: self.rt, raw }
    }
}

impl Drop for Value {
    fn drop(&mut self) {
        if !self.rt.is_null() {
            unsafe { JS_FreeValueRT(self.rt, self.raw) };
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

const CFUNC_CONSTRUCTOR_OR_FUNC_MAGIC: c_int = 5;

unsafe extern "C" fn call_native_magic(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    magic: c_int,
) -> JSValue {
    unsafe { call_native(ctx, this_val, argc, argv, magic, ptr::null_mut()) }
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
    use core::ffi::{c_char, c_int, c_void};

    pub use super::{JSContext, JSValue};
    use super::{Scope, Value};

    pub unsafe fn with_context<R>(ctx: *mut JSContext, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        f(&mut Scope::of(ctx))
    }

    pub unsafe fn borrow_value(scope: &Scope<'_>, raw: JSValue) -> Value {
        Value::own(scope.ctx, unsafe { super::JS_DupValue(scope.ctx, raw) })
    }

    pub fn raw_context(scope: &Scope<'_>) -> *mut JSContext {
        scope.ctx
    }

    pub type JSCFunction = unsafe extern "C" fn(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;

    const CFUNC_GENERIC: c_int = 0;

    #[repr(C)]
    struct JSMemoryUsage {
        malloc_size: i64,
        malloc_limit: i64,
        memory_used_size: i64,
        rest: [i64; 61],
    }

    unsafe extern "C" {
        fn JS_NewCFunction2(
            ctx: *mut JSContext,
            func: JSCFunction,
            name: *const c_char,
            length: c_int,
            cproto: c_int,
            magic: c_int,
        ) -> JSValue;
        fn JS_ComputeMemoryUsage(rt: *mut super::JSRuntime, s: *mut JSMemoryUsage);
        fn JS_GetContextOpaque(ctx: *mut JSContext) -> *mut c_void;
    }

    unsafe extern "C" {
        fn JS_GetFunctionRealm(ctx: *mut JSContext, func_obj: JSValue) -> *mut JSContext;
    }

    pub struct MemoryUsage {
        pub malloc_size: i64,
        pub malloc_limit: i64,
        pub memory_used_size: i64,
    }

    pub unsafe fn take_value(scope: &Scope<'_>, raw: JSValue) -> Value {
        Value::own(scope.ctx, raw)
    }

    pub fn into_raw(value: Value) -> JSValue {
        value.into_raw()
    }

    pub fn raw(value: &Value) -> JSValue {
        value.raw
    }

    pub fn checked(scope: &mut Scope<'_>, value: Value) -> Result<Value, Value> {
        if value.raw.tag == super::TAG_EXCEPTION {
            core::mem::forget(value);
            Err(scope.exception())
        } else {
            Ok(value)
        }
    }

    pub fn raw_is_object(raw: JSValue) -> bool {
        raw.tag == super::TAG_OBJECT
    }

    pub const UNDEFINED: JSValue = super::UNDEFINED;

    pub unsafe fn free_raw(rt: *mut c_void, raw: JSValue) {
        if !rt.is_null() {
            unsafe { super::JS_FreeValueRT(rt.cast(), raw) };
        }
    }

    pub fn result_raw(scope: &mut Scope<'_>, result: Result<Value, Value>) -> JSValue {
        match result {
            Ok(value) => value.into_raw(),
            Err(error) => unsafe { super::JS_Throw(scope.ctx, error.into_raw()) },
        }
    }

    pub const TYPED_ARRAY_UINT8C: c_int = 0;

    unsafe extern "C" {
        fn JS_GetTypedArrayType(obj: JSValue) -> c_int;
        fn JS_NewArrayBufferCopy(ctx: *mut JSContext, buf: *const u8, len: usize) -> JSValue;
        fn JS_GetOpaque(obj: JSValue, class_id: u32) -> *mut c_void;
    }

    pub fn typed_array_type(value: &Value) -> c_int {
        unsafe { JS_GetTypedArrayType(value.raw) }
    }

    pub fn array_buffer_copy(scope: &mut Scope<'_>, bytes: &[u8]) -> Result<Value, Value> {
        let raw = unsafe { JS_NewArrayBufferCopy(scope.ctx, bytes.as_ptr(), bytes.len()) };
        scope.take(raw)
    }

    pub const BOXED_NUMBER: c_int = 1;
    pub const BOXED_STRING: c_int = 2;
    pub const BOXED_BOOLEAN: c_int = 3;
    pub const BOXED_BIGINT: c_int = 4;

    const TAG_SYMBOL: i64 = -8;
    const GPN_ENUM_ONLY: c_int = 1 << 4;
    const PROP_C_W_E: c_int = 7;

    unsafe extern "C" {
        fn JS_IsArrayBuffer(obj: JSValue) -> bool;
        fn JS_GetArrayBuffer(ctx: *mut JSContext, psize: *mut usize, obj: JSValue) -> *mut u8;
        fn JS_GetTypedArrayBuffer(
            ctx: *mut JSContext,
            obj: JSValue,
            pbyte_offset: *mut usize,
            pbyte_length: *mut usize,
            pbytes_per_element: *mut usize,
        ) -> JSValue;
        fn JS_NewTypedArray(
            ctx: *mut JSContext,
            argc: c_int,
            argv: *mut JSValue,
            kind: c_int,
        ) -> JSValue;
        fn JS_IsDate(v: JSValue) -> bool;
        fn JS_IsRegExp(v: JSValue) -> bool;
        fn JS_IsMap(v: JSValue) -> bool;
        fn JS_IsSet(v: JSValue) -> bool;
        fn JS_IsDataView(v: JSValue) -> bool;
        fn JS_IsError(v: JSValue) -> bool;
        fn JS_GetBoxedPrimitiveKind(v: JSValue) -> c_int;
        fn JS_IsInstanceOf(ctx: *mut JSContext, val: JSValue, obj: JSValue) -> c_int;
        fn JS_IsConstructor(ctx: *mut JSContext, val: JSValue) -> bool;
        fn JS_ToObject(ctx: *mut JSContext, val: JSValue) -> JSValue;
        fn JS_ToString(ctx: *mut JSContext, val: JSValue) -> JSValue;
        fn JS_AtomToString(ctx: *mut JSContext, atom: super::JSAtom) -> JSValue;
        fn JS_GetProperty(ctx: *mut JSContext, obj: JSValue, atom: super::JSAtom) -> JSValue;
        fn JS_SetProperty(
            ctx: *mut JSContext,
            obj: JSValue,
            atom: super::JSAtom,
            val: JSValue,
        ) -> c_int;
        fn JS_DefinePropertyValue(
            ctx: *mut JSContext,
            obj: JSValue,
            atom: super::JSAtom,
            val: JSValue,
            flags: c_int,
        ) -> c_int;
    }

    unsafe extern "C" {
        fn JS_IsEngineFunction(v: JSValue) -> bool;
    }

    pub fn identity(value: &Value) -> usize {
        if value.is_object() {
            unsafe { value.raw.u.ptr as usize }
        } else {
            0
        }
    }

    pub fn take_exception(scope: &mut Scope<'_>) -> Value {
        scope.exception()
    }

    pub fn to_js_string(scope: &mut Scope<'_>, value: &Value) -> Result<Value, Value> {
        let raw = unsafe { JS_ToString(scope.ctx, value.raw) };
        scope.take(raw)
    }

    pub fn is_symbol(value: &Value) -> bool {
        value.raw.tag == TAG_SYMBOL
    }

    pub fn is_array_buffer(value: &Value) -> bool {
        unsafe { JS_IsArrayBuffer(value.raw) }
    }

    pub fn is_date(value: &Value) -> bool {
        unsafe { JS_IsDate(value.raw) }
    }

    pub fn is_regexp(value: &Value) -> bool {
        unsafe { JS_IsRegExp(value.raw) }
    }

    pub fn is_map(value: &Value) -> bool {
        unsafe { JS_IsMap(value.raw) }
    }

    pub fn is_set(value: &Value) -> bool {
        unsafe { JS_IsSet(value.raw) }
    }

    pub fn is_data_view(value: &Value) -> bool {
        unsafe { JS_IsDataView(value.raw) }
    }

    pub fn is_error(value: &Value) -> bool {
        unsafe { JS_IsError(value.raw) }
    }

    pub fn boxed_kind(value: &Value) -> c_int {
        unsafe { JS_GetBoxedPrimitiveKind(value.raw) }
    }

    pub fn is_engine_function(value: &Value) -> bool {
        unsafe { JS_IsEngineFunction(value.raw) }
    }

    pub fn is_constructor(scope: &mut Scope<'_>, value: &Value) -> bool {
        unsafe { JS_IsConstructor(scope.ctx, value.raw) }
    }

    pub fn instance_of(scope: &mut Scope<'_>, value: &Value, constructor: &Value) -> bool {
        if !constructor.is_object() {
            return false;
        }
        let status = unsafe { JS_IsInstanceOf(scope.ctx, value.raw, constructor.raw) };
        if status < 0 {
            drop(scope.exception());
        }
        status > 0
    }

    pub fn to_object(scope: &mut Scope<'_>, value: &Value) -> Result<Value, Value> {
        let raw = unsafe { JS_ToObject(scope.ctx, value.raw) };
        scope.take(raw)
    }

    pub fn array_buffer_bytes(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
        let mut size = 0usize;
        let data = unsafe { JS_GetArrayBuffer(scope.ctx, &mut size, value.raw) };
        if data.is_null() {
            drop(scope.exception());
            return None;
        }
        Some(unsafe { core::slice::from_raw_parts(data, size) }.to_vec())
    }

    pub fn fill_array_buffer(scope: &mut Scope<'_>, value: &Value, bytes: &[u8]) {
        let mut size = 0usize;
        let data = unsafe { JS_GetArrayBuffer(scope.ctx, &mut size, value.raw) };
        if data.is_null() {
            drop(scope.exception());
            return;
        }
        if size >= bytes.len() {
            unsafe { core::ptr::copy_nonoverlapping(bytes.as_ptr(), data, bytes.len()) };
        }
    }

    pub struct TypedArrayParts {
        pub buffer: Value,
        pub byte_offset: usize,
        pub byte_length: usize,
        pub bytes_per_element: usize,
    }

    pub fn typed_array_parts(
        scope: &mut Scope<'_>,
        value: &Value,
    ) -> Result<TypedArrayParts, Value> {
        let (mut offset, mut length, mut per) = (0usize, 0usize, 0usize);
        let raw = unsafe {
            JS_GetTypedArrayBuffer(scope.ctx, value.raw, &mut offset, &mut length, &mut per)
        };
        Ok(TypedArrayParts {
            buffer: scope.take(raw)?,
            byte_offset: offset,
            byte_length: length,
            bytes_per_element: per,
        })
    }

    pub fn new_typed_array(
        scope: &mut Scope<'_>,
        args: &[Value],
        kind: c_int,
    ) -> Result<Value, Value> {
        let mut raw: Vec<JSValue> = args.iter().map(|a| a.raw).collect();
        let out =
            unsafe { JS_NewTypedArray(scope.ctx, raw.len() as c_int, raw.as_mut_ptr(), kind) };
        scope.take(out)
    }

    pub fn own_enumerable_string_keys(
        scope: &mut Scope<'_>,
        object: &Value,
    ) -> Result<Vec<Value>, Value> {
        let mut tab: *mut super::JSPropertyEnum = core::ptr::null_mut();
        let mut len = 0u32;
        let flags = super::GPN_STRING_MASK | GPN_ENUM_ONLY;
        let status = unsafe {
            super::JS_GetOwnPropertyNames(scope.ctx, &mut tab, &mut len, object.raw, flags)
        };
        scope.status(status)?;
        let entries = if tab.is_null() {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(tab, len as usize) }
        };
        let keys = entries
            .iter()
            .map(|entry| Value::own(scope.ctx, unsafe { JS_AtomToString(scope.ctx, entry.atom) }))
            .collect();
        unsafe { super::JS_FreePropertyEnum(scope.ctx, tab, len) };
        Ok(keys)
    }

    fn with_atom<R>(
        scope: &mut Scope<'_>,
        key: &Value,
        f: impl FnOnce(&mut Scope<'_>, super::JSAtom) -> Result<R, Value>,
    ) -> Result<R, Value> {
        let atom = unsafe { super::JS_ValueToAtom(scope.ctx, key.raw) };
        if atom == super::ATOM_NULL {
            return Err(scope.exception());
        }
        let result = f(scope, atom);
        unsafe { super::JS_FreeAtom(scope.ctx, atom) };
        result
    }

    pub fn get_by_key(scope: &mut Scope<'_>, object: &Value, key: &Value) -> Result<Value, Value> {
        with_atom(scope, key, |scope, atom| {
            let raw = unsafe { JS_GetProperty(scope.ctx, object.raw, atom) };
            scope.take(raw)
        })
    }

    pub fn set_by_key(
        scope: &mut Scope<'_>,
        object: &Value,
        key: &Value,
        value: Value,
    ) -> Result<(), Value> {
        with_atom(scope, key, |scope, atom| {
            let status = unsafe { JS_SetProperty(scope.ctx, object.raw, atom, value.into_raw()) };
            scope.status(status)
        })
    }

    pub fn define_by_key(
        scope: &mut Scope<'_>,
        object: &Value,
        key: &Value,
        value: Value,
    ) -> Result<(), Value> {
        with_atom(scope, key, |scope, atom| {
            let status = unsafe {
                JS_DefinePropertyValue(scope.ctx, object.raw, atom, value.into_raw(), PROP_C_W_E)
            };
            scope.status(status)
        })
    }

    pub enum OwnSlot {
        Missing,
        Accessor,
        Data(Value),
    }

    pub fn own_slot(scope: &mut Scope<'_>, object: &Value, key: &str) -> Result<OwnSlot, Value> {
        let key = scope.string(key);
        with_atom(scope, &key, |scope, atom| {
            let mut desc = super::JSPropertyDescriptor {
                flags: 0,
                value: super::UNDEFINED,
                getter: super::UNDEFINED,
                setter: super::UNDEFINED,
            };
            let status =
                unsafe { super::JS_GetOwnProperty(scope.ctx, &mut desc, object.raw, atom) };
            scope.status(status)?;
            if status == 0 {
                return Ok(OwnSlot::Missing);
            }
            let value = Value::own(scope.ctx, desc.value);
            drop(Value::own(scope.ctx, desc.getter));
            drop(Value::own(scope.ctx, desc.setter));
            Ok(if desc.flags & super::PROP_GETSET != 0 {
                OwnSlot::Accessor
            } else {
                OwnSlot::Data(value)
            })
        })
    }

    pub unsafe fn with_host<T: core::any::Any, R>(
        raw: JSValue,
        f: impl FnOnce(&T) -> R,
    ) -> Option<R> {
        if raw.tag != super::TAG_OBJECT {
            return None;
        }
        let host = unsafe { JS_GetOpaque(raw, super::HOST_CLASS_ID) }.cast::<super::HostData>();
        let host = unsafe { host.as_ref() }?;
        host.data.downcast_ref::<T>().map(f)
    }

    pub fn call_c_function(
        scope: &mut Scope<'_>,
        f: JSCFunction,
        this: &Value,
        args: &[Value],
    ) -> Result<Value, Value> {
        let mut raw_args: Vec<JSValue> = args.iter().map(|arg| arg.raw).collect();
        let raw = unsafe {
            f(
                scope.ctx,
                this.raw,
                raw_args.len() as c_int,
                raw_args.as_mut_ptr(),
            )
        };
        scope.take(raw)
    }

    pub unsafe fn call_native(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
        f: impl FnOnce(&mut Scope<'_>, &Value, &[Value]) -> Result<Value, Value>,
    ) -> JSValue {
        let raw_args = if argv.is_null() || argc <= 0 {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(argv, argc as usize) }
        };
        let mut scope = Scope::of(ctx);
        let args: Vec<Value> = raw_args
            .iter()
            .map(|&raw| unsafe { borrow_value(&scope, raw) })
            .collect();
        let this = unsafe { borrow_value(&scope, this_val) };
        match f(&mut scope, &this, &args) {
            Ok(value) => value.into_raw(),
            Err(error) => unsafe { super::JS_Throw(ctx, error.into_raw()) },
        }
    }

    pub type JSCFunctionMagic = unsafe extern "C" fn(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
        magic: c_int,
    ) -> JSValue;

    pub(super) fn c_function_with(
        scope: &mut Scope<'_>,
        name: &str,
        arity: u32,
        f: JSCFunction,
        cproto: c_int,
        magic: c_int,
    ) -> Value {
        let name = super::c_text(name);
        let raw =
            unsafe { JS_NewCFunction2(scope.ctx, f, name.as_ptr(), arity as c_int, cproto, magic) };
        Value::own(scope.ctx, raw)
    }

    pub fn c_function(scope: &mut Scope<'_>, name: &str, arity: u32, f: JSCFunction) -> Value {
        let name = super::c_text(name);
        let raw = unsafe {
            JS_NewCFunction2(
                scope.ctx,
                f,
                name.as_ptr(),
                arity as c_int,
                CFUNC_GENERIC,
                0,
            )
        };
        Value::own(scope.ctx, raw)
    }

    pub fn function_realm(
        scope: &mut Scope<'_>,
        function: &Value,
    ) -> Result<*mut JSContext, Value> {
        let realm = unsafe { JS_GetFunctionRealm(scope.ctx, function.raw) };
        if realm.is_null() {
            Err(scope.exception())
        } else {
            Ok(realm)
        }
    }

    pub fn runtime(scope: &Scope<'_>) -> *mut c_void {
        scope.rt().cast()
    }

    pub fn context_opaque(scope: &Scope<'_>) -> *mut c_void {
        unsafe { JS_GetContextOpaque(scope.ctx) }
    }

    pub fn memory_usage(scope: &Scope<'_>) -> MemoryUsage {
        let mut usage = JSMemoryUsage {
            malloc_size: 0,
            malloc_limit: 0,
            memory_used_size: 0,
            rest: [0; 61],
        };
        unsafe { JS_ComputeMemoryUsage(scope.rt(), &mut usage) };
        MemoryUsage {
            malloc_size: usage.malloc_size,
            malloc_limit: usage.malloc_limit,
            memory_used_size: usage.memory_used_size,
        }
    }

    #[derive(Clone, Copy)]
    #[repr(C)]
    pub enum BrandMode {
        Throw,
        Reject,
        Ignore,
    }

    unsafe extern "C" {
        fn JS_NewCFunctionBrand(
            ctx: *mut JSContext,
            class_ids: *const u32,
            count: core::ffi::c_int,
        ) -> core::ffi::c_int;
        fn JS_SetCFunctionBrand(
            ctx: *mut JSContext,
            func: JSValue,
            brand: core::ffi::c_int,
            mode: BrandMode,
        );
    }

    pub fn new_function_brand(scope: &mut Scope<'_>, class_ids: &[u32]) -> i32 {
        unsafe { JS_NewCFunctionBrand(scope.ctx, class_ids.as_ptr(), class_ids.len() as _) }
    }

    pub fn set_function_brand(
        scope: &mut Scope<'_>,
        function: &Value,
        brand: i32,
        mode: BrandMode,
    ) {
        unsafe { JS_SetCFunctionBrand(scope.ctx, function.raw, brand, mode) };
    }
}

pub struct Realm {
    ctx: *mut JSContext,
}

impl Drop for Realm {
    fn drop(&mut self) {
        unsafe { JS_FreeContext(self.ctx) };
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

    pub fn eval_native_script(&mut self, source: &str, name: &str) -> Result<Value, Value> {
        let input = nul_terminated(source);
        let name = c_text(name);
        let raw = unsafe {
            JS_Eval(
                self.ctx,
                input.as_ptr().cast(),
                source.len(),
                name.as_ptr(),
                EVAL_TYPE_GLOBAL | EVAL_FLAG_HIDE_SOURCE,
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

    pub fn constructor_or_function(&mut self, name: &str, arity: u32, f: NativeFn) -> Value {
        let trampoline: quickjs::JSCFunctionMagic = call_native_magic;
        let generic: quickjs::JSCFunction = unsafe { mem::transmute(trampoline) };
        quickjs::c_function_with(
            self,
            name,
            arity,
            generic,
            CFUNC_CONSTRUCTOR_OR_FUNC_MAGIC,
            native_index(f),
        )
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

    pub fn bound_constructor(
        &mut self,
        name: &str,
        arity: u32,
        f: BoundFn,
        data: &[Value],
    ) -> Value {
        let function = self.bound_function(name, arity, f, data);
        unsafe { JS_SetConstructorBit(self.ctx, function.raw, 1) };
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

    pub fn define_accessor(
        &mut self,
        object: &Value,
        key: &str,
        getter: Option<&Value>,
        setter: Option<&Value>,
        attributes: Attributes,
    ) -> Result<(), Value> {
        let key = c_text(key);
        let atom = unsafe { JS_NewAtom(self.ctx, key.as_ptr()) };
        let accessor =
            |value: Option<&Value>| value.cloned().unwrap_or_else(Value::undefined).into_raw();
        let status = unsafe {
            JS_DefinePropertyGetSet(
                self.ctx,
                object.raw,
                atom,
                accessor(getter),
                accessor(setter),
                prop_flags(attributes),
            )
        };
        unsafe { JS_FreeAtom(self.ctx, atom) };
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

    pub fn delete(&mut self, object: &Value, key: &str) -> Result<bool, Value> {
        let key = c_text(key);
        let atom = unsafe { JS_NewAtom(self.ctx, key.as_ptr()) };
        let status = unsafe { JS_DeleteProperty(self.ctx, object.raw, atom, 0) };
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

    pub fn syntax_error(&mut self, message: &str) -> Value {
        let message = c_text(message);
        unsafe { JS_ThrowSyntaxError(self.ctx, c"%s".as_ptr(), message.as_ptr()) };
        self.exception()
    }

    pub fn freeze(&mut self, object: &Value) -> Result<(), Value> {
        let status = unsafe { JS_FreezeObject(self.ctx, object.raw) };
        self.status(status)
    }

    pub fn get_key(&mut self, object: &Value, key: &Value) -> Result<Value, Value> {
        quickjs::get_by_key(self, object, key)
    }

    pub fn dom_exception(&mut self, name: &str, message: &str) -> Value {
        let name = c_text(name);
        let message = c_text(message);
        unsafe { JS_ThrowDOMException(self.ctx, name.as_ptr(), c"%s".as_ptr(), message.as_ptr()) };
        self.exception()
    }

    pub fn write_object(&mut self, value: &Value) -> Result<Vec<u8>, Option<Value>> {
        let mut len = 0usize;
        let buf = unsafe { JS_WriteObject(self.ctx, &mut len, value.raw, OBJ_REFERENCE) };
        if buf.is_null() {
            return Err(unsafe { JS_HasException(self.ctx) }.then(|| self.exception()));
        }
        let bytes = unsafe { core::slice::from_raw_parts(buf, len) }.to_vec();
        unsafe { js_free(self.ctx, buf.cast()) };
        Ok(bytes)
    }

    pub fn read_object(&mut self, bytes: &[u8]) -> Result<Value, Value> {
        let raw = unsafe { JS_ReadObject(self.ctx, bytes.as_ptr(), bytes.len(), OBJ_REFERENCE) };
        self.take(raw)
    }

    pub fn new_host_object<T: Any>(&mut self, prototype: Option<&Value>, data: T) -> Value {
        self.host_object(prototype, data, None)
    }

    fn host_object<T: Any>(
        &mut self,
        prototype: Option<&Value>,
        data: T,
        trace: Option<HostTrace>,
    ) -> Value {
        register_host_class(self.rt());
        let raw = unsafe {
            match prototype {
                Some(prototype) => JS_NewObjectProtoClass(self.ctx, prototype.raw, HOST_CLASS_ID),
                None => JS_NewObjectClass(self.ctx, HOST_CLASS_ID as c_int),
            }
        };
        if raw.tag == TAG_OBJECT {
            let boxed = Box::into_raw(Box::new(HostData {
                data: Box::new(data),
                trace,
            }));
            unsafe { JS_SetOpaque(raw, boxed.cast()) };
        }
        Value::own(self.ctx, raw)
    }

    pub fn new_traced_host_object<T: Any + Trace>(
        &mut self,
        prototype: Option<&Value>,
        data: T,
    ) -> Value {
        self.host_object(prototype, data, Some(trace_as::<T>))
    }

    pub fn host_data<T: Any + Clone>(&mut self, value: &Value) -> Option<T> {
        if value.raw.tag != TAG_OBJECT {
            return None;
        }
        let data = unsafe { JS_GetOpaque(value.raw, HOST_CLASS_ID) }.cast::<HostData>();
        unsafe { data.as_ref() }.and_then(|host| host.data.downcast_ref::<T>().cloned())
    }

    pub fn detach_array_buffer(&mut self, value: &Value) -> Result<(), Value> {
        unsafe { JS_DetachArrayBuffer(self.ctx, value.raw) };
        Ok(())
    }

    pub unsafe fn external_array_buffer(
        &mut self,
        data: *mut u8,
        len: usize,
    ) -> Result<Value, Value> {
        let raw = unsafe {
            JS_NewArrayBuffer(self.ctx, data, len, 0, ptr::null(), ptr::null_mut(), false)
        };
        self.take(raw)
    }

    pub fn buffer_source_bytes(&mut self, value: &Value) -> Option<Vec<u8>> {
        if !unsafe { JS_IsArrayBuffer(value.raw) } {
            return self.view_data(value);
        }
        let mut total = 0usize;
        let base = unsafe { JS_GetArrayBuffer(self.ctx, &mut total, value.raw) };
        if base.is_null() {
            drop(self.exception());
            return None;
        }
        Some(unsafe { core::slice::from_raw_parts(base, total) }.to_vec())
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

    pub fn typed_array_element(&mut self, value: &Value) -> Option<ElementType> {
        let kind = unsafe { JS_GetTypedArrayType(value.raw) };
        usize::try_from(kind)
            .ok()
            .and_then(|kind| ELEMENT_TYPES.get(kind).copied())
    }

    pub fn with_buffer_bytes_mut<R>(
        &mut self,
        value: &Value,
        f: impl FnOnce(&mut [u8]) -> R,
    ) -> Option<R> {
        if unsafe { JS_IsArrayBuffer(value.raw) } {
            let mut len = 0usize;
            let base = unsafe { JS_GetArrayBuffer(self.ctx, &mut len, value.raw) };
            if base.is_null() {
                return None;
            }
            return Some(f(unsafe { core::slice::from_raw_parts_mut(base, len) }));
        }
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
        let buffer = self.take(buffer).ok()?;
        let mut total = 0usize;
        let base = unsafe { JS_GetArrayBuffer(self.ctx, &mut total, buffer.raw) };
        if base.is_null()
            || byte_offset
                .checked_add(length)
                .is_none_or(|end| end > total)
        {
            return None;
        }
        let bytes = unsafe { core::slice::from_raw_parts_mut(base.add(byte_offset), length) };
        let result = f(bytes);
        drop(buffer);
        Some(result)
    }

    pub fn new_typed_array(&mut self, kind: ElementType, bytes: &[u8]) -> Result<Value, Value> {
        let index = ELEMENT_TYPES
            .iter()
            .position(|&known| known == kind)
            .unwrap_or(0) as c_int;
        let buffer = unsafe { JS_NewArrayBufferCopy(self.ctx, bytes.as_ptr(), bytes.len()) };
        let buffer = self.take(buffer)?;
        let mut args = [buffer.raw, UNDEFINED, UNDEFINED];
        let raw = unsafe { JS_NewTypedArray(self.ctx, 3, args.as_mut_ptr(), index) };
        self.take(raw)
    }

    pub fn view_data(&mut self, value: &Value) -> Option<Vec<u8>> {
        if value.raw.tag != TAG_OBJECT {
            return None;
        }
        let (mut byte_offset, mut length) = (0usize, 0usize);
        let buffer = unsafe {
            JS_GetArrayBufferViewBuffer(self.ctx, value.raw, &mut byte_offset, &mut length)
        };
        let buffer = self.take(buffer).ok()?;
        let mut total = 0usize;
        let base = unsafe { JS_GetArrayBuffer(self.ctx, &mut total, buffer.raw) };
        if base.is_null() {
            drop(self.exception());
            return None;
        }
        if byte_offset
            .checked_add(length)
            .is_none_or(|end| end > total)
        {
            return None;
        }
        Some(unsafe { core::slice::from_raw_parts(base.add(byte_offset), length) }.to_vec())
    }

    pub fn gc(&mut self) {
        unsafe { JS_RunGC(self.rt()) };
    }

    pub fn own_property_keys(
        &mut self,
        object: &Value,
        symbols: bool,
    ) -> Result<Vec<Value>, Value> {
        let mut tab: *mut JSPropertyEnum = ptr::null_mut();
        let mut len = 0u32;
        let flags = GPN_STRING_MASK | if symbols { GPN_SYMBOL_MASK } else { 0 };
        let status =
            unsafe { JS_GetOwnPropertyNames(self.ctx, &mut tab, &mut len, object.raw, flags) };
        self.status(status)?;
        let entries = if tab.is_null() {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(tab, len as usize) }
        };
        let keys = entries
            .iter()
            .map(|entry| Value::own(self.ctx, unsafe { JS_AtomToValue(self.ctx, entry.atom) }))
            .collect();
        unsafe { JS_FreePropertyEnum(self.ctx, tab, len) };
        Ok(keys)
    }

    pub fn own_property(
        &mut self,
        object: &Value,
        key: &Value,
    ) -> Result<Option<PropertyDescriptor>, Value> {
        let atom = unsafe { JS_ValueToAtom(self.ctx, key.raw) };
        if atom == ATOM_NULL {
            return Err(self.exception());
        }
        let mut desc = JSPropertyDescriptor {
            flags: 0,
            value: UNDEFINED,
            getter: UNDEFINED,
            setter: UNDEFINED,
        };
        let status = unsafe { JS_GetOwnProperty(self.ctx, &mut desc, object.raw, atom) };
        unsafe { JS_FreeAtom(self.ctx, atom) };
        self.status(status)?;
        Ok((status > 0).then(|| PropertyDescriptor {
            value: Value::own(self.ctx, desc.value),
            getter: Value::own(self.ctx, desc.getter),
            setter: Value::own(self.ctx, desc.setter),
            accessor: desc.flags & PROP_GETSET != 0,
        }))
    }

    pub fn object_kind(&mut self, value: &Value) -> Option<ObjectKind> {
        if !value.is_object() {
            return None;
        }
        if quickjs::is_array_buffer(value) {
            return Some(ObjectKind::ArrayBuffer);
        }
        if let Some(element) = self.typed_array_element(value) {
            return Some(ObjectKind::TypedArray(element));
        }
        Some(if quickjs::is_data_view(value) {
            ObjectKind::DataView
        } else if quickjs::is_date(value) {
            ObjectKind::Date
        } else if quickjs::is_regexp(value) {
            ObjectKind::RegExp
        } else if quickjs::is_map(value) {
            ObjectKind::Map
        } else if quickjs::is_set(value) {
            ObjectKind::Set
        } else if quickjs::is_error(value) {
            ObjectKind::Error
        } else {
            match quickjs::boxed_kind(value) {
                quickjs::BOXED_NUMBER => ObjectKind::Number,
                quickjs::BOXED_STRING => ObjectKind::String,
                quickjs::BOXED_BOOLEAN => ObjectKind::Boolean,
                quickjs::BOXED_BIGINT => ObjectKind::BigInt,
                _ => ObjectKind::Other,
            }
        })
    }

    pub fn typed_array_view(&mut self, value: &Value) -> Result<Option<TypedArrayView>, Value> {
        let Some(element) = self.typed_array_element(value) else {
            return Ok(None);
        };
        let parts = quickjs::typed_array_parts(self, value)?;
        Ok(Some(TypedArrayView {
            buffer: parts.buffer,
            byte_offset: parts.byte_offset,
            length: parts
                .byte_length
                .checked_div(parts.bytes_per_element)
                .unwrap_or(0),
            element,
        }))
    }

    pub fn new_array_buffer(&mut self, bytes: &[u8]) -> Result<Value, Value> {
        quickjs::array_buffer_copy(self, bytes)
    }

    pub fn array_buffer_bytes(&mut self, value: &Value) -> Option<Vec<u8>> {
        quickjs::array_buffer_bytes(self, value)
    }

    pub fn own_enumerable_keys(&mut self, object: &Value) -> Result<Vec<Value>, Value> {
        quickjs::own_enumerable_string_keys(self, object)
    }

    pub fn set_key(&mut self, object: &Value, key: &Value, value: Value) -> Result<(), Value> {
        quickjs::set_by_key(self, object, key, value)
    }

    pub fn define_entry(&mut self, object: &Value, key: &Value, value: Value) -> Result<(), Value> {
        quickjs::define_by_key(self, object, key, value)
    }

    pub fn is_native_function(&mut self, value: &Value) -> bool {
        self.is_function(value) && quickjs::is_engine_function(value)
    }

    pub fn to_object(&mut self, value: &Value) -> Result<Value, Value> {
        quickjs::to_object(self, value)
    }

    pub fn get_prototype(&mut self, object: &Value) -> Result<Value, Value> {
        let raw = unsafe { JS_GetPrototype(self.ctx, object.raw) };
        self.take(raw)
    }

    pub fn set_prototype(&mut self, object: &Value, prototype: &Value) -> Result<(), Value> {
        let status = unsafe { JS_SetPrototype(self.ctx, object.raw, prototype.raw) };
        self.status(status)
    }

    pub fn enqueue_job(&mut self, job: Job) -> Result<(), Value> {
        let mut index = Value::int(job_index(job)).into_raw();
        let status = unsafe { JS_EnqueueJob(self.ctx, run_job, 1, &mut index) };
        self.status(status)
    }

    pub fn is_error(&mut self, value: &Value) -> bool {
        quickjs::is_error(value)
    }

    pub fn json_stringify(&mut self, value: &Value) -> Result<Value, Value> {
        let raw = unsafe { JS_JSONStringify(self.ctx, value.raw, UNDEFINED, UNDEFINED) };
        self.take(raw)
    }

    pub fn enqueue_call(&mut self, function: &Value, args: &[Value]) -> Result<(), Value> {
        let mut raw: Vec<JSValue> = core::iter::once(function)
            .chain(args)
            .map(|value| value.raw)
            .collect();
        let status =
            unsafe { JS_EnqueueJob(self.ctx, call_job, raw.len() as c_int, raw.as_mut_ptr()) };
        self.status(status)
    }

    pub fn is_function(&mut self, value: &Value) -> bool {
        unsafe { JS_IsFunction(self.ctx, value.raw) }
    }

    pub fn to_string_value(&mut self, value: &Value) -> Result<Value, Value> {
        quickjs::to_js_string(self, value)
    }

    pub fn is_constructor(&mut self, value: &Value) -> bool {
        quickjs::is_constructor(self, value)
    }

    pub fn instance_of(&mut self, value: &Value, constructor: &Value) -> bool {
        quickjs::instance_of(self, value, constructor)
    }

    pub fn new_error(&mut self) -> Value {
        Value::own(self.ctx, unsafe { JS_NewError(self.ctx) })
    }

    pub fn new_promise(&mut self) -> Result<(Value, Value, Value), Value> {
        let mut resolving = [UNDEFINED; 2];
        let promise = unsafe { JS_NewPromiseCapability(self.ctx, resolving.as_mut_ptr()) };
        let promise = self.take(promise)?;
        let [resolve, reject] = resolving.map(|raw| Value::own(self.ctx, raw));
        Ok((promise, resolve, reject))
    }

    pub fn rejected_promise(&mut self, reason: &Value) -> Result<Value, Value> {
        let mut resolving = [UNDEFINED; 2];
        let promise = unsafe { JS_NewPromiseCapability(self.ctx, resolving.as_mut_ptr()) };
        let promise = self.take(promise)?;
        let [resolve, reject] = resolving.map(|raw| Value::own(self.ctx, raw));
        drop(resolve);
        self.call(&reject, &Value::undefined(), core::slice::from_ref(reason))?;
        Ok(promise)
    }

    pub fn new_detached_realm(&mut self) -> Option<Realm> {
        let ctx = unsafe { JS_NewContext(self.rt()) };
        (!ctx.is_null()).then_some(Realm { ctx })
    }

    pub fn in_realm<R>(&mut self, realm: &Realm, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        f(&mut Scope::of(realm.ctx))
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
