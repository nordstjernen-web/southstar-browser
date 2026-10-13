//! Southstar — property tables on QuickJS: each table becomes a cached JSCFunctionListEntry array whose magic indexes the native it dispatches to, so QuickJS still creates the functions lazily.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
use core::ptr;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Mutex, RwLock};

use super::function_list::{FunctionListEntry, accessor_entry, install_entries, method_entry};
use super::{JS_Throw, JSContext, JSValue, Scope, UNDEFINED, Value};
use crate::{GetterFn, GetterMagicFn, NativeFn, Prop, SetterFn, SetterMagicFn};

#[derive(Clone, Copy)]
enum Accessor {
    Plain(Option<GetterFn>, Option<SetterFn>),
    Magic(Option<GetterMagicFn>, Option<SetterMagicFn>, i32),
}

static METHODS: RwLock<Vec<NativeFn>> = RwLock::new(Vec::new());
static ACCESSORS: RwLock<Vec<Accessor>> = RwLock::new(Vec::new());
static TABLES: Mutex<Option<HashMap<usize, &'static [FunctionListEntry]>>> = Mutex::new(None);

fn method_index(f: NativeFn) -> i16 {
    let mut methods = METHODS.write().unwrap_or_else(|e| e.into_inner());
    let index = methods
        .iter()
        .position(|&known| ptr::fn_addr_eq(known, f))
        .unwrap_or_else(|| {
            methods.push(f);
            methods.len() - 1
        });
    i16::try_from(index).unwrap_or(i16::MAX)
}

fn accessor_index(accessor: Accessor) -> i16 {
    let mut accessors = ACCESSORS.write().unwrap_or_else(|e| e.into_inner());
    accessors.push(accessor);
    i16::try_from(accessors.len() - 1).unwrap_or(i16::MAX)
}

fn method(magic: c_int) -> Option<NativeFn> {
    let methods = METHODS.read().unwrap_or_else(|e| e.into_inner());
    methods.get(magic as usize).copied()
}

fn accessor(magic: c_int) -> Option<Accessor> {
    let accessors = ACCESSORS.read().unwrap_or_else(|e| e.into_inner());
    accessors.get(magic as usize).copied()
}

fn finish(ctx: *mut JSContext, result: Result<Value, Value>) -> JSValue {
    match result {
        Ok(value) => value.into_raw(),
        Err(error) => unsafe { JS_Throw(ctx, error.into_raw()) },
    }
}

unsafe extern "C" fn call_method(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    magic: c_int,
) -> JSValue {
    let Some(f) = method(magic) else {
        return UNDEFINED;
    };
    unsafe { super::quickjs::call_native(ctx, this_val, argc, argv, f) }
}

unsafe extern "C" fn call_getter(ctx: *mut JSContext, this_val: JSValue, magic: c_int) -> JSValue {
    let mut scope = Scope::of(ctx);
    let this = unsafe { super::quickjs::borrow_value(&scope, this_val) };
    let result = match accessor(magic) {
        Some(Accessor::Plain(Some(get), _)) => get(&mut scope, &this),
        Some(Accessor::Magic(Some(get), _, magic)) => get(&mut scope, &this, magic),
        _ => Ok(Value::undefined()),
    };
    finish(ctx, result)
}

unsafe extern "C" fn call_setter(
    ctx: *mut JSContext,
    this_val: JSValue,
    value: JSValue,
    magic: c_int,
) -> JSValue {
    let mut scope = Scope::of(ctx);
    let this = unsafe { super::quickjs::borrow_value(&scope, this_val) };
    let value = unsafe { super::quickjs::borrow_value(&scope, value) };
    let result = match accessor(magic) {
        Some(Accessor::Plain(_, Some(set))) => set(&mut scope, &this, &value),
        Some(Accessor::Magic(_, Some(set), magic)) => set(&mut scope, &this, &value, magic),
        _ => Ok(()),
    };
    finish(ctx, result.map(|()| Value::undefined()))
}

fn leak_name(name: &str) -> *const c_char {
    let name = CString::new(name.replace('\0', "")).unwrap_or_default();
    Box::leak(name.into_boxed_c_str()).as_ptr()
}

fn build(props: &[Prop]) -> &'static [FunctionListEntry] {
    let entries: Vec<FunctionListEntry> = props
        .iter()
        .map(|prop| match *prop {
            Prop::Method {
                name,
                arity,
                f,
                enumerable,
            } => method_entry(
                leak_name(name),
                arity,
                call_method,
                method_index(f),
                enumerable,
            ),
            Prop::Accessor { name, get, set } => accessor_entry(
                leak_name(name),
                get.map(|_| call_getter as super::function_list::GetterMagic),
                set.map(|_| call_setter as super::function_list::SetterMagic),
                accessor_index(Accessor::Plain(get, set)),
            ),
            Prop::AccessorMagic {
                name,
                get,
                set,
                magic,
            } => accessor_entry(
                leak_name(name),
                get.map(|_| call_getter as super::function_list::GetterMagic),
                set.map(|_| call_setter as super::function_list::SetterMagic),
                accessor_index(Accessor::Magic(get, set, i32::from(magic))),
            ),
        })
        .collect();
    Box::leak(entries.into_boxed_slice())
}

fn entries_for(props: &'static [Prop]) -> &'static [FunctionListEntry] {
    let key = props.as_ptr() as usize;
    let mut tables = TABLES.lock().unwrap_or_else(|e| e.into_inner());
    let tables = tables.get_or_insert_with(HashMap::new);
    if let Some(entries) = tables.get(&key) {
        return entries;
    }
    let entries = build(props);
    tables.insert(key, entries);
    entries
}

impl Scope<'_> {
    pub fn install_props(&mut self, object: &Value, props: &'static [Prop]) {
        if props.is_empty() || !object.is_object() {
            return;
        }
        let entries = entries_for(props);
        install_entries(self, object, entries);
    }
}
