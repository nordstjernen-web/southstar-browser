//! Southstar — QuickJS function lists: static tables of native methods and accessors laid out as JSCFunctionListEntry and installed with JS_SetPropertyFunctionList.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use super::quickjs::JSCFunction;
use super::{JSContext, JSValue, Scope, Value};

pub type Getter = unsafe extern "C" fn(ctx: *mut JSContext, this_val: JSValue) -> JSValue;
pub type Setter =
    unsafe extern "C" fn(ctx: *mut JSContext, this_val: JSValue, value: JSValue) -> JSValue;
pub type GetterMagic =
    unsafe extern "C" fn(ctx: *mut JSContext, this_val: JSValue, magic: c_int) -> JSValue;
pub type SetterMagic = unsafe extern "C" fn(
    ctx: *mut JSContext,
    this_val: JSValue,
    value: JSValue,
    magic: c_int,
) -> JSValue;

const PROP_CONFIGURABLE: u8 = 1 << 0;
const PROP_WRITABLE: u8 = 1 << 1;
const PROP_ENUMERABLE: u8 = 1 << 2;
const DEF_CFUNC: u8 = 0;
const DEF_CGETSET: u8 = 1;
const DEF_CGETSET_MAGIC: u8 = 2;
const CFUNC_GENERIC: u8 = 0;

#[repr(C)]
#[derive(Clone, Copy)]
union FunctionType {
    generic: Option<JSCFunction>,
    getter: Option<Getter>,
    setter: Option<Setter>,
    getter_magic: Option<GetterMagic>,
    setter_magic: Option<SetterMagic>,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FunctionDef {
    length: u8,
    cproto: u8,
    cfunc: FunctionType,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct GetSetDef {
    get: FunctionType,
    set: FunctionType,
}

#[repr(C)]
#[derive(Clone, Copy)]
union EntryValue {
    func: FunctionDef,
    getset: GetSetDef,
    i64: i64,
}

#[repr(C)]
pub struct FunctionListEntry {
    name: *const c_char,
    prop_flags: u8,
    def_type: u8,
    magic: i16,
    u: EntryValue,
}

unsafe impl Sync for FunctionListEntry {}

const _: () = assert!(core::mem::size_of::<FunctionListEntry>() == 32);

unsafe extern "C" {
    fn JS_SetPropertyFunctionList(
        ctx: *mut JSContext,
        obj: JSValue,
        tab: *const FunctionListEntry,
        len: c_int,
    ) -> c_int;
}

pub const fn cfunc(name: &'static CStr, length: u8, f: JSCFunction) -> FunctionListEntry {
    FunctionListEntry {
        name: name.as_ptr(),
        prop_flags: PROP_WRITABLE | PROP_CONFIGURABLE | PROP_ENUMERABLE,
        def_type: DEF_CFUNC,
        magic: 0,
        u: EntryValue {
            func: FunctionDef {
                length,
                cproto: CFUNC_GENERIC,
                cfunc: FunctionType { generic: Some(f) },
            },
        },
    }
}

pub const fn getset(
    name: &'static CStr,
    get: Option<Getter>,
    set: Option<Setter>,
) -> FunctionListEntry {
    FunctionListEntry {
        name: name.as_ptr(),
        prop_flags: PROP_CONFIGURABLE | PROP_ENUMERABLE,
        def_type: DEF_CGETSET,
        magic: 0,
        u: EntryValue {
            getset: GetSetDef {
                get: FunctionType { getter: get },
                set: FunctionType { setter: set },
            },
        },
    }
}

pub const fn getset_magic(
    name: &'static CStr,
    get: Option<GetterMagic>,
    set: Option<SetterMagic>,
    magic: i16,
) -> FunctionListEntry {
    FunctionListEntry {
        name: name.as_ptr(),
        prop_flags: PROP_CONFIGURABLE | PROP_ENUMERABLE,
        def_type: DEF_CGETSET_MAGIC,
        magic,
        u: EntryValue {
            getset: GetSetDef {
                get: FunctionType { getter_magic: get },
                set: FunctionType { setter_magic: set },
            },
        },
    }
}

pub fn set_function_list(
    scope: &mut Scope<'_>,
    object: &Value,
    list: &'static [FunctionListEntry],
) {
    unsafe {
        JS_SetPropertyFunctionList(scope.ctx, object.raw, list.as_ptr(), list.len() as c_int);
    }
}
