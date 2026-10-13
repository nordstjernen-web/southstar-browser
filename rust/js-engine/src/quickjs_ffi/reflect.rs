//! Southstar — the object-model calls that copy one realm's objects into another on QuickJS: every own key, full property definitions, native-function clones, forwarders and the plain-object test.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};
use std::ffi::CString;

use super::{
    ATOM_NULL, GPN_STRING_MASK, GPN_SYMBOL_MASK, JS_AtomToValue, JS_FreeAtom, JS_FreePropertyEnum,
    JS_GetOwnPropertyNames, JS_ValueToAtom, JSAtom, JSContext, JSPropertyEnum, JSValue,
    PROP_CONFIGURABLE, PROP_ENUMERABLE, PROP_HAS_GET, PROP_HAS_SET, PROP_HAS_VALUE, PROP_WRITABLE,
    Scope, TAG_OBJECT, UNDEFINED, Value,
};
use crate::{HostClass, PropertyDescriptor};

const GPN_PRIVATE_MASK: c_int = 1 << 2;
const PROP_HAS_CONFIGURABLE: c_int = 1 << 8;
const PROP_HAS_WRITABLE: c_int = 1 << 9;
const PROP_HAS_ENUMERABLE: c_int = 1 << 10;
const CLASS_OBJECT: u32 = 1;

unsafe extern "C" {
    fn JS_DefineProperty(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: JSAtom,
        val: JSValue,
        getter: JSValue,
        setter: JSValue,
        flags: c_int,
    ) -> c_int;
    fn JS_CloneCFunction(ctx: *mut JSContext, func: JSValue) -> JSValue;
    fn JS_NewForwarder(
        ctx: *mut JSContext,
        target: JSValue,
        name: *const c_char,
        length: c_int,
        constructor: bool,
    ) -> JSValue;
    fn JS_GetClassID(v: JSValue) -> u32;
}

impl Scope<'_> {
    pub fn own_property_keys_with_private(&mut self, object: &Value) -> Result<Vec<Value>, Value> {
        let mut tab: *mut JSPropertyEnum = core::ptr::null_mut();
        let mut len = 0u32;
        let flags = GPN_STRING_MASK | GPN_SYMBOL_MASK | GPN_PRIVATE_MASK;
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

    pub fn define_property(
        &mut self,
        object: &Value,
        key: &Value,
        descriptor: &PropertyDescriptor,
    ) -> Result<(), Value> {
        let atom = unsafe { JS_ValueToAtom(self.ctx, key.raw) };
        if atom == ATOM_NULL {
            return Err(self.exception());
        }
        let mut flags = PROP_HAS_CONFIGURABLE | PROP_HAS_ENUMERABLE;
        if descriptor.configurable {
            flags |= PROP_CONFIGURABLE;
        }
        if descriptor.enumerable {
            flags |= PROP_ENUMERABLE;
        }
        let (value, getter, setter) = if descriptor.accessor {
            flags |= PROP_HAS_GET | PROP_HAS_SET;
            (UNDEFINED, descriptor.getter.raw, descriptor.setter.raw)
        } else {
            flags |= PROP_HAS_VALUE | PROP_HAS_WRITABLE;
            if descriptor.writable {
                flags |= PROP_WRITABLE;
            }
            (descriptor.value.raw, UNDEFINED, UNDEFINED)
        };
        let status =
            unsafe { JS_DefineProperty(self.ctx, object.raw, atom, value, getter, setter, flags) };
        unsafe { JS_FreeAtom(self.ctx, atom) };
        self.status(status)
    }

    pub fn clone_native_function(&mut self, function: &Value) -> Result<Value, Value> {
        let raw = unsafe { JS_CloneCFunction(self.ctx, function.raw) };
        self.take(raw)
    }

    pub fn new_forwarder(
        &mut self,
        target: &Value,
        name: &str,
        length: i32,
        constructor: bool,
    ) -> Result<Value, Value> {
        let name = CString::new(name.split('\0').next().unwrap_or_default()).unwrap_or_default();
        let raw =
            unsafe { JS_NewForwarder(self.ctx, target.raw, name.as_ptr(), length, constructor) };
        self.take(raw)
    }

    pub fn registered_classes(&self) -> Vec<&'static HostClass> {
        super::class::registered()
    }

    pub fn is_plain_object(&self, value: &Value) -> bool {
        value.raw.tag == TAG_OBJECT && unsafe { JS_GetClassID(value.raw) } == CLASS_OBJECT
    }
}
