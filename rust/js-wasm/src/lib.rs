//! Southstar — the WebAssembly JavaScript API over the wasmi interpreter, installed into each page and worker realm.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod convert;
mod ffi;
mod objects;
mod realm;
mod sections;

use southstar_js_engine::{Attributes, BoundFn, NativeFn, Scope, Value};

use crate::realm::{Protos, Realm, RealmHandle, engine};

const BOOTSTRAP: &str = "(() => {\
  const W = WebAssembly, dp = Object.defineProperty;\
  const error = (name) => {\
    const C = ({ [name]: class extends Error {\
      constructor(message) { super(message, arguments[1]); } } })[name];\
    dp(C.prototype, 'name', { value: name, writable: true, configurable: true });\
    dp(C.prototype, 'message', { value: '', writable: true, configurable: true });\
    dp(W, name, { value: C, writable: true, configurable: true });\
  };\
  error('CompileError'); error('LinkError'); error('RuntimeError');\
  const op = (f) => dp(W, f.name, { value: f, writable: true, enumerable: true, configurable: true });\
  op(async function compileStreaming(source) {\
    return W.compile(await (await source).arrayBuffer()); });\
  op(async function instantiateStreaming(source) {\
    const importObject = arguments[1];\
    return W.instantiate(await (await source).arrayBuffer(), importObject); });\
})();";

const TRANSFER_METHODS: [&str; 3] = ["transfer", "transferToFixedLength", "transferToImmutable"];

pub(crate) fn named_error(scope: &mut Scope<'_>, name: &str, message: &str) -> Value {
    let global = scope.global();
    let constructor = scope
        .get(&global, "WebAssembly")
        .ok()
        .filter(Value::is_object)
        .and_then(|namespace| scope.get(&namespace, name).ok());
    if let Some(constructor) = constructor.filter(|c| scope.is_function(c)) {
        let text = scope.string(message);
        return match scope.construct(&constructor, &[text]) {
            Ok(error) => error,
            Err(error) => error,
        };
    }
    scope.type_error(&format!("{name}: {message}"))
}

pub(crate) fn to_u32_index(scope: &mut Scope<'_>, value: &Value, what: &str) -> Result<u32, Value> {
    let number = scope.to_number(value)?;
    let integer = if number.is_nan() { 0.0 } else { number.trunc() };
    if !(0.0..=9007199254740991.0).contains(&integer) {
        return Err(scope.range_error("invalid array index"));
    }
    if integer > f64::from(u32::MAX) {
        return Err(scope.range_error(&format!("{what} is too large")));
    }
    Ok(integer as u32)
}

fn settle(scope: &mut Scope<'_>, result: Result<Value, Value>) -> Result<Value, Value> {
    let (promise, resolve, reject) = scope.new_promise()?;
    match result {
        Ok(value) => scope.call(&resolve, &Value::undefined(), &[value])?,
        Err(reason) => scope.call(&reject, &Value::undefined(), &[reason])?,
    };
    Ok(promise)
}

fn compile(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let result = match args
        .first()
        .and_then(|source| scope.buffer_source_bytes(source))
    {
        Some(bytes) => objects::compile_module(scope, &data[0], &bytes),
        None => Err(scope.type_error("WebAssembly.compile requires a BufferSource")),
    };
    settle(scope, result)
}

fn instantiate_source(
    scope: &mut Scope<'_>,
    realm_obj: &Value,
    source: &Value,
    imports: &Value,
) -> Result<Value, Value> {
    if source.with_host::<objects::ModuleData, _>(|_| ()).is_some() {
        return objects::instantiate(scope, realm_obj, source, imports);
    }
    let Some(bytes) = scope.buffer_source_bytes(source) else {
        return Err(scope.type_error("WebAssembly.instantiate requires a BufferSource or Module"));
    };
    let module = objects::compile_module(scope, realm_obj, &bytes)?;
    let instance = objects::instantiate(scope, realm_obj, &module, imports)?;
    let pair = scope.new_object();
    scope.set(&pair, "module", module)?;
    scope.set(&pair, "instance", instance)?;
    Ok(pair)
}

fn instantiate(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    let source = args.first().cloned().unwrap_or_else(Value::undefined);
    let imports = args.get(1).cloned().unwrap_or_else(Value::undefined);
    let result = instantiate_source(scope, &data[0], &source, &imports);
    settle(scope, result)
}

fn validate(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(bytes) = args
        .first()
        .and_then(|source| scope.buffer_source_bytes(source))
    else {
        return Err(scope.type_error("WebAssembly.validate requires a BufferSource"));
    };
    Ok(Value::boolean(
        wasmi::Module::validate(engine(), &bytes).is_ok(),
    ))
}

fn transfer_guard(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
    data: &[Value],
) -> Result<Value, Value> {
    if objects::realm_of(&data[1]).is_some_and(|realm| realm.is_memory_buffer(this)) {
        return Err(scope.type_error("cannot transfer the buffer of a WebAssembly.Memory"));
    }
    scope.call(&data[0], this, args)
}

fn method(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    arity: u32,
    f: NativeFn,
) -> Result<(), Value> {
    let function = scope.function(name, arity, f);
    scope.define(object, name, function, Attributes::METHOD)
}

fn getter(
    scope: &mut Scope<'_>,
    object: &Value,
    name: &str,
    get: NativeFn,
    set: Option<NativeFn>,
) -> Result<(), Value> {
    let get = scope.function(&format!("get {name}"), 0, get);
    let set = set.map(|set| scope.function(&format!("set {name}"), 1, set));
    scope.define_accessor(
        object,
        name,
        Some(&get),
        set.as_ref(),
        Attributes::CONFIGURABLE,
    )
}

fn interface(
    scope: &mut Scope<'_>,
    namespace: &Value,
    name: &str,
    f: BoundFn,
    prototype: &Value,
    realm_obj: &Value,
) -> Result<Value, Value> {
    let constructor = scope.bound_constructor(name, 1, f, core::slice::from_ref(realm_obj));
    scope.set_constructor(&constructor, prototype)?;
    scope.define(namespace, name, constructor.clone(), Attributes::METHOD)?;
    Ok(constructor)
}

fn guard_buffer_transfer(
    scope: &mut Scope<'_>,
    global: &Value,
    realm_obj: &Value,
) -> Result<(), Value> {
    let constructor = scope.get(global, "ArrayBuffer")?;
    if !constructor.is_object() {
        return Ok(());
    }
    let prototype = scope.get(&constructor, "prototype")?;
    if !prototype.is_object() {
        return Ok(());
    }
    for name in TRANSFER_METHODS {
        let original = scope.get(&prototype, name)?;
        if scope.is_function(&original) {
            let guard =
                scope.bound_function(name, 0, transfer_guard, &[original, realm_obj.clone()]);
            scope.define(&prototype, name, guard, Attributes::METHOD)?;
        }
    }
    Ok(())
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) -> Result<(), Value> {
    let protos = Protos {
        module: scope.new_object(),
        instance: scope.new_object(),
        memory: scope.new_object(),
        table: scope.new_object(),
        global: scope.new_object(),
    };
    let realm_obj = scope.new_traced_host_object(
        None,
        RealmHandle {
            realm: Realm::new(),
            protos: Protos {
                module: protos.module.clone(),
                instance: protos.instance.clone(),
                memory: protos.memory.clone(),
                table: protos.table.clone(),
                global: protos.global.clone(),
            },
        },
    );

    getter(
        scope,
        &protos.memory,
        "buffer",
        objects::memory_buffer,
        None,
    )?;
    method(scope, &protos.memory, "grow", 1, objects::memory_grow)?;
    scope.define_to_string_tag(&protos.memory, "WebAssembly.Memory")?;

    getter(scope, &protos.table, "length", objects::table_length, None)?;
    method(scope, &protos.table, "get", 1, objects::table_get)?;
    method(scope, &protos.table, "set", 2, objects::table_set)?;
    method(scope, &protos.table, "grow", 1, objects::table_grow)?;
    scope.define_to_string_tag(&protos.table, "WebAssembly.Table")?;

    getter(
        scope,
        &protos.global,
        "value",
        objects::global_value,
        Some(objects::global_set_value),
    )?;
    method(scope, &protos.global, "valueOf", 0, objects::global_value)?;
    scope.define_to_string_tag(&protos.global, "WebAssembly.Global")?;

    let namespace = scope.new_object();
    let module = interface(
        scope,
        &namespace,
        "Module",
        objects::module_ctor,
        &protos.module,
        &realm_obj,
    )?;
    let exports = scope.function("exports", 1, objects::module_exports);
    scope.set(&module, "exports", exports)?;
    let imports = scope.function("imports", 1, objects::module_imports);
    scope.set(&module, "imports", imports)?;
    interface(
        scope,
        &namespace,
        "Instance",
        objects::instance_ctor,
        &protos.instance,
        &realm_obj,
    )?;
    interface(
        scope,
        &namespace,
        "Memory",
        objects::memory_ctor,
        &protos.memory,
        &realm_obj,
    )?;
    interface(
        scope,
        &namespace,
        "Table",
        objects::table_ctor,
        &protos.table,
        &realm_obj,
    )?;
    interface(
        scope,
        &namespace,
        "Global",
        objects::global_ctor,
        &protos.global,
        &realm_obj,
    )?;

    let compile = scope.bound_function("compile", 1, compile, core::slice::from_ref(&realm_obj));
    scope.set(&namespace, "compile", compile)?;
    let validate = scope.function("validate", 1, validate);
    scope.set(&namespace, "validate", validate)?;
    let instantiate = scope.bound_function(
        "instantiate",
        1,
        instantiate,
        core::slice::from_ref(&realm_obj),
    );
    scope.set(&namespace, "instantiate", instantiate)?;
    scope.define_to_string_tag(&namespace, "WebAssembly")?;
    scope.define(global, "WebAssembly", namespace, Attributes::METHOD)?;
    guard_buffer_transfer(scope, global, &realm_obj)?;
    drop(scope.eval_native_script(BOOTSTRAP, "<wasm-bootstrap>"));
    Ok(())
}
