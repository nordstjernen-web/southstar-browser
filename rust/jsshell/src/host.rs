//! Southstar — the host objects southstar-jsshell installs in every realm: print and test262's $262.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;

use southstar_js_engine::{Scope, Value};

thread_local! {
    static PRINTED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static ECHO: RefCell<bool> = const { RefCell::new(true) };
}

pub fn set_echo(echo: bool) {
    ECHO.with(|e| *e.borrow_mut() = echo);
}

pub fn take_printed() -> Vec<String> {
    PRINTED.with(|printed| std::mem::take(&mut *printed.borrow_mut()))
}

fn argument(args: &[Value], index: usize) -> Value {
    args.get(index).cloned().unwrap_or_else(Value::undefined)
}

fn print(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let mut line = Vec::new();
    for arg in args {
        line.push(scope.to_string(arg)?);
    }
    let line = line.join(" ");
    if ECHO.with(|e| *e.borrow()) {
        println!("{line}");
    }
    PRINTED.with(|printed| printed.borrow_mut().push(line));
    Ok(Value::undefined())
}

fn read(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let path = scope.to_string(&argument(args, 0))?;
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok(scope.string(&text)),
        Err(e) => Err(scope.type_error(&format!("cannot read {path}: {e}"))),
    }
}

fn eval_script(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> Result<Value, Value> {
    let source = scope.to_string(&argument(args, 0))?;
    scope.eval_script(&source, "evalScript")
}

fn detach_array_buffer(
    scope: &mut Scope<'_>,
    _this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    scope.detach_array_buffer(&argument(args, 0))?;
    Ok(Value::undefined())
}

fn gc(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    scope.gc();
    Ok(Value::undefined())
}

fn create_realm(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> Result<Value, Value> {
    let global = scope.new_realm(install)?;
    scope.get(&global, "$262")
}

pub fn install(scope: &mut Scope<'_>) -> Result<(), Value> {
    let global = scope.global();
    let print = scope.function("print", 1, print);
    scope.set(&global, "print", print)?;
    let read = scope.function("read", 1, read);
    scope.set(&global, "read", read)?;
    let test262 = scope.new_object();
    scope.set(&test262, "global", global.clone())?;
    for (name, arity, f) in [
        (
            "evalScript",
            1,
            eval_script as southstar_js_engine::NativeFn,
        ),
        ("detachArrayBuffer", 1, detach_array_buffer),
        ("gc", 0, gc),
        ("createRealm", 0, create_realm),
    ] {
        let function = scope.function(name, arity, f);
        scope.set(&test262, name, function)?;
    }
    scope.set(&global, "$262", test262)
}

pub fn describe(scope: &mut Scope<'_>, error: &Value) -> (String, String) {
    let name = scope
        .get(error, "constructor")
        .and_then(|constructor| scope.get(&constructor, "name"))
        .and_then(|name| scope.to_string(&name))
        .unwrap_or_default();
    let message = scope
        .get(error, "message")
        .and_then(|message| {
            if message.is_undefined() {
                scope.to_string(error)
            } else {
                scope.to_string(&message)
            }
        })
        .or_else(|_| scope.to_string(error))
        .unwrap_or_else(|_| "<unprintable>".to_owned());
    (name, message)
}
