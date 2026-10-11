//! Southstar — host classes on QuickJS: class registration, instances with Rust data, and the exotic-method trampolines that run a class's hooks.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::any::Any;
use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use core::sync::atomic::{AtomicPtr, AtomicU32, Ordering};

use super::{
    HostData, HostTrace, JS_DupValue, JS_FreeAtom, JS_GetOpaque, JS_GetRuntime,
    JS_IsRegisteredClass, JS_MarkValue, JS_NewClass, JS_NewObjectClass, JS_NewObjectProtoClass,
    JS_SetOpaque, JS_Throw, JSAtom, JSClassDef, JSContext, JSPropertyDescriptor, JSPropertyEnum,
    JSRuntime, JSValue, PROP_CONFIGURABLE, PROP_ENUMERABLE, PROP_GETSET, PROP_WRITABLE, Scope,
    TAG_OBJECT, Value, trace_as,
};
use crate::{HostClass, OwnProperty, Trace};

const MAX_CLASSES: usize = 4096;
const FIRST_CLASS_ID: u32 = 192;
const PROP_NO_EXOTIC: c_int = 1 << 17;

static NEXT_CLASS_ID: AtomicU32 = AtomicU32::new(FIRST_CLASS_ID);
static CLASSES: [AtomicPtr<HostClass>; MAX_CLASSES] =
    [const { AtomicPtr::new(ptr::null_mut()) }; MAX_CLASSES];
static EXOTICS: [AtomicPtr<JSClassExoticMethods>; MAX_CLASSES] =
    [const { AtomicPtr::new(ptr::null_mut()) }; MAX_CLASSES];

type GetOwnFn = unsafe extern "C" fn(
    ctx: *mut JSContext,
    desc: *mut JSPropertyDescriptor,
    obj: JSValue,
    prop: JSAtom,
) -> c_int;
type OwnNamesFn = unsafe extern "C" fn(
    ctx: *mut JSContext,
    ptab: *mut *mut JSPropertyEnum,
    plen: *mut u32,
    obj: JSValue,
) -> c_int;
type DeleteFn = unsafe extern "C" fn(ctx: *mut JSContext, obj: JSValue, prop: JSAtom) -> c_int;
type DefineFn = unsafe extern "C" fn(
    ctx: *mut JSContext,
    obj: JSValue,
    prop: JSAtom,
    val: JSValue,
    getter: JSValue,
    setter: JSValue,
    flags: c_int,
) -> c_int;
type SetFn = unsafe extern "C" fn(
    ctx: *mut JSContext,
    obj: JSValue,
    prop: JSAtom,
    value: JSValue,
    receiver: JSValue,
    flags: c_int,
) -> c_int;
type GetOwnReceiverFn = unsafe extern "C" fn(
    ctx: *mut JSContext,
    desc: *mut JSPropertyDescriptor,
    obj: JSValue,
    prop: JSAtom,
    receiver: JSValue,
) -> c_int;

#[repr(C)]
pub(super) struct JSClassExoticMethods {
    get_own_property: Option<GetOwnFn>,
    get_own_property_names: Option<OwnNamesFn>,
    delete_property: Option<DeleteFn>,
    define_own_property: Option<DefineFn>,
    has_property: *const c_void,
    get_property: *const c_void,
    set_property: Option<SetFn>,
    get_own_property_receiver: Option<GetOwnReceiverFn>,
}

unsafe extern "C" {
    fn JS_GetClassID(v: JSValue) -> u32;
    fn JS_SetClassProto(ctx: *mut JSContext, class_id: u32, obj: JSValue);
    fn JS_GetClassProto(ctx: *mut JSContext, class_id: u32) -> JSValue;
    fn JS_NewAtomLen(ctx: *mut JSContext, str: *const c_char, len: usize) -> JSAtom;
    fn JS_DupAtom(ctx: *mut JSContext, v: JSAtom) -> JSAtom;
    fn JS_AtomToValue(ctx: *mut JSContext, atom: JSAtom) -> JSValue;
    fn JS_AtomToCStringLen(ctx: *mut JSContext, plen: *mut usize, atom: JSAtom) -> *const c_char;
    fn JS_FreeCString(ctx: *mut JSContext, ptr: *const c_char);
    fn JS_DefineProperty(
        ctx: *mut JSContext,
        this_obj: JSValue,
        prop: JSAtom,
        val: JSValue,
        getter: JSValue,
        setter: JSValue,
        flags: c_int,
    ) -> c_int;
    fn js_malloc(ctx: *mut JSContext, size: usize) -> *mut c_void;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_new_class_id(slot: *mut u32) -> u32 {
    let slot = unsafe { AtomicU32::from_ptr(slot) };
    allocate_id(slot)
}

fn allocate_id(slot: &AtomicU32) -> u32 {
    let id = slot.load(Ordering::Acquire);
    if id != 0 {
        return id;
    }
    let fresh = NEXT_CLASS_ID.fetch_add(1, Ordering::AcqRel);
    match slot.compare_exchange(0, fresh, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => fresh,
        Err(existing) => existing,
    }
}

pub struct PropertyKey {
    ctx: *mut JSContext,
    atom: JSAtom,
    owned: bool,
}

impl PropertyKey {
    fn borrowed(ctx: *mut JSContext, atom: JSAtom) -> PropertyKey {
        PropertyKey {
            ctx,
            atom,
            owned: false,
        }
    }

    pub fn is_symbol(&self, scope: &mut Scope<'_>) -> bool {
        self.to_value(scope).is_symbol()
    }

    pub fn to_value(&self, scope: &mut Scope<'_>) -> Value {
        Value::own(scope.ctx, unsafe { JS_AtomToValue(scope.ctx, self.atom) })
    }

    pub fn to_c_bytes(&self, scope: &mut Scope<'_>) -> Option<Vec<u8>> {
        let text = unsafe { JS_AtomToCStringLen(scope.ctx, ptr::null_mut(), self.atom) };
        if text.is_null() {
            return None;
        }
        let bytes = unsafe { core::ffi::CStr::from_ptr(text) }
            .to_bytes()
            .to_vec();
        unsafe { JS_FreeCString(scope.ctx, text) };
        Some(bytes)
    }

    pub fn raw_atom(&self) -> u32 {
        self.atom
    }
}

impl Clone for PropertyKey {
    fn clone(&self) -> PropertyKey {
        PropertyKey {
            ctx: self.ctx,
            atom: unsafe { JS_DupAtom(self.ctx, self.atom) },
            owned: true,
        }
    }
}

impl Drop for PropertyKey {
    fn drop(&mut self) {
        if self.owned {
            unsafe { JS_FreeAtom(self.ctx, self.atom) };
        }
    }
}

pub struct PropertyDefinition {
    pub value: Value,
    pub getter: Value,
    pub setter: Value,
    flags: c_int,
}

impl PropertyDefinition {
    pub fn has_get(&self) -> bool {
        self.flags & super::PROP_HAS_GET != 0
    }

    pub fn has_set(&self) -> bool {
        self.flags & super::PROP_HAS_SET != 0
    }

    pub fn has_value(&self) -> bool {
        self.flags & super::PROP_HAS_VALUE != 0
    }
}

fn class_of(obj: JSValue) -> Option<&'static HostClass> {
    if obj.tag != TAG_OBJECT {
        return None;
    }
    let id = unsafe { JS_GetClassID(obj) } as usize;
    let class = CLASSES.get(id)?.load(Ordering::Acquire);
    unsafe { class.as_ref() }
}

fn borrow(ctx: *mut JSContext, raw: JSValue) -> Value {
    Value::own(ctx, unsafe { JS_DupValue(ctx, raw) })
}

fn throw(ctx: *mut JSContext, error: Value) -> c_int {
    unsafe { JS_Throw(ctx, error.into_raw()) };
    -1
}

fn fill(desc: *mut JSPropertyDescriptor, property: OwnProperty) {
    let Some(desc) = (unsafe { desc.as_mut() }) else {
        return;
    };
    desc.flags = (if property.configurable {
        PROP_CONFIGURABLE
    } else {
        0
    }) | (if property.enumerable {
        PROP_ENUMERABLE
    } else {
        0
    }) | (if property.writable { PROP_WRITABLE } else { 0 })
        | (if property.accessor { PROP_GETSET } else { 0 });
    desc.value = property.value.into_raw();
    desc.getter = property.getter.into_raw();
    desc.setter = property.setter.into_raw();
}

fn found(
    ctx: *mut JSContext,
    desc: *mut JSPropertyDescriptor,
    result: Result<Option<OwnProperty>, Value>,
) -> c_int {
    match result {
        Ok(None) => 0,
        Ok(Some(property)) => {
            fill(desc, property);
            1
        }
        Err(error) => throw(ctx, error),
    }
}

fn status(ctx: *mut JSContext, result: Result<bool, Value>) -> c_int {
    match result {
        Ok(done) => c_int::from(done),
        Err(error) => throw(ctx, error),
    }
}

unsafe extern "C" fn get_own_property(
    ctx: *mut JSContext,
    desc: *mut JSPropertyDescriptor,
    obj: JSValue,
    prop: JSAtom,
) -> c_int {
    let Some(hook) = class_of(obj).and_then(|class| class.hooks.get_own_property) else {
        return 0;
    };
    let object = borrow(ctx, obj);
    let key = PropertyKey::borrowed(ctx, prop);
    let result = hook(&mut Scope::of(ctx), &object, &key);
    found(ctx, desc, result)
}

unsafe extern "C" fn get_own_property_receiver(
    ctx: *mut JSContext,
    desc: *mut JSPropertyDescriptor,
    obj: JSValue,
    prop: JSAtom,
    receiver: JSValue,
) -> c_int {
    let Some(hook) = class_of(obj).and_then(|class| class.hooks.get_own_property_receiver) else {
        return 0;
    };
    let object = borrow(ctx, obj);
    let receiver = borrow(ctx, receiver);
    let key = PropertyKey::borrowed(ctx, prop);
    let result = hook(&mut Scope::of(ctx), &object, &key, &receiver);
    found(ctx, desc, result)
}

unsafe extern "C" fn get_own_property_names(
    ctx: *mut JSContext,
    ptab: *mut *mut JSPropertyEnum,
    plen: *mut u32,
    obj: JSValue,
) -> c_int {
    unsafe {
        *ptab = ptr::null_mut();
        *plen = 0;
    }
    let Some(hook) = class_of(obj).and_then(|class| class.hooks.own_keys) else {
        return 0;
    };
    let object = borrow(ctx, obj);
    let keys = match hook(&mut Scope::of(ctx), &object) {
        Ok(keys) => keys,
        Err(error) => return throw(ctx, error),
    };
    if keys.is_empty() {
        return 0;
    }
    let tab = unsafe { js_malloc(ctx, size_of::<JSPropertyEnum>() * keys.len()) }
        .cast::<JSPropertyEnum>();
    if tab.is_null() {
        return -1;
    }
    let len = keys.len();
    for (i, key) in keys.into_iter().enumerate() {
        let atom = if key.owned {
            let atom = key.atom;
            core::mem::forget(key);
            atom
        } else {
            unsafe { JS_DupAtom(ctx, key.atom) }
        };
        unsafe {
            tab.add(i).write(JSPropertyEnum {
                is_enumerable: 1,
                atom,
            })
        };
    }
    unsafe {
        *ptab = tab;
        *plen = len as u32;
    }
    0
}

unsafe extern "C" fn delete_property(ctx: *mut JSContext, obj: JSValue, prop: JSAtom) -> c_int {
    let Some(hook) = class_of(obj).and_then(|class| class.hooks.delete) else {
        return 1;
    };
    let object = borrow(ctx, obj);
    let key = PropertyKey::borrowed(ctx, prop);
    let result = hook(&mut Scope::of(ctx), &object, &key);
    status(ctx, result)
}

unsafe extern "C" fn define_own_property(
    ctx: *mut JSContext,
    obj: JSValue,
    prop: JSAtom,
    val: JSValue,
    getter: JSValue,
    setter: JSValue,
    flags: c_int,
) -> c_int {
    let object = borrow(ctx, obj);
    let definition = PropertyDefinition {
        value: borrow(ctx, val),
        getter: borrow(ctx, getter),
        setter: borrow(ctx, setter),
        flags,
    };
    let key = PropertyKey::borrowed(ctx, prop);
    let mut scope = Scope::of(ctx);
    let result = match class_of(obj).and_then(|class| class.hooks.define_own_property) {
        Some(hook) => hook(&mut scope, &object, &key, &definition),
        None => scope.define_ordinary(&object, &key, &definition),
    };
    status(ctx, result)
}

unsafe extern "C" fn set_property(
    ctx: *mut JSContext,
    obj: JSValue,
    prop: JSAtom,
    value: JSValue,
    receiver: JSValue,
    _flags: c_int,
) -> c_int {
    let Some(hook) = class_of(obj).and_then(|class| class.hooks.set) else {
        return 0;
    };
    let object = borrow(ctx, obj);
    let value = borrow(ctx, value);
    let receiver = borrow(ctx, receiver);
    let key = PropertyKey::borrowed(ctx, prop);
    let result = hook(&mut Scope::of(ctx), &object, &key, &value, &receiver);
    status(ctx, result)
}

unsafe extern "C" fn finalize(_rt: *mut JSRuntime, val: JSValue) {
    let id = unsafe { JS_GetClassID(val) };
    let data = unsafe { JS_GetOpaque(val, id) };
    if !data.is_null() {
        drop(unsafe { Box::from_raw(data.cast::<HostData>()) });
    }
}

unsafe extern "C" fn mark(rt: *mut JSRuntime, val: JSValue, mark_func: *const c_void) {
    let id = unsafe { JS_GetClassID(val) };
    let data = unsafe { JS_GetOpaque(val, id) }.cast::<HostData>();
    let Some(host) = (unsafe { data.as_ref() }) else {
        return;
    };
    if let Some(trace) = host.trace {
        trace(&*host.data, &mut |value| unsafe {
            JS_MarkValue(rt, value.raw, mark_func)
        });
    }
}

fn exotic_for(class: &HostClass) -> *const JSClassExoticMethods {
    let id = class.assigned_id() as usize;
    let cached = EXOTICS[id].load(Ordering::Acquire);
    if !cached.is_null() {
        return cached;
    }
    let hooks = &class.hooks;
    let methods = Box::new(JSClassExoticMethods {
        get_own_property: hooks.get_own_property.map(|_| get_own_property as GetOwnFn),
        get_own_property_names: hooks.own_keys.map(|_| get_own_property_names as OwnNamesFn),
        delete_property: hooks.delete.map(|_| delete_property as DeleteFn),
        define_own_property: hooks
            .define_own_property
            .map(|_| define_own_property as DefineFn),
        has_property: ptr::null(),
        get_property: ptr::null(),
        set_property: hooks.set.map(|_| set_property as SetFn),
        get_own_property_receiver: hooks
            .get_own_property_receiver
            .map(|_| get_own_property_receiver as GetOwnReceiverFn),
    });
    let fresh = Box::into_raw(methods);
    match EXOTICS[id].compare_exchange(ptr::null_mut(), fresh, Ordering::AcqRel, Ordering::Acquire)
    {
        Ok(_) => fresh,
        Err(existing) => {
            drop(unsafe { Box::from_raw(fresh) });
            existing
        }
    }
}

pub fn class_id(class: &HostClass) -> u32 {
    class.assigned_id()
}

impl Scope<'_> {
    pub fn register_class(&mut self, class: &'static HostClass) {
        let id = allocate_id(class.id_slot());
        if let Some(slot) = CLASSES.get(id as usize) {
            slot.store(ptr::from_ref(class).cast_mut(), Ordering::Release);
        }
        let rt = unsafe { JS_GetRuntime(self.ctx) };
        if unsafe { JS_IsRegisteredClass(rt, id) } {
            return;
        }
        let def = JSClassDef {
            class_name: class.name.as_ptr(),
            finalizer: Some(finalize),
            gc_mark: Some(mark),
            call: ptr::null(),
            exotic: exotic_for(class).cast(),
        };
        unsafe { JS_NewClass(rt, id, &def) };
    }

    pub fn set_class_prototype(&mut self, class: &HostClass, prototype: Value) {
        unsafe { JS_SetClassProto(self.ctx, class.assigned_id(), prototype.into_raw()) };
    }

    pub fn class_prototype(&mut self, class: &HostClass) -> Value {
        Value::own(self.ctx, unsafe {
            JS_GetClassProto(self.ctx, class.assigned_id())
        })
    }

    fn instance(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: Option<HostData>,
    ) -> Result<Value, Value> {
        let id = class.assigned_id();
        let raw = unsafe {
            match prototype {
                Some(prototype) => JS_NewObjectProtoClass(self.ctx, prototype.raw, id),
                None => JS_NewObjectClass(self.ctx, id as c_int),
            }
        };
        let object = self.take(raw)?;
        if let Some(data) = data {
            unsafe { JS_SetOpaque(object.raw, Box::into_raw(Box::new(data)).cast()) };
        }
        Ok(object)
    }

    pub fn new_instance(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
    ) -> Result<Value, Value> {
        self.instance(class, prototype, None)
    }

    pub fn new_instance_with<T: Any>(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: T,
    ) -> Result<Value, Value> {
        let host = HostData {
            data: Box::new(data),
            trace: None,
        };
        self.instance(class, prototype, Some(host))
    }

    pub fn new_traced_instance<T: Any + Trace>(
        &mut self,
        class: &HostClass,
        prototype: Option<&Value>,
        data: T,
    ) -> Result<Value, Value> {
        let host = HostData {
            data: Box::new(data),
            trace: Some(trace_as::<T> as HostTrace),
        };
        self.instance(class, prototype, Some(host))
    }

    pub fn property_key(&mut self, name: &[u8]) -> PropertyKey {
        PropertyKey {
            ctx: self.ctx,
            atom: unsafe { JS_NewAtomLen(self.ctx, name.as_ptr().cast(), name.len()) },
            owned: true,
        }
    }

    pub fn define_ordinary(
        &mut self,
        object: &Value,
        key: &PropertyKey,
        definition: &PropertyDefinition,
    ) -> Result<bool, Value> {
        let status = unsafe {
            JS_DefineProperty(
                self.ctx,
                object.raw,
                key.atom,
                definition.value.raw,
                definition.getter.raw,
                definition.setter.raw,
                definition.flags | PROP_NO_EXOTIC,
            )
        };
        if status < 0 {
            Err(self.exception())
        } else {
            Ok(status != 0)
        }
    }
}

impl Value {
    pub fn with_instance<T: Any, R>(
        &self,
        class: &HostClass,
        f: impl FnOnce(&T) -> R,
    ) -> Option<R> {
        let id = class.assigned_id();
        if id == 0 || self.raw.tag != TAG_OBJECT {
            return None;
        }
        let data = unsafe { JS_GetOpaque(self.raw, id) }.cast::<HostData>();
        unsafe { data.as_ref() }.and_then(|host| host.data.downcast_ref::<T>().map(f))
    }

    pub fn is_instance_of_class(&self, class: &HostClass) -> bool {
        let id = class.assigned_id();
        id != 0 && self.raw.tag == TAG_OBJECT && unsafe { JS_GetClassID(self.raw) } == id
    }
}
