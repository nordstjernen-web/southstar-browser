//! Southstar — the realm cloner: gives a frame realm its own copies of the page realm's platform objects, functions and prototypes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use southstar_js_engine::Value;
use southstar_js_engine::quickjs;

use crate::ffi::qjs::{
    Atom, Ctx, Descriptor, GPN_ENUM_ONLY, GPN_PRIVATE, GPN_STRING, GPN_SYMBOL, PROP_C_W_E,
    PROP_CONFIGURABLE, PROP_ENUMERABLE, PROP_HAS_CONFIGURABLE, PROP_HAS_ENUMERABLE, PROP_HAS_GET,
    PROP_HAS_SET, PROP_HAS_VALUE, PROP_HAS_WRITABLE, PROP_WRITABLE, class_id, opaque, set_opaque,
};
use crate::ffi::{self, Js};
use crate::page;

const INTRINSIC_NAMES: &[&str] = &[
    "Object",
    "Function",
    "Array",
    "Number",
    "Boolean",
    "String",
    "Symbol",
    "BigInt",
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "AggregateError",
    "Promise",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "WeakRef",
    "FinalizationRegistry",
    "ArrayBuffer",
    "SharedArrayBuffer",
    "DataView",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float16Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    "Date",
    "RegExp",
    "Proxy",
    "Reflect",
    "JSON",
    "Math",
    "Atomics",
    "Iterator",
    "DOMException",
];

const INTRINSIC_PROBE: &str = "(function(){ var a = [][Symbol.iterator](); var g = function*(){}; var ag = async function*(){}; var af = async function(){}; return [Object.getPrototypeOf(a), Object.getPrototypeOf(Object.getPrototypeOf(a)), Object.getPrototypeOf(g), Object.getPrototypeOf(ag), Object.getPrototypeOf(af), Object.getPrototypeOf(new Map()[Symbol.iterator]()), Object.getPrototypeOf(new Set()[Symbol.iterator]()), Object.getPrototypeOf(''[Symbol.iterator]())]; })()";

const SINGLETON_NAMES: &[&str] = &[
    "navigator",
    "screen",
    "crypto",
    "localStorage",
    "sessionStorage",
    "caches",
    "indexedDB",
    "cookieStore",
    "trustedTypes",
    "visualViewport",
    "scheduler",
    "navigation",
    "external",
    "locationbar",
    "menubar",
    "personalbar",
    "scrollbars",
    "statusbar",
    "toolbar",
    "console",
    "speechSynthesis",
    "styleMedia",
    "customElements",
];

const PLAIN_OBJECT_CLASS: u32 = 1;
const MAX_DEPTH: u32 = 32;

pub(crate) type Shared = Rc<RefCell<Cloner>>;

pub(crate) struct Cloner {
    src: Ctx,
    dst: Ctx,
    memo: HashMap<usize, (Value, Value)>,
}

fn copied_flags(desc: &Descriptor) -> i32 {
    PROP_HAS_CONFIGURABLE
        | PROP_HAS_ENUMERABLE
        | (desc.flags & (PROP_CONFIGURABLE | PROP_ENUMERABLE))
}

fn data_flags(desc: &Descriptor) -> i32 {
    PROP_HAS_VALUE | PROP_HAS_WRITABLE | (desc.flags & PROP_WRITABLE)
}

impl Cloner {
    fn new(src: Ctx, main: Ctx, dst: Ctx) -> Cloner {
        let mut cloner = Cloner {
            src,
            dst,
            memo: HashMap::new(),
        };
        cloner.seed(src);
        if main != src {
            cloner.seed(main);
        }
        if ffi::debug_realm() {
            cloner.debug_report(main);
        }
        cloner
    }

    fn debug_report(&self, main: Ctx) {
        let src = self.src;
        let global = src.global();
        let object = src.get(&global, "Object");
        let object_proto = src.get(&object, "prototype");
        let error = src.get(&global, "Error");
        let error_proto = src.get(&error, "prototype");
        eprintln!(
            "NSREALM memo={} objproto={} errproto={} src==main:{}",
            self.memo.len(),
            self.memo.contains_key(&quickjs::identity(&object_proto)) as i32,
            self.memo.contains_key(&quickjs::identity(&error_proto)) as i32,
            (src == main) as i32
        );
    }

    fn put(&mut self, from: &Value, to: &Value) {
        if !from.is_object() || !to.is_object() {
            return;
        }
        self.memo
            .entry(quickjs::identity(from))
            .or_insert_with(|| (from.clone(), to.clone()));
    }

    fn lookup(&self, from: &Value) -> Option<&Value> {
        if !from.is_object() {
            return None;
        }
        self.memo.get(&quickjs::identity(from)).map(|(_, to)| to)
    }

    fn map_pair(&mut self, from: &Value, to: &Value) {
        self.put(from, to);
        if !from.is_object() || !to.is_object() {
            return;
        }
        let from_proto = self.src.get(from, "prototype");
        let to_proto = self.dst.get(to, "prototype");
        self.put(&from_proto, &to_proto);
    }

    fn map_proto_of(&mut self, from: &Value, to: &Value) {
        let from_proto = self.src.prototype(from);
        let to_proto = self.dst.prototype(to);
        self.map_pair(&from_proto, &to_proto);
    }

    fn seed(&mut self, src: Ctx) {
        let dst = self.dst;
        let src_global = src.global();
        let dst_global = dst.global();
        for name in INTRINSIC_NAMES {
            let from = src.get(&src_global, name);
            let to = dst.get(&dst_global, name);
            self.map_pair(&from, &to);
        }
        let from_u8 = src.get(&src_global, "Uint8Array");
        let to_u8 = dst.get(&dst_global, "Uint8Array");
        if from_u8.is_object() && to_u8.is_object() {
            self.map_proto_of(&from_u8, &to_u8);
        }
        let from_list = src.eval_hidden(INTRINSIC_PROBE, c"<realm-intrinsics>");
        let to_list = dst.eval_hidden(INTRINSIC_PROBE, c"<realm-intrinsics>");
        if let (Some(from_list), Some(to_list)) = (from_list, to_list)
            && from_list.is_array()
            && to_list.is_array()
        {
            let length = src.get(&from_list, "length");
            let count = src.to_int32(&length).max(0) as u32;
            for index in 0..count {
                let from = src
                    .enter(|scope| scope.get_index(&from_list, index))
                    .unwrap_or_else(|_| Value::undefined());
                let to = dst
                    .enter(|scope| scope.get_index(&to_list, index))
                    .unwrap_or_else(|_| Value::undefined());
                self.map_pair(&from, &to);
            }
        }
    }

    fn clone_value(&mut self, value: &Value, depth: u32) -> Value {
        if !value.is_object() {
            return value.clone();
        }
        if let Some(hit) = self.lookup(value) {
            return hit.clone();
        }
        if depth > MAX_DEPTH {
            return value.clone();
        }
        let Some(mut out) = self.dst.clone_c_function(value) else {
            return value.clone();
        };
        if out.is_undefined() {
            if self.src.is_function(value) {
                return self.clone_js_function(value, depth);
            }
            match self.new_shell(value) {
                Some(shell) => out = shell,
                None => return value.clone(),
            }
        }
        self.put(value, &out);
        self.clone_prototype(value, &out, depth);
        self.clone_own_properties(value, &out, depth);
        out
    }

    fn clone_descriptor_into(&mut self, to: &Value, atom: &Atom, desc: &Descriptor, depth: u32) {
        let mut flags = copied_flags(desc);
        let undefined = Value::undefined();
        if desc.is_accessor() {
            let getter = self.clone_value(&desc.getter, depth + 1);
            let setter = self.clone_value(&desc.setter, depth + 1);
            flags |= PROP_HAS_GET | PROP_HAS_SET;
            self.dst
                .define(to, atom, &undefined, &getter, &setter, flags);
        } else {
            let value = self.clone_value(&desc.value, depth + 1);
            flags |= data_flags(desc);
            self.dst
                .define(to, atom, &value, &undefined, &undefined, flags);
        }
    }

    fn clone_own_properties(&mut self, from: &Value, to: &Value, depth: u32) {
        let src = self.src;
        let Some(keys) = src.own_keys(from, GPN_STRING | GPN_SYMBOL) else {
            return;
        };
        for atom in &keys {
            if let Some(desc) = src.own_property(from, atom) {
                self.clone_descriptor_into(to, atom, &desc, depth);
            }
        }
    }

    fn object_is_shape(&self, value: &Value) -> bool {
        let src = self.src;
        let proto = src.prototype(value);
        let mut plain_namespace = proto.is_null();
        if proto.is_object() {
            let dst = self.dst;
            let dst_global = dst.global();
            let dst_object = dst.get(&dst_global, "Object");
            let dst_object_proto = dst.get(&dst_object, "prototype");
            plain_namespace = self
                .lookup(&proto)
                .is_some_and(|mapped| mapped.same_object(&dst_object_proto));
        }
        if plain_namespace {
            return true;
        }
        let constructor = Atom::new(src, c"constructor");
        src.has_own(value, &constructor).unwrap_or(false)
    }

    fn adopt_prototype(&mut self, object: &Value) {
        let proto = self.src.prototype(object);
        if proto.is_object() {
            let cloned = self.clone_value(&proto, 0);
            self.dst.set_prototype(object, &cloned);
        }
    }

    fn adopt_property(&mut self, object: &Value, atom: &Atom) {
        let Some(desc) = self.src.own_property(object, atom) else {
            return;
        };
        let accessor = desc.is_accessor();
        let function = !accessor && self.src.is_function(&desc.value);
        if desc.flags & PROP_CONFIGURABLE != 0 && (accessor || function) {
            self.clone_descriptor_into(object, atom, &desc, 0);
        }
    }

    fn adopt_in_place(&mut self, object: &Value) {
        if !object.is_object() {
            return;
        }
        self.adopt_prototype(object);
        let Some(keys) = self.src.own_keys(object, GPN_STRING | GPN_SYMBOL) else {
            return;
        };
        for atom in &keys {
            self.adopt_property(object, atom);
        }
    }

    fn fn_is_interface(&self, function: &Value) -> bool {
        let src = self.src;
        if !src.is_constructor(function) {
            return false;
        }
        let name = src.get(function, "name");
        if name.is_string()
            && src
                .to_string(&name)
                .is_some_and(|name| name.as_bytes().first().is_some_and(u8::is_ascii_uppercase))
        {
            return true;
        }
        let proto = src.get(function, "prototype");
        proto.is_object()
            && src
                .own_keys(&proto, GPN_STRING | GPN_SYMBOL)
                .is_some_and(|keys| keys.len() > 1)
    }

    fn forwarder_for(&self, function: &Value, constructor: bool) -> Option<Value> {
        let src = self.src;
        let name_value = src.get(function, "name");
        let name = if name_value.is_string() {
            src.to_string(&name_value).unwrap_or_default()
        } else {
            String::new()
        };
        let length_value = src.get(function, "length");
        let length = if length_value.is_number() {
            src.to_int32(&length_value)
        } else {
            0
        };
        self.dst.new_forwarder(function, &name, length, constructor)
    }

    fn ctor_owns_proto(&self, constructor: &Value) -> bool {
        let marker = self.src.get(constructor, "__ndRealmProto");
        self.src.to_bool(&marker)
    }

    fn forwarder_property(
        &mut self,
        function: &Value,
        out: &Value,
        atom: &Atom,
        is_proto: bool,
        depth: u32,
    ) {
        let Some(desc) = self.src.own_property(function, atom) else {
            return;
        };
        if !desc.is_accessor()
            && is_proto
            && desc.value.is_object()
            && !self.ctor_owns_proto(function)
        {
            self.put(&desc.value, &desc.value);
            let flags = copied_flags(&desc) | data_flags(&desc);
            let undefined = Value::undefined();
            self.dst
                .define(out, atom, &desc.value, &undefined, &undefined, flags);
            return;
        }
        self.clone_descriptor_into(out, atom, &desc, depth);
    }

    fn clone_js_function(&mut self, function: &Value, depth: u32) -> Value {
        let constructor = self.fn_is_interface(function);
        let Some(out) = self.forwarder_for(function, constructor) else {
            return function.clone();
        };
        self.put(function, &out);
        let src = self.src;
        let Some(keys) = src.own_keys(function, GPN_STRING | GPN_SYMBOL) else {
            return out;
        };
        let name = Atom::new(src, c"name");
        let length = Atom::new(src, c"length");
        let prototype = Atom::new(src, c"prototype");
        for atom in &keys {
            if atom.same(&name) || atom.same(&length) {
                continue;
            }
            let is_proto = atom.same(&prototype);
            if is_proto && !constructor {
                continue;
            }
            self.forwarder_property(function, &out, atom, is_proto, depth);
        }
        out
    }

    fn clone_prototype(&mut self, value: &Value, out: &Value, depth: u32) {
        let proto = self.src.prototype(value);
        if proto.is_object() {
            let cloned = self.clone_value(&proto, depth + 1);
            self.dst.set_prototype(out, &cloned);
        } else if proto.is_null() {
            self.dst.set_prototype(out, &Value::null());
        }
    }

    fn new_shell(&self, value: &Value) -> Option<Value> {
        let class = class_id(value);
        let named = ffi::window_named_class_id();
        if named != 0 && class == named {
            return self.dst.new_object_class(named);
        }
        if class != PLAIN_OBJECT_CLASS || !self.object_is_shape(value) {
            return None;
        }
        self.dst.new_object_proto(&Value::null())
    }

    fn clone_class_protos(&mut self) {
        let count = self.dst.class_count();
        for class in 1..count {
            if self.dst.class_proto(class).is_object() {
                continue;
            }
            let src_proto = self.src.class_proto(class);
            if src_proto.is_object() {
                let cloned = self.clone_value(&src_proto, 0);
                self.dst.set_class_proto(class, cloned);
            }
        }
    }

    fn clone_storage(&mut self, value: &Value, storage_class: u32) -> Value {
        let Some(out) = self.dst.new_object_class(storage_class) else {
            return value.clone();
        };
        set_opaque(&out, opaque(value, storage_class));
        self.put(value, &out);
        out
    }

    fn instance_is_plain(&self, value: &Value, class: u32, depth: u32) -> bool {
        let object_class = class_id(&self.dst.new_object());
        class == object_class && depth <= 3 && !value.is_array()
    }

    fn clone_instance_properties(&mut self, value: &Value, out: &Value, depth: u32) {
        let src = self.src;
        let Some(keys) = src.own_keys(value, GPN_STRING | GPN_SYMBOL | GPN_PRIVATE) else {
            return;
        };
        let undefined = Value::undefined();
        for atom in &keys {
            let Some(desc) = src.own_property(value, atom) else {
                continue;
            };
            if desc.is_accessor() {
                self.clone_descriptor_into(out, atom, &desc, depth);
            } else {
                let cloned = self.clone_instance(&desc.value, depth + 1);
                let flags = copied_flags(&desc) | data_flags(&desc);
                self.dst
                    .define(out, atom, &cloned, &undefined, &undefined, flags);
            }
        }
    }

    fn clone_plain_instance(&mut self, value: &Value, depth: u32) -> Value {
        let proto = self.src.prototype(value);
        let cloned_proto = if proto.is_object() {
            self.clone_value(&proto, 0)
        } else {
            proto
        };
        let Some(out) = self.dst.new_object_proto(&cloned_proto) else {
            return value.clone();
        };
        self.put(value, &out);
        self.clone_instance_properties(value, &out, depth);
        out
    }

    fn clone_instance(&mut self, value: &Value, depth: u32) -> Value {
        if !value.is_object() || self.src.is_function(value) {
            return self.clone_value(value, depth);
        }
        if let Some(hit) = self.lookup(value) {
            return hit.clone();
        }
        let class = class_id(value);
        let storage = ffi::storage_class_id();
        if storage != 0 && class == storage {
            return self.clone_storage(value, storage);
        }
        if !self.instance_is_plain(value, class, depth) {
            return value.clone();
        }
        self.clone_plain_instance(value, depth)
    }

    fn adopt_navigator_value(&mut self, mine: &Value, atom: &Atom, values: &Value) {
        let value = self.src.get_atom(mine, atom);
        if !value.is_object() {
            return;
        }
        let own = self.clone_instance(&value, 0);
        if let Some(name) = atom.name()
            && own.is_object()
        {
            self.dst.set(values, &name.to_string_lossy(), own);
        }
    }

    fn adopt_navigator_objects(&mut self, brand: &Value, frame_nav: &Value) {
        let src = self.src;
        let get = src.get(brand, "navigatorObjects");
        let adopt = src.get(brand, "adoptNavigatorObjects");
        let mine = if src.is_function(&get) {
            src.call(&get, brand, &[]).unwrap_or_else(Value::undefined)
        } else {
            Value::undefined()
        };
        if !mine.is_object() || !src.is_function(&adopt) {
            return;
        }
        let Some(keys) = src.own_keys(&mine, GPN_STRING | GPN_ENUM_ONLY) else {
            return;
        };
        let Some(values) = self.dst.new_object_proto(&Value::null()) else {
            return;
        };
        for atom in &keys {
            self.adopt_navigator_value(&mine, atom, &values);
        }
        let _ = src.call(&adopt, brand, &[frame_nav.clone(), values]);
    }

    fn install_singleton(&mut self, parent_global: &Value, frame_global: &Value, name: &str) {
        let dst = self.dst;
        let atom = Atom::new(dst, &std::ffi::CString::new(name).unwrap_or_default());
        let Some(desc) = dst.own_property(frame_global, &atom) else {
            return;
        };
        if desc.is_accessor() || !desc.value.is_object() {
            return;
        }
        let parent_value = self.src.get(parent_global, name);
        if !parent_value.is_object() || !parent_value.same_object(&desc.value) {
            return;
        }
        let own = self.clone_instance(&desc.value, 0);
        let flags = PROP_HAS_VALUE
            | PROP_HAS_WRITABLE
            | PROP_HAS_CONFIGURABLE
            | PROP_HAS_ENUMERABLE
            | (desc.flags & (PROP_WRITABLE | PROP_CONFIGURABLE | PROP_ENUMERABLE));
        let undefined = Value::undefined();
        dst.define(frame_global, &atom, &own, &undefined, &undefined, flags);
    }

    fn register_navigator(&mut self, nav: &Value) {
        let js = Js::of_ctx(self.dst);
        if js.is_null() || !nav.is_object() {
            return;
        }
        let brand = ffi::navigator_brand(js, self.src);
        if !brand.is_object() {
            return;
        }
        let src = self.src;
        let add = src.get(&brand, "add");
        if src.is_function(&add) {
            let _ = src.call(&add, &brand, core::slice::from_ref(nav));
        }
        self.adopt_navigator_objects(&brand, nav);
    }

    fn client_information(&self, frame_global: &Value, nav: &Value) {
        let dst = self.dst;
        let atom = Atom::new(dst, c"clientInformation");
        if dst.has_own(frame_global, &atom) == Some(true) && nav.is_object() {
            dst.define_value(frame_global, &atom, nav.clone(), PROP_C_W_E);
        }
    }

    fn install_singletons(&mut self, parent_global: &Value, frame_global: &Value) {
        for name in SINGLETON_NAMES {
            self.install_singleton(parent_global, frame_global, name);
        }
        let nav = self.dst.get(frame_global, "navigator");
        self.register_navigator(&nav);
        self.client_information(frame_global, &nav);
    }
}

fn lookup(js: Js, dst: Ctx) -> Option<Shared> {
    if js.is_null() {
        return None;
    }
    page::peek(js, |page| page.cloners.get(&dst).cloned()).flatten()
}

fn cloner_for(js: Js, dst: Ctx) -> Shared {
    if let Some(found) = lookup(js, dst) {
        return found;
    }
    let main = ffi::main_realm(js);
    let cloner = Rc::new(RefCell::new(Cloner::new(main, main, dst)));
    let replaced = page::with(js, |page| {
        page.cloners_made = true;
        page.cloners.insert(dst, cloner.clone())
    });
    drop(replaced);
    cloner
}

pub(crate) fn prepare_frame_realm(js: Js, dst: Ctx, document: &Value) {
    let cloner = cloner_for(js, dst);
    if let Ok(mut cloner) = cloner.try_borrow_mut() {
        cloner.clone_class_protos();
        cloner.adopt_in_place(document);
    }
}

pub(crate) fn install_singletons(js: Js, dst: Ctx, parent_global: &Value, frame_global: &Value) {
    let cloner = cloner_for(js, dst);
    if let Ok(mut cloner) = cloner.try_borrow_mut() {
        cloner.install_singletons(parent_global, frame_global);
    }
}

pub(crate) fn clone_into(js: Js, dst: Ctx, value: &Value) -> Value {
    let Some(cloner) = lookup(js, dst) else {
        return value.clone();
    };
    match cloner.try_borrow_mut() {
        Ok(mut cloner) => cloner.clone_value(value, 0),
        Err(_) => value.clone(),
    }
}

pub(crate) fn proto_for(js: Js, realm: Ctx, proto: &Value) -> Option<Value> {
    let cloner = lookup(js, realm)?;
    if !proto.is_object() {
        return None;
    }
    let mut cloner = cloner.try_borrow_mut().ok()?;
    if let Some(hit) = cloner.lookup(proto) {
        return Some(hit.clone());
    }
    drop(cloner.clone_value(proto, 0));
    cloner.lookup(proto).cloned()
}

pub(crate) fn clear(js: Js) -> Vec<Shared> {
    page::peek_mut(js, |page| page.cloners.drain().map(|(_, c)| c).collect()).unwrap_or_default()
}
