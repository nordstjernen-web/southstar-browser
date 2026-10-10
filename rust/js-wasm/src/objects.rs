//! Southstar — WebAssembly.Module, Instance, Memory, Table and Global, and the exported functions that call into wasm.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::rc::Rc;

use southstar_js_engine::{Attributes, Scope, Trace, Value};
use wasmi::{
    Caller, Extern, ExternType, Func, FuncType, Global, Instance, Memory, MemoryType, Module,
    Mutability, Ref, RefType, StoreContextMut, Table, TableType, Val, ValType,
};

use crate::convert::{self, Raw};
use crate::ffi;
use crate::realm::{HostState, MEMORY_MAX_BYTES, PAGE_SIZE, Realm, RealmHandle, engine};
use crate::sections;
use crate::{named_error, to_u32_index};

const MAX_PARAMS: usize = 64;
const MAX_RESULTS: usize = 16;
const MAX_PAGES: u64 = 65536;

#[derive(Clone)]
struct FuncHandle(Func);

pub(crate) struct ModuleData {
    pub module: Module,
    pub exports: Vec<String>,
}

struct InstanceData {
    realm: Rc<Realm>,
    realm_obj: Value,
    module: Value,
    id: u32,
}

struct MemoryData {
    realm: Rc<Realm>,
    view: usize,
}

struct TableData {
    realm: Rc<Realm>,
    realm_obj: Value,
    table: Table,
}

enum GlobalData {
    Wasm {
        realm: Rc<Realm>,
        realm_obj: Value,
        global: Global,
    },
    Js {
        value: RefCell<Value>,
        mutable: bool,
    },
}

impl Trace for InstanceData {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.realm_obj);
        visit(&self.module);
    }
}

impl Drop for InstanceData {
    fn drop(&mut self) {
        let removed = self.realm.imports.borrow_mut().remove(&self.id);
        drop(removed);
    }
}

impl Trace for TableData {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.realm_obj);
    }
}

impl Trace for GlobalData {
    fn trace(&self, visit: &mut dyn FnMut(&Value)) {
        match self {
            GlobalData::Wasm { realm_obj, .. } => visit(realm_obj),
            GlobalData::Js { value, .. } => visit(&value.borrow()),
        }
    }
}

pub(crate) fn realm_of(value: &Value) -> Option<Rc<Realm>> {
    value.with_host::<RealmHandle, _>(|handle| handle.realm.clone())
}

fn proto_of(value: &Value, pick: fn(&RealmHandle) -> &Value) -> Option<Value> {
    value.with_host::<RealmHandle, _>(|handle| pick(handle).clone())
}

fn realm_or_error(scope: &mut Scope<'_>, value: &Value) -> Result<Rc<Realm>, Value> {
    realm_of(value).ok_or_else(|| scope.type_error("wasm realm is gone"))
}

fn arg(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

fn require_new(scope: &mut Scope<'_>, this: &Value, name: &str) -> Result<(), Value> {
    if scope.is_function(this) {
        Ok(())
    } else {
        Err(scope.type_error(&format!("WebAssembly.{name} must be called with 'new'")))
    }
}

pub(crate) fn make_function(
    scope: &mut Scope<'_>,
    realm_obj: &Value,
    func: Func,
    instance: Option<&Value>,
    name: &str,
) -> Value {
    let Some(realm) = realm_of(realm_obj) else {
        return Value::null();
    };
    let arity = realm.with_store(|ctx| func.ty(&ctx).params().len() as u32);
    let handle = scope.new_host_object(None, FuncHandle(func));
    let mut data = vec![realm_obj.clone(), handle];
    if let Some(instance) = instance {
        data.push(instance.clone());
    }
    scope.bound_function(name, arity, call_function, &data)
}

fn call_function(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let realm_obj = arg(data, 0);
    let realm = realm_or_error(scope, &realm_obj)?;
    let Some(func) = arg(data, 1).with_host::<FuncHandle, _>(|handle| handle.0) else {
        return Err(scope.type_error("not a wasm function"));
    };
    let ty = realm.with_store(|ctx| func.ty(&ctx));
    if ty.params().len() > MAX_PARAMS || ty.results().len() > MAX_RESULTS {
        return Err(scope.type_error("wasm function arity too large"));
    }
    let mut raws = Vec::with_capacity(ty.params().len());
    for (index, &param) in ty.params().iter().enumerate() {
        raws.push(convert::to_raw(scope, &realm, param, &arg(args, index))?);
    }
    let mut outputs: Vec<Val> = ty
        .results()
        .iter()
        .map(|&t| Val::default_for_ty(t))
        .collect();
    let stale = realm.pending.borrow_mut().take();
    drop(stale);
    let outcome = ffi::entered(scope, &realm.entry, || {
        realm.with_store(|mut ctx| {
            let inputs: Vec<Val> = raws.into_iter().map(|raw| raw.into_val(&mut ctx)).collect();
            func.call(&mut ctx, &inputs, &mut outputs)
        })
    });
    realm.sync_views(scope);
    if let Err(error) = outcome {
        let pending = realm.pending.borrow_mut().take();
        return Err(match pending {
            Some(exception) => exception,
            None => named_error(scope, "RuntimeError", &error.to_string()),
        });
    }
    let results: Vec<Raw> = realm.with_store(|mut ctx| {
        outputs
            .into_iter()
            .map(|val| Raw::from_val(&mut ctx, val))
            .collect()
    });
    let mut values: Vec<Value> = results
        .into_iter()
        .map(|raw| convert::to_js(scope, &realm, Some(&realm_obj), raw))
        .collect();
    match values.len() {
        0 => Ok(Value::undefined()),
        1 => Ok(values.remove(0)),
        _ => {
            let array = scope.new_array();
            for (index, value) in values.into_iter().enumerate() {
                scope.set_index(&array, index as u32, value)?;
            }
            Ok(array)
        }
    }
}

fn host_function(
    ctx: &mut StoreContextMut<'_, HostState>,
    ty: FuncType,
    instance: u32,
    index: usize,
) -> Func {
    let result_ty = ty.results().first().copied();
    Func::new(
        ctx,
        ty,
        move |mut caller: Caller<'_, HostState>, params: &[Val], results: &mut [Val]| {
            let Some(realm) = caller.data().realm.upgrade() else {
                return Err(wasmi::Error::new("wasm realm is gone"));
            };
            let Some(function) = realm.import(instance, index) else {
                return Err(wasmi::Error::new("wasm import binding is gone"));
            };
            let raws: Vec<Raw> = params
                .iter()
                .map(|val| Raw::from_val(&mut caller, val.clone()))
                .collect();
            let outcome = ffi::in_import(&realm.entry, &mut caller, |scope| {
                realm.sync_views(scope);
                let args: Vec<Value> = raws
                    .into_iter()
                    .map(|raw| convert::to_js(scope, &realm, None, raw))
                    .collect();
                let returned = scope.call(&function, &Value::undefined(), &args)?;
                match result_ty {
                    Some(ty) => convert::to_raw(scope, &realm, ty, &returned).map(Some),
                    None => Ok(None),
                }
            });
            match outcome {
                None => Err(wasmi::Error::new("wasm import called outside JavaScript")),
                Some(Err(exception)) => {
                    let previous = realm.pending.borrow_mut().replace(exception);
                    drop(previous);
                    Err(wasmi::Error::new("uncaught JavaScript exception"))
                }
                Some(Ok(raw)) => {
                    if let (Some(raw), Some(slot)) = (raw, results.first_mut()) {
                        *slot = raw.into_val(&mut caller);
                    }
                    Ok(())
                }
            }
        },
    )
}

pub(crate) fn compile_module(
    scope: &mut Scope<'_>,
    realm_obj: &Value,
    bytes: &[u8],
) -> Result<Value, Value> {
    let module = Module::new(engine(), bytes)
        .map_err(|error| named_error(scope, "CompileError", &error.to_string()))?;
    let proto = proto_of(realm_obj, |handle| &handle.protos.module);
    let exports = sections::export_names(bytes);
    Ok(scope.new_host_object(proto.as_ref(), ModuleData { module, exports }))
}

enum Plan {
    Func(FuncType, usize),
    Extern(Extern),
    Global(Val, Mutability),
}

fn link_error(scope: &mut Scope<'_>, message: &str) -> Value {
    named_error(scope, "LinkError", message)
}

fn import_global_value(
    scope: &mut Scope<'_>,
    ty: ValType,
    value: &Value,
) -> Result<Option<Val>, Value> {
    if ty == ValType::I64 && !value.is_number() {
        return Ok(scope.to_bigint64(value).ok().map(Val::I64));
    }
    if !value.is_number() {
        return Ok(None);
    }
    let number = scope.to_number(value)?;
    Ok(match ty {
        ValType::I32 => Some(Val::I32(number as i64 as i32)),
        ValType::I64 => Some(Val::I64(number as i64)),
        ValType::F32 => Some(Val::F32(wasmi::F32::from_float(number as f32))),
        ValType::F64 => Some(Val::F64(wasmi::F64::from_float(number))),
        _ => None,
    })
}

pub(crate) fn instantiate(
    scope: &mut Scope<'_>,
    realm_obj: &Value,
    module_value: &Value,
    import_object: &Value,
) -> Result<Value, Value> {
    let realm = realm_or_error(scope, realm_obj)?;
    let Some((module, names)) =
        module_value.with_host::<ModuleData, _>(|data| (data.module.clone(), data.exports.clone()))
    else {
        return Err(scope.type_error("WebAssembly.Instance requires a Module"));
    };
    let wanted: Vec<(String, String, ExternType)> = module
        .imports()
        .map(|import| {
            (
                import.module().to_owned(),
                import.name().to_owned(),
                import.ty().clone(),
            )
        })
        .collect();
    let mut plans = Vec::with_capacity(wanted.len());
    let mut functions = Vec::new();
    let mut imported_memory: Option<Value> = None;
    for (module_name, name, ty) in &wanted {
        if !import_object.is_object() {
            return Err(link_error(scope, "import object required"));
        }
        let namespace = scope.get(import_object, module_name)?;
        let value = if namespace.is_object() {
            scope.get(&namespace, name)?
        } else {
            Value::undefined()
        };
        match ty {
            ExternType::Func(func_type) => {
                if !scope.is_function(&value) {
                    let message = format!("import {module_name}.{name} is not a function");
                    return Err(link_error(scope, &message));
                }
                if func_type.params().len() > MAX_PARAMS || func_type.results().len() > 1 {
                    return Err(link_error(scope, "unsupported wasm import function arity"));
                }
                if !func_type
                    .params()
                    .iter()
                    .all(|&t| convert::is_host_compatible(t))
                {
                    return Err(link_error(scope, "unsupported wasm import parameter type"));
                }
                if !func_type
                    .results()
                    .iter()
                    .all(|&t| convert::is_host_compatible(t))
                {
                    return Err(link_error(scope, "unsupported wasm import result type"));
                }
                plans.push(Plan::Func(func_type.clone(), functions.len()));
                functions.push(value);
            }
            ExternType::Memory(_) => {
                let memory = value
                    .with_host::<MemoryData, _>(|data| {
                        Rc::ptr_eq(&data.realm, &realm).then_some(data.view)
                    })
                    .flatten()
                    .and_then(|view| realm.view_memory(view));
                let Some(memory) = memory else {
                    let message =
                        format!("import {module_name}.{name} is not a WebAssembly.Memory");
                    return Err(link_error(scope, &message));
                };
                plans.push(Plan::Extern(Extern::Memory(memory)));
                if imported_memory.is_none() {
                    imported_memory = Some(value.clone());
                }
            }
            ExternType::Table(_) => {
                let table = value
                    .with_host::<TableData, _>(|data| {
                        Rc::ptr_eq(&data.realm, &realm).then_some(data.table)
                    })
                    .flatten();
                let Some(table) = table else {
                    let message = format!("import {module_name}.{name} is not a WebAssembly.Table");
                    return Err(link_error(scope, &message));
                };
                plans.push(Plan::Extern(Extern::Table(table)));
            }
            ExternType::Global(global_type) => {
                let global = value
                    .with_host::<GlobalData, _>(|data| match data {
                        GlobalData::Wasm {
                            realm: owner,
                            global,
                            ..
                        } if Rc::ptr_eq(owner, &realm) => Some(*global),
                        _ => None,
                    })
                    .flatten();
                if let Some(global) = global {
                    plans.push(Plan::Extern(Extern::Global(global)));
                    continue;
                }
                match import_global_value(scope, global_type.content(), &value)? {
                    Some(val) => plans.push(Plan::Global(val, Mutability::Const)),
                    None => {
                        let message = format!(
                            "import {module_name}.{name} is not a WebAssembly.Global or a number"
                        );
                        return Err(link_error(scope, &message));
                    }
                }
            }
        }
    }
    let id = realm.next_instance_id();
    let replaced = realm.imports.borrow_mut().insert(id, functions);
    drop(replaced);
    let externs: Vec<Extern> = realm.with_store(|mut ctx| {
        plans
            .into_iter()
            .map(|plan| match plan {
                Plan::Func(ty, index) => Extern::Func(host_function(&mut ctx, ty, id, index)),
                Plan::Extern(external) => external,
                Plan::Global(val, mutability) => {
                    Extern::Global(Global::new(&mut ctx, val, mutability))
                }
            })
            .collect()
    });
    let stale = realm.pending.borrow_mut().take();
    drop(stale);
    let outcome = ffi::entered(scope, &realm.entry, || {
        realm.with_store(|mut ctx| Instance::new(&mut ctx, &module, &externs))
    });
    realm.sync_views(scope);
    let instance = match outcome {
        Ok(instance) => instance,
        Err(error) => {
            let removed = realm.imports.borrow_mut().remove(&id);
            drop(removed);
            let pending = realm.pending.borrow_mut().take();
            if let Some(exception) = pending {
                return Err(exception);
            }
            let kind = if error.as_trap_code().is_some() {
                "RuntimeError"
            } else {
                "LinkError"
            };
            return Err(named_error(scope, kind, &error.to_string()));
        }
    };
    let instance_proto = proto_of(realm_obj, |handle| &handle.protos.instance);
    let instance_obj = scope.new_traced_host_object(
        instance_proto.as_ref(),
        InstanceData {
            realm: realm.clone(),
            realm_obj: realm_obj.clone(),
            module: module_value.clone(),
            id,
        },
    );
    let exports = scope.new_object();
    let mut own_memory: Option<Value> = None;
    for name in names {
        let external = realm.with_store(|ctx| instance.get_export(&ctx, &name));
        let value = match external {
            Some(Extern::Func(func)) => {
                make_function(scope, realm_obj, func, Some(&instance_obj), &name)
            }
            Some(Extern::Memory(memory)) => match (&imported_memory, &own_memory) {
                (Some(object), _) | (None, Some(object)) => object.clone(),
                (None, None) => {
                    let object = new_memory_object(scope, realm_obj, &realm, memory);
                    own_memory = Some(object.clone());
                    object
                }
            },
            Some(Extern::Table(table)) => new_table_object(scope, realm_obj, &realm, table),
            Some(Extern::Global(global)) => new_global_object(scope, realm_obj, &realm, global),
            None => continue,
        };
        scope.set(&exports, &name, value)?;
    }
    scope.define(&instance_obj, "exports", exports, Attributes::ENUMERABLE)?;
    Ok(instance_obj)
}

fn new_memory_object(
    scope: &mut Scope<'_>,
    realm_obj: &Value,
    realm: &Rc<Realm>,
    memory: Memory,
) -> Value {
    let proto = proto_of(realm_obj, |handle| &handle.protos.memory);
    let view = realm.add_view(memory);
    scope.new_host_object(
        proto.as_ref(),
        MemoryData {
            realm: realm.clone(),
            view,
        },
    )
}

fn new_table_object(
    scope: &mut Scope<'_>,
    realm_obj: &Value,
    realm: &Rc<Realm>,
    table: Table,
) -> Value {
    let proto = proto_of(realm_obj, |handle| &handle.protos.table);
    scope.new_traced_host_object(
        proto.as_ref(),
        TableData {
            realm: realm.clone(),
            realm_obj: realm_obj.clone(),
            table,
        },
    )
}

fn new_global_object(
    scope: &mut Scope<'_>,
    realm_obj: &Value,
    realm: &Rc<Realm>,
    global: Global,
) -> Value {
    let proto = proto_of(realm_obj, |handle| &handle.protos.global);
    scope.new_traced_host_object(
        proto.as_ref(),
        GlobalData::Wasm {
            realm: realm.clone(),
            realm_obj: realm_obj.clone(),
            global,
        },
    )
}

pub(crate) fn module_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    require_new(scope, this, "Module")?;
    let Some(bytes) = args
        .first()
        .and_then(|source| scope.buffer_source_bytes(source))
    else {
        return Err(scope.type_error("WebAssembly.Module requires a BufferSource"));
    };
    compile_module(scope, &arg(data, 0), &bytes)
}

pub(crate) fn instance_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    require_new(scope, this, "Instance")?;
    if args.is_empty() {
        return Err(scope.type_error("WebAssembly.Instance requires a Module"));
    }
    instantiate(scope, &arg(data, 0), &args[0], &arg(args, 1))
}

fn kind_name(ty: &ExternType) -> &'static str {
    match ty {
        ExternType::Func(_) => "function",
        ExternType::Table(_) => "table",
        ExternType::Memory(_) => "memory",
        ExternType::Global(_) => "global",
    }
}

fn module_arg(
    scope: &mut Scope<'_>,
    args: &[Value],
    what: &str,
) -> Result<(Module, Vec<String>), Value> {
    args.first()
        .and_then(|value| {
            value.with_host::<ModuleData, _>(|data| (data.module.clone(), data.exports.clone()))
        })
        .ok_or_else(|| scope.type_error(&format!("WebAssembly.Module.{what} requires a Module")))
}

pub(crate) fn module_exports(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let (module, names) = module_arg(scope, args, "exports")?;
    let array = scope.new_array();
    for (index, export) in names.iter().enumerate() {
        let Some(ty) = module.get_export(export) else {
            continue;
        };
        let entry = scope.new_object();
        let name = scope.string(export);
        scope.set(&entry, "name", name)?;
        let kind = scope.string(kind_name(&ty));
        scope.set(&entry, "kind", kind)?;
        scope.set_index(&array, index as u32, entry)?;
    }
    Ok(array)
}

pub(crate) fn module_imports(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let (module, _) = module_arg(scope, args, "imports")?;
    let array = scope.new_array();
    for (index, import) in module.imports().enumerate() {
        let entry = scope.new_object();
        let module_name = scope.string(import.module());
        scope.set(&entry, "module", module_name)?;
        let name = scope.string(import.name());
        scope.set(&entry, "name", name)?;
        let kind = scope.string(kind_name(import.ty()));
        scope.set(&entry, "kind", kind)?;
        scope.set_index(&array, index as u32, entry)?;
    }
    Ok(array)
}

fn memory_this(scope: &mut Scope<'_>, this: &Value) -> Result<(Rc<Realm>, usize), Value> {
    this.with_host::<MemoryData, _>(|data| (data.realm.clone(), data.view))
        .ok_or_else(|| scope.type_error("not a WebAssembly.Memory"))
}

pub(crate) fn memory_buffer(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    let (realm, view) = memory_this(scope, this)?;
    realm.view_buffer(scope, view)
}

pub(crate) fn memory_grow(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let (realm, view) = memory_this(scope, this)?;
    let delta = to_u32_index(scope, &arg(args, 0), "wasm memory.grow delta")?;
    let Some(memory) = realm.view_memory(view) else {
        return Err(scope.type_error("wasm memory is gone"));
    };
    let old_pages = realm.with_store(|ctx| memory.size(&ctx));
    if delta > 0 {
        if (old_pages + u64::from(delta)) * PAGE_SIZE > MEMORY_MAX_BYTES {
            return Err(scope.range_error("wasm memory.grow failed"));
        }
        let grown = realm.with_store(|mut ctx| memory.grow(&mut ctx, u64::from(delta)));
        if grown.is_err() {
            return Err(scope.range_error("wasm memory.grow failed"));
        }
        realm.detach_view(scope, view);
    }
    Ok(Value::number(old_pages as f64))
}

fn descriptor_index(
    scope: &mut Scope<'_>,
    descriptor: &Value,
    keys: &[&str],
    what: &str,
) -> Result<Option<u32>, Value> {
    for key in keys {
        let value = scope.get(descriptor, key)?;
        if !value.is_undefined() {
            return to_u32_index(scope, &value, what).map(Some);
        }
    }
    Ok(None)
}

pub(crate) fn memory_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    require_new(scope, this, "Memory")?;
    let realm_obj = arg(data, 0);
    let realm = realm_or_error(scope, &realm_obj)?;
    let descriptor = arg(args, 0);
    if !descriptor.is_object() {
        return Err(scope.type_error("wasm memory descriptor required"));
    }
    let initial = descriptor_index(
        scope,
        &descriptor,
        &["initial", "minimum"],
        "wasm memory initial",
    )?
    .unwrap_or(0);
    let maximum = descriptor_index(scope, &descriptor, &["maximum"], "wasm memory maximum")?;
    if maximum.is_some_and(|maximum| maximum < initial) {
        return Err(scope.range_error("wasm memory maximum is less than initial"));
    }
    let shared = scope.get(&descriptor, "shared")?;
    if scope.to_bool(&shared) {
        return Err(scope.type_error("shared wasm memory is not supported"));
    }
    if u64::from(initial) * PAGE_SIZE > MEMORY_MAX_BYTES {
        return Err(scope.range_error("wasm memory initial is too large"));
    }
    if maximum.is_some_and(|maximum| u64::from(maximum) > MAX_PAGES) {
        return Err(scope.range_error("wasm memory maximum is too large"));
    }
    let created = realm.with_store(|mut ctx| {
        Memory::new(&mut ctx, MemoryType::new(initial, maximum)).map_err(|e| e.to_string())
    });
    match created {
        Ok(memory) => Ok(new_memory_object(scope, &realm_obj, &realm, memory)),
        Err(message) => Err(scope.range_error(&message)),
    }
}

fn table_this(scope: &mut Scope<'_>, this: &Value) -> Result<(Rc<Realm>, Value, Table), Value> {
    this.with_host::<TableData, _>(|data| (data.realm.clone(), data.realm_obj.clone(), data.table))
        .ok_or_else(|| scope.type_error("not a WebAssembly.Table"))
}

pub(crate) fn table_length(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    let (realm, _, table) = table_this(scope, this)?;
    let size = realm.with_store(|ctx| table.size(&ctx));
    Ok(Value::number(size as f64))
}

pub(crate) fn table_get(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let (realm, realm_obj, table) = table_this(scope, this)?;
    let index = to_u32_index(scope, &arg(args, 0), "wasm table index")?;
    let raw = realm.with_store(|mut ctx| {
        table
            .get(&ctx, u64::from(index))
            .map(|reference| Raw::from_ref(&mut ctx, reference))
    });
    match raw {
        Some(raw) => Ok(convert::to_js(scope, &realm, Some(&realm_obj), raw)),
        None => Err(scope.range_error("table index out of bounds")),
    }
}

fn table_element(
    scope: &mut Scope<'_>,
    realm: &Realm,
    element: RefType,
    value: &Value,
    action: &str,
) -> Result<Raw, Value> {
    match element {
        RefType::Func if value.is_null() => Ok(Raw::Func(None)),
        RefType::Func => Err(scope.type_error(&format!(
            "{action} funcref table entries from JS is not supported"
        ))),
        RefType::Extern if value.is_null() => Ok(Raw::Extern(None)),
        RefType::Extern => Ok(Raw::Extern(Some(realm.add_extern(value.clone())))),
    }
}

pub(crate) fn table_set(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let (realm, _, table) = table_this(scope, this)?;
    let index = to_u32_index(scope, &arg(args, 0), "wasm table index")?;
    let (element, size) = realm.with_store(|ctx| (table.ty(&ctx).element(), table.size(&ctx)));
    let raw = table_element(scope, &realm, element, &arg(args, 1), "setting")?;
    if u64::from(index) >= size {
        return Err(scope.range_error("table index out of bounds"));
    }
    let stored = realm.with_store(|mut ctx| {
        let reference = raw.into_ref(&mut ctx).unwrap_or(Ref::null(element));
        table.set(&mut ctx, u64::from(index), reference)
    });
    match stored {
        Ok(()) => Ok(Value::undefined()),
        Err(_) => Err(scope.range_error("table index out of bounds")),
    }
}

pub(crate) fn table_grow(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let (realm, _, table) = table_this(scope, this)?;
    let delta = to_u32_index(scope, &arg(args, 0), "wasm table.grow delta")?;
    let element = realm.with_store(|ctx| table.ty(&ctx).element());
    let raw = if args.len() > 1 {
        table_element(scope, &realm, element, &args[1], "growing")?
    } else {
        match element {
            RefType::Func => Raw::Func(None),
            RefType::Extern => Raw::Extern(None),
        }
    };
    let grown = realm.with_store(|mut ctx| {
        let reference = raw.into_ref(&mut ctx).unwrap_or(Ref::null(element));
        table.grow(&mut ctx, u64::from(delta), reference)
    });
    match grown {
        Ok(old) => Ok(Value::number(old as f64)),
        Err(_) => Err(scope.range_error("wasm table.grow failed")),
    }
}

pub(crate) fn table_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    require_new(scope, this, "Table")?;
    let realm_obj = arg(data, 0);
    let realm = realm_or_error(scope, &realm_obj)?;
    let descriptor = arg(args, 0);
    if !descriptor.is_object() {
        return Err(scope.type_error("wasm table descriptor required"));
    }
    let element_value = scope.get(&descriptor, "element")?;
    let element = match scope.to_string(&element_value)?.as_str() {
        "externref" => RefType::Extern,
        _ => RefType::Func,
    };
    let initial = descriptor_index(
        scope,
        &descriptor,
        &["initial", "minimum"],
        "wasm table initial",
    )?
    .unwrap_or(0);
    let maximum = descriptor_index(scope, &descriptor, &["maximum"], "wasm table maximum")?;
    if maximum.is_some_and(|maximum| maximum < initial) {
        return Err(scope.range_error("wasm table maximum is less than initial"));
    }
    let created = realm.with_store(|mut ctx| {
        Table::new(
            &mut ctx,
            TableType::new(element, initial, maximum),
            Ref::null(element),
        )
        .map_err(|e| e.to_string())
    });
    match created {
        Ok(table) => Ok(new_table_object(scope, &realm_obj, &realm, table)),
        Err(message) => Err(scope.range_error(&message)),
    }
}

fn global_get(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
    let target = this.with_host::<GlobalData, _>(|data| match data {
        GlobalData::Wasm {
            realm,
            realm_obj,
            global,
        } => Ok((realm.clone(), realm_obj.clone(), *global)),
        GlobalData::Js { value, .. } => Err(value.borrow().clone()),
    });
    match target {
        None => Err(scope.type_error("not a WebAssembly.Global")),
        Some(Err(value)) => Ok(value),
        Some(Ok((realm, realm_obj, global))) => {
            let raw = realm.with_store(|mut ctx| {
                let val = global.get(&ctx);
                Raw::from_val(&mut ctx, val)
            });
            Ok(convert::to_js(scope, &realm, Some(&realm_obj), raw))
        }
    }
}

pub(crate) fn global_value(
    scope: &mut Scope<'_>,
    this: &Value,
    _args: &[Value],
) -> Result<Value, Value> {
    global_get(scope, this)
}

pub(crate) fn global_set_value(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    let value = arg(args, 0);
    let target = this.with_host::<GlobalData, _>(|data| match data {
        GlobalData::Wasm { realm, global, .. } => Some((realm.clone(), *global)),
        GlobalData::Js { .. } => None,
    });
    match target {
        None => Err(scope.type_error("not a WebAssembly.Global")),
        Some(None) => {
            let replaced = this.with_host::<GlobalData, _>(|data| match data {
                GlobalData::Js {
                    value: slot,
                    mutable: true,
                } => Some(slot.replace(value.clone())),
                _ => None,
            });
            match replaced.flatten() {
                Some(old) => {
                    drop(old);
                    Ok(Value::undefined())
                }
                None => Err(scope.type_error("WebAssembly.Global is immutable")),
            }
        }
        Some(Some((realm, global))) => {
            let ty = realm.with_store(|ctx| global.ty(&ctx));
            if ty.mutability().is_const() {
                return Err(scope.type_error("WebAssembly.Global is immutable"));
            }
            let raw = convert::to_raw(scope, &realm, ty.content(), &value)?;
            let stored = realm.with_store(|mut ctx| {
                let val = raw.into_val(&mut ctx);
                global.set(&mut ctx, val).map_err(|e| e.to_string())
            });
            match stored {
                Ok(()) => Ok(Value::undefined()),
                Err(message) => Err(scope.type_error(&message)),
            }
        }
    }
}

pub(crate) fn global_ctor(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    require_new(scope, this, "Global")?;
    let realm_obj = arg(data, 0);
    let realm = realm_or_error(scope, &realm_obj)?;
    let descriptor = arg(args, 0);
    if !descriptor.is_object() {
        return Err(scope.type_error("wasm global descriptor required"));
    }
    let type_value = scope.get(&descriptor, "value")?;
    let type_name = scope.to_string(&type_value)?;
    let mutable_value = scope.get(&descriptor, "mutable")?;
    let mutable = scope.to_bool(&mutable_value);
    let init = arg(args, 1);
    let ty = match type_name.as_str() {
        "i32" => ValType::I32,
        "i64" => ValType::I64,
        "f32" => ValType::F32,
        "f64" => ValType::F64,
        "externref" | "anyfunc" | "funcref" => {
            let is_func = type_name != "externref";
            if is_func && !init.is_undefined() && !init.is_null() && !scope.is_function(&init) {
                return Err(
                    scope.type_error("funcref global requires an exported wasm function or null")
                );
            }
            let value = if is_func && init.is_undefined() {
                Value::null()
            } else {
                init
            };
            let proto = proto_of(&realm_obj, |handle| &handle.protos.global);
            return Ok(scope.new_traced_host_object(
                proto.as_ref(),
                GlobalData::Js {
                    value: RefCell::new(value),
                    mutable,
                },
            ));
        }
        _ => return Err(scope.type_error("unsupported wasm global type")),
    };
    let raw = if init.is_undefined() {
        convert::default_raw(ty)
    } else {
        convert::to_raw(scope, &realm, ty, &init)?
    };
    let mutability = if mutable {
        Mutability::Var
    } else {
        Mutability::Const
    };
    let global = realm.with_store(|mut ctx| {
        let val = raw.into_val(&mut ctx);
        Global::new(&mut ctx, val, mutability)
    });
    Ok(new_global_object(scope, &realm_obj, &realm, global))
}
