//! Southstar — the QuickJS object model calls the realm cloner needs: atoms, property descriptors with their flags, class prototypes, C function clones and forwarders.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use std::ffi::CString;

use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

type JSAtom = u32;

pub(crate) const PROP_CONFIGURABLE: c_int = 1 << 0;
pub(crate) const PROP_WRITABLE: c_int = 1 << 1;
pub(crate) const PROP_ENUMERABLE: c_int = 1 << 2;
pub(crate) const PROP_C_W_E: c_int = PROP_CONFIGURABLE | PROP_WRITABLE | PROP_ENUMERABLE;
pub(crate) const PROP_GETSET: c_int = 1 << 4;
pub(crate) const PROP_HAS_CONFIGURABLE: c_int = 1 << 8;
pub(crate) const PROP_HAS_WRITABLE: c_int = 1 << 9;
pub(crate) const PROP_HAS_ENUMERABLE: c_int = 1 << 10;
pub(crate) const PROP_HAS_GET: c_int = 1 << 11;
pub(crate) const PROP_HAS_SET: c_int = 1 << 12;
pub(crate) const PROP_HAS_VALUE: c_int = 1 << 13;

pub(crate) const GPN_STRING: c_int = 1 << 0;
pub(crate) const GPN_SYMBOL: c_int = 1 << 1;
pub(crate) const GPN_PRIVATE: c_int = 1 << 2;
pub(crate) const GPN_ENUM_ONLY: c_int = 1 << 4;

const EVAL_TYPE_GLOBAL: c_int = 0;
const EVAL_FLAG_HIDE_SOURCE: c_int = 1 << 8;

#[repr(C)]
struct JSPropertyEnum {
    is_enumerable: bool,
    atom: JSAtom,
}

#[repr(C)]
struct JSPropertyDescriptor {
    flags: c_int,
    value: JSValue,
    getter: JSValue,
    setter: JSValue,
}

unsafe extern "C" {
    fn JS_NewAtom(ctx: *mut JSContext, s: *const c_char) -> JSAtom;
    fn JS_FreeAtom(ctx: *mut JSContext, atom: JSAtom);
    fn JS_DupAtom(ctx: *mut JSContext, atom: JSAtom) -> JSAtom;
    fn JS_ValueToAtom(ctx: *mut JSContext, v: JSValue) -> JSAtom;
    fn JS_AtomIsArrayIndex(ctx: *mut JSContext, pval: *mut u32, atom: JSAtom) -> bool;
    fn JS_AtomToCStringLen(ctx: *mut JSContext, plen: *mut usize, atom: JSAtom) -> *const c_char;
    fn JS_FreeCString(ctx: *mut JSContext, ptr: *const c_char);
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
    fn JS_GetProperty(ctx: *mut JSContext, obj: JSValue, prop: JSAtom) -> JSValue;
    fn JS_DefineProperty(
        ctx: *mut JSContext,
        obj: JSValue,
        prop: JSAtom,
        val: JSValue,
        getter: JSValue,
        setter: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_DefinePropertyValue(
        ctx: *mut JSContext,
        obj: JSValue,
        prop: JSAtom,
        val: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_DeleteProperty(ctx: *mut JSContext, obj: JSValue, prop: JSAtom, flags: c_int) -> c_int;
    fn JS_GetPrototype(ctx: *mut JSContext, val: JSValue) -> JSValue;
    fn JS_SetPrototype(ctx: *mut JSContext, obj: JSValue, proto: JSValue) -> c_int;
    fn JS_IsFunction(ctx: *mut JSContext, val: JSValue) -> bool;
    fn JS_IsConstructor(ctx: *mut JSContext, val: JSValue) -> bool;
    fn JS_ToBool(ctx: *mut JSContext, val: JSValue) -> c_int;
    fn JS_CloneCFunction(ctx: *mut JSContext, func: JSValue) -> JSValue;
    fn JS_NewForwarder(
        ctx: *mut JSContext,
        target: JSValue,
        name: *const c_char,
        length: c_int,
        constructor: bool,
    ) -> JSValue;
    fn JS_GetRuntime(ctx: *mut JSContext) -> *mut c_void;
    fn JS_GetClassCount(rt: *mut c_void) -> c_int;
    fn JS_GetClassID(v: JSValue) -> u32;
    fn JS_GetClassProto(ctx: *mut JSContext, class_id: u32) -> JSValue;
    fn JS_SetClassProto(ctx: *mut JSContext, class_id: u32, obj: JSValue);
    fn JS_NewObjectClass(ctx: *mut JSContext, class_id: c_int) -> JSValue;
    fn JS_NewObjectProto(ctx: *mut JSContext, proto: JSValue) -> JSValue;
    fn JS_GetOpaque(obj: JSValue, class_id: u32) -> *mut c_void;
    fn JS_SetOpaque(obj: JSValue, opaque: *mut c_void) -> c_int;
    fn JS_GetContextOpaque(ctx: *mut JSContext) -> *mut c_void;
    fn JS_Eval(
        ctx: *mut JSContext,
        input: *const c_char,
        input_len: usize,
        filename: *const c_char,
        eval_flags: c_int,
    ) -> JSValue;
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Ctx(usize);

pub(crate) struct Atom {
    ctx: Ctx,
    atom: JSAtom,
}

impl Drop for Atom {
    fn drop(&mut self) {
        unsafe { JS_FreeAtom(self.ctx.ptr(), self.atom) };
    }
}

impl Atom {
    pub fn new(ctx: Ctx, name: &CStr) -> Atom {
        Atom {
            ctx,
            atom: unsafe { JS_NewAtom(ctx.ptr(), name.as_ptr()) },
        }
    }

    pub fn same(&self, other: &Atom) -> bool {
        self.atom == other.atom
    }

    pub fn name(&self) -> Option<CString> {
        let ctx = self.ctx.ptr();
        let mut len = 0usize;
        let text = unsafe { JS_AtomToCStringLen(ctx, &mut len, self.atom) };
        if text.is_null() {
            self.ctx.clear_exception();
            return None;
        }
        let owned = unsafe { CStr::from_ptr(text) }.to_owned();
        unsafe { JS_FreeCString(ctx, text) };
        Some(owned)
    }
}

pub(crate) enum PropertyKey {
    Index(u32),
    Name(CString),
}

pub(crate) struct Descriptor {
    pub flags: c_int,
    pub value: Value,
    pub getter: Value,
    pub setter: Value,
}

impl Descriptor {
    pub fn is_accessor(&self) -> bool {
        self.flags & PROP_GETSET != 0
    }
}

pub(crate) fn class_id(value: &Value) -> u32 {
    unsafe { JS_GetClassID(quickjs::raw(value)) }
}

pub(crate) fn opaque(value: &Value, class: u32) -> *mut c_void {
    unsafe { JS_GetOpaque(quickjs::raw(value), class) }
}

pub(crate) fn set_opaque(value: &Value, data: *mut c_void) {
    unsafe { JS_SetOpaque(quickjs::raw(value), data) };
}

fn trimmed_c_text(text: &str) -> CString {
    let end = text.find('\0').unwrap_or(text.len());
    CString::new(&text[..end]).unwrap_or_default()
}

impl Ctx {
    pub const NULL: Ctx = Ctx(0);

    pub fn from_ptr(ctx: *mut JSContext) -> Option<Ctx> {
        (!ctx.is_null()).then_some(Ctx(ctx as usize))
    }

    pub fn ptr(self) -> *mut JSContext {
        self.0 as *mut JSContext
    }

    pub fn opaque(self) -> *mut c_void {
        unsafe { JS_GetContextOpaque(self.ptr()) }
    }

    pub fn enter<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        unsafe { quickjs::with_context(self.ptr(), f) }
    }

    pub fn take(self, raw: JSValue) -> Option<Value> {
        self.enter(|scope| {
            let value = unsafe { quickjs::take_value(scope, raw) };
            quickjs::checked(scope, value).ok()
        })
    }

    pub fn wrap(self, raw: JSValue) -> Value {
        self.enter(|scope| unsafe { quickjs::take_value(scope, raw) })
    }

    pub fn borrow(self, raw: JSValue) -> Value {
        self.enter(|scope| unsafe { quickjs::borrow_value(scope, raw) })
    }

    pub fn clear_exception(self) {
        self.enter(|scope| drop(quickjs::take_exception(scope)));
    }

    pub fn global(self) -> Value {
        self.enter(|scope| scope.global())
    }

    pub fn get(self, object: &Value, name: &str) -> Value {
        self.enter(|scope| scope.get(object, name))
            .unwrap_or_else(|_| Value::undefined())
    }

    pub fn get_atom(self, object: &Value, atom: &Atom) -> Value {
        let raw = unsafe { JS_GetProperty(self.ptr(), quickjs::raw(object), atom.atom) };
        self.take(raw).unwrap_or_else(Value::undefined)
    }

    pub fn set(self, object: &Value, name: &str, value: Value) {
        let _ = self.enter(|scope| scope.set(object, name, value));
    }

    pub fn delete(self, object: &Value, name: &CStr) -> c_int {
        let atom = Atom::new(self, name);
        let status = unsafe { JS_DeleteProperty(self.ptr(), quickjs::raw(object), atom.atom, 0) };
        if status < 0 {
            self.clear_exception();
        }
        status
    }

    pub fn own_keys(self, object: &Value, flags: c_int) -> Option<Vec<Atom>> {
        let mut tab: *mut JSPropertyEnum = core::ptr::null_mut();
        let mut len = 0u32;
        let status = unsafe {
            JS_GetOwnPropertyNames(self.ptr(), &mut tab, &mut len, quickjs::raw(object), flags)
        };
        if status < 0 {
            self.clear_exception();
            return None;
        }
        let entries = if tab.is_null() {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(tab, len as usize) }
        };
        let atoms = entries
            .iter()
            .map(|entry| Atom {
                ctx: self,
                atom: unsafe { JS_DupAtom(self.ptr(), entry.atom) },
            })
            .collect();
        unsafe { JS_FreePropertyEnum(self.ptr(), tab, len) };
        Some(atoms)
    }

    pub fn own_property(self, object: &Value, atom: &Atom) -> Option<Descriptor> {
        let mut desc = JSPropertyDescriptor {
            flags: 0,
            value: quickjs::UNDEFINED,
            getter: quickjs::UNDEFINED,
            setter: quickjs::UNDEFINED,
        };
        let status =
            unsafe { JS_GetOwnProperty(self.ptr(), &mut desc, quickjs::raw(object), atom.atom) };
        if status < 0 {
            self.clear_exception();
            return None;
        }
        (status > 0).then(|| Descriptor {
            flags: desc.flags,
            value: self.wrap(desc.value),
            getter: self.wrap(desc.getter),
            setter: self.wrap(desc.setter),
        })
    }

    pub fn has_own(self, object: &Value, atom: &Atom) -> Option<bool> {
        let status = unsafe {
            JS_GetOwnProperty(
                self.ptr(),
                core::ptr::null_mut(),
                quickjs::raw(object),
                atom.atom,
            )
        };
        if status < 0 {
            self.clear_exception();
            return None;
        }
        Some(status > 0)
    }

    pub fn define(
        self,
        object: &Value,
        atom: &Atom,
        value: &Value,
        getter: &Value,
        setter: &Value,
        flags: c_int,
    ) {
        let status = unsafe {
            JS_DefineProperty(
                self.ptr(),
                quickjs::raw(object),
                atom.atom,
                quickjs::raw(value),
                quickjs::raw(getter),
                quickjs::raw(setter),
                flags,
            )
        };
        if status < 0 {
            self.clear_exception();
        }
    }

    pub fn define_value(self, object: &Value, atom: &Atom, value: Value, flags: c_int) {
        let status = unsafe {
            JS_DefinePropertyValue(
                self.ptr(),
                quickjs::raw(object),
                atom.atom,
                quickjs::into_raw(value),
                flags,
            )
        };
        if status < 0 {
            self.clear_exception();
        }
    }

    pub fn define_value_str(self, object: &Value, name: &str, value: Value, flags: c_int) {
        let atom = Atom::new(self, &trimmed_c_text(name));
        self.define_value(object, &atom, value, flags);
    }

    pub fn prototype(self, value: &Value) -> Value {
        let raw = unsafe { JS_GetPrototype(self.ptr(), quickjs::raw(value)) };
        self.take(raw).unwrap_or_else(Value::undefined)
    }

    pub fn set_prototype(self, object: &Value, prototype: &Value) {
        let status =
            unsafe { JS_SetPrototype(self.ptr(), quickjs::raw(object), quickjs::raw(prototype)) };
        if status < 0 {
            self.clear_exception();
        }
    }

    pub fn is_function(self, value: &Value) -> bool {
        unsafe { JS_IsFunction(self.ptr(), quickjs::raw(value)) }
    }

    pub fn is_constructor(self, value: &Value) -> bool {
        unsafe { JS_IsConstructor(self.ptr(), quickjs::raw(value)) }
    }

    pub fn to_bool(self, value: &Value) -> bool {
        let truth = unsafe { JS_ToBool(self.ptr(), quickjs::raw(value)) };
        if truth < 0 {
            self.clear_exception();
        }
        truth > 0
    }

    pub fn to_int32(self, value: &Value) -> i32 {
        self.enter(|scope| scope.to_int32(value)).unwrap_or(0)
    }

    pub fn to_string(self, value: &Value) -> Option<String> {
        self.enter(|scope| scope.to_string(value)).ok()
    }

    pub fn clone_c_function(self, function: &Value) -> Option<Value> {
        let raw = unsafe { JS_CloneCFunction(self.ptr(), quickjs::raw(function)) };
        self.take(raw)
    }

    pub fn new_forwarder(
        self,
        target: &Value,
        name: &str,
        length: i32,
        constructor: bool,
    ) -> Option<Value> {
        let name = trimmed_c_text(name);
        let raw = unsafe {
            JS_NewForwarder(
                self.ptr(),
                quickjs::raw(target),
                name.as_ptr(),
                length,
                constructor,
            )
        };
        self.take(raw)
    }

    pub fn class_count(self) -> u32 {
        unsafe { JS_GetClassCount(JS_GetRuntime(self.ptr())) }.max(0) as u32
    }

    pub fn class_proto(self, class: u32) -> Value {
        self.wrap(unsafe { JS_GetClassProto(self.ptr(), class) })
    }

    pub fn set_class_proto(self, class: u32, prototype: Value) {
        unsafe { JS_SetClassProto(self.ptr(), class, quickjs::into_raw(prototype)) };
    }

    pub fn new_object_class(self, class: u32) -> Option<Value> {
        let raw = unsafe { JS_NewObjectClass(self.ptr(), class as c_int) };
        self.take(raw)
    }

    pub fn new_object_proto(self, prototype: &Value) -> Option<Value> {
        let raw = unsafe { JS_NewObjectProto(self.ptr(), quickjs::raw(prototype)) };
        self.take(raw)
    }

    pub fn new_object(self) -> Value {
        self.enter(|scope| scope.new_object())
    }

    pub fn eval_hidden(self, source: &str, name: &CStr) -> Option<Value> {
        let input = CString::new(source).unwrap_or_default();
        let raw = unsafe {
            JS_Eval(
                self.ptr(),
                input.as_ptr(),
                source.len(),
                name.as_ptr(),
                EVAL_TYPE_GLOBAL | EVAL_FLAG_HIDE_SOURCE,
            )
        };
        self.take(raw)
    }

    pub fn call(self, function: &Value, this: &Value, args: &[Value]) -> Option<Value> {
        self.enter(|scope| scope.call(function, this, args)).ok()
    }

    pub fn property_key(self, key: &Value) -> Option<PropertyKey> {
        let ctx = self.ptr();
        let atom = unsafe { JS_ValueToAtom(ctx, quickjs::raw(key)) };
        if atom == 0 {
            return None;
        }
        let atom = Atom { ctx: self, atom };
        let mut index = 0u32;
        if unsafe { JS_AtomIsArrayIndex(ctx, &mut index, atom.atom) } {
            return Some(PropertyKey::Index(index));
        }
        Some(PropertyKey::Name(atom.name().unwrap_or_default()))
    }
}
