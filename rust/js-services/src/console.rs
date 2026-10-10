//! Southstar — the window console: formatting arguments into log lines, count, time and the namespace object.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::time::Instant;

use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{self, Js};
use crate::{navigator, page_or_new, with_page};

const LOG_METHODS: &[&str] = &[
    "log",
    "info",
    "debug",
    "trace",
    "table",
    "group",
    "groupCollapsed",
    "dir",
    "dirxml",
];

const NOOP_METHODS: &[&str] = &[
    "groupEnd",
    "profile",
    "profileEnd",
    "timeStamp",
    "context",
    "clear",
];

fn formatted(scope: &mut Scope<'_>, out: &mut String, args: &[Value]) -> usize {
    if args.len() < 2 || !args[0].is_string() {
        return 0;
    }
    let Ok(format) = scope.to_string(&args[0]) else {
        return 0;
    };
    if !format.contains('%') {
        return 0;
    }
    let mut next = 1;
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let Some(directive) = chars.next() else {
            out.push('%');
            break;
        };
        match directive {
            '%' => out.push('%'),
            'c' => {
                if next < args.len() {
                    next += 1;
                }
            }
            's' | 'd' | 'i' | 'f' | 'o' | 'O' | 'j' => {
                if next < args.len() {
                    if let Ok(text) = scope.to_string(&args[next]) {
                        out.push_str(&text);
                    }
                    next += 1;
                }
            }
            other => {
                out.push('%');
                out.push(other);
            }
        }
    }
    next
}

fn text_unless_undefined(scope: &mut Scope<'_>, value: &Value) -> Option<String> {
    if value.is_undefined() {
        None
    } else {
        scope.to_string(value).ok()
    }
}

fn text_if_string(scope: &mut Scope<'_>, value: &Value) -> Option<String> {
    if value.is_string() {
        scope.to_string(value).ok()
    } else {
        None
    }
}

fn error_fields(scope: &mut Scope<'_>, value: &Value) -> [Value; 3] {
    ["name", "message", "stack"]
        .map(|key| scope.get(value, key).unwrap_or_else(|_| Value::undefined()))
}

fn append_error(scope: &mut Scope<'_>, out: &mut String, value: &Value) {
    let [name, message, stack] = error_fields(scope, value);
    let name = text_unless_undefined(scope, &name).filter(|n| !n.is_empty());
    let message = text_unless_undefined(scope, &message).filter(|m| !m.is_empty());
    let stack = text_unless_undefined(scope, &stack).filter(|s| !s.is_empty());
    out.push_str(name.as_deref().unwrap_or("Error"));
    if let Some(message) = message {
        out.push_str(": ");
        out.push_str(&message);
    }
    if let Some(stack) = stack {
        out.push('\n');
        out.push_str(&stack);
    }
}

fn append_object(scope: &mut Scope<'_>, out: &mut String, value: &Value) {
    let [name, message, stack] = error_fields(scope, value);
    if stack.is_string() {
        let name = text_if_string(scope, &name).filter(|n| !n.is_empty());
        let message = text_if_string(scope, &message).filter(|m| !m.is_empty());
        let stack = scope.to_string(&stack).ok();
        if let Some(name) = &name {
            out.push_str(name);
        }
        if let Some(message) = &message {
            if name.is_some() {
                out.push_str(": ");
            }
            out.push_str(message);
        }
        if name.is_some() || message.is_some() {
            out.push('\n');
        }
        if let Some(stack) = stack {
            out.push_str(&stack);
        }
        return;
    }
    let json = scope
        .json_stringify(value)
        .ok()
        .filter(|json| !json.is_undefined());
    let text = match json {
        Some(json) => scope.to_string(&json),
        None => scope.to_string(value),
    };
    if let Ok(text) = text {
        out.push_str(&text);
    }
}

pub(crate) fn emit(scope: &mut Scope<'_>, js: Js, prefix: &str, args: &[Value]) {
    if !ffi::log_enabled(js) {
        return;
    }
    let mut out = String::from(prefix);
    let mut start = 0;
    if args.len() >= 2 && args[0].is_string() {
        let mut format_out = String::new();
        start = formatted(scope, &mut format_out, args);
        if start > 0 {
            if !prefix.is_empty() {
                out.push(' ');
            }
            out.push_str(&format_out);
        }
    }
    for (index, value) in args.iter().enumerate().skip(start) {
        if index > start || start > 0 || !prefix.is_empty() {
            out.push(' ');
        }
        if scope.is_error(value) {
            append_error(scope, &mut out, value);
        } else if value.is_object() && !scope.is_function(value) {
            append_object(scope, &mut out, value);
        } else if let Ok(text) = scope.to_string(value) {
            out.push_str(&text);
        }
    }
    ffi::log_line(js, &out);
}

fn log(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    emit(scope, js, "", args);
    Ok(Value::undefined())
}

fn warn(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    emit(scope, js, "[warn]", args);
    Ok(Value::undefined())
}

fn error(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    emit(scope, js, "[error]", args);
    Ok(Value::undefined())
}

fn assert(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let Some(condition) = args.first() else {
        return Ok(Value::undefined());
    };
    if scope.to_bool(condition) {
        return Ok(Value::undefined());
    }
    let js = ffi::js_of(scope);
    emit(scope, js, "[assert]", &args[1..]);
    Ok(Value::undefined())
}

pub(crate) fn alert(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let js = ffi::js_of(scope);
    emit(scope, js, "[alert]", args);
    Ok(Value::undefined())
}

fn label(scope: &mut Scope<'_>, args: &[Value]) -> Result<String, Value> {
    match args.first() {
        Some(value) if !value.is_undefined() => scope.to_string(value),
        _ => Ok("default".to_owned()),
    }
}

fn count(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let label = label(scope, args)?;
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let page = page_or_new(js);
    let n = {
        let mut counts = page.counts.borrow_mut();
        let n = counts.entry(label.clone()).or_insert(0);
        *n += 1;
        *n
    };
    ffi::log_line(js, &format!("{label}: {n}"));
    Ok(Value::undefined())
}

fn count_reset(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let label = label(scope, args)?;
    with_page(ffi::js_of(scope), |page| {
        page.counts.borrow_mut().remove(&label);
    });
    Ok(Value::undefined())
}

fn time(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let label = label(scope, args)?;
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::undefined());
    }
    page_or_new(js)
        .console_timers
        .borrow_mut()
        .get_or_insert_default()
        .insert(label, Instant::now());
    Ok(Value::undefined())
}

fn time_emit(scope: &mut Scope<'_>, js: Js, label: &str, extra: &[Value]) {
    if !ffi::log_enabled(js) {
        return;
    }
    let Some(started) = with_page(js, |page| {
        page.console_timers
            .borrow()
            .as_ref()
            .map(|timers| timers.get(label).copied())
    })
    .flatten() else {
        return;
    };
    let Some(started) = started else {
        ffi::log_line(js, &format!("Timer '{label}' does not exist"));
        return;
    };
    let ms = started.elapsed().as_secs_f64() * 1000.0;
    let prefix = format!("{label}: {ms:.3}ms");
    if extra.is_empty() {
        ffi::log_line(js, &prefix);
    } else {
        emit(scope, js, &prefix, extra);
    }
}

fn time_end(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let label = label(scope, args)?;
    let js = ffi::js_of(scope);
    time_emit(scope, js, &label, &[]);
    with_page(js, |page| {
        if let Some(timers) = page.console_timers.borrow_mut().as_mut() {
            timers.remove(&label);
        }
    });
    Ok(Value::undefined())
}

fn time_log(scope: &mut Scope<'_>, _: &Value, args: &[Value]) -> Result<Value, Value> {
    let label = label(scope, args)?;
    let js = ffi::js_of(scope);
    time_emit(scope, js, &label, args.get(1..).unwrap_or(&[]));
    Ok(Value::undefined())
}

fn bind(scope: &mut Scope<'_>, object: &Value, name: &str, f: NativeFn) {
    let function = scope.function(name, 0, f);
    let _ = scope.set(object, name, function);
}

pub(crate) fn install_namespace(
    scope: &mut Scope<'_>,
    global: &Value,
    name: &str,
    object: Value,
    tag: &str,
) {
    let prototype = scope.new_object();
    let _ = scope.set_prototype(&object, &prototype);
    let _ = scope.define_to_string_tag(&object, tag);
    let _ = scope.define(
        global,
        name,
        object,
        Attributes {
            writable: true,
            enumerable: false,
            configurable: true,
        },
    );
}

pub(crate) fn install(scope: &mut Scope<'_>, global: &Value) {
    let console = scope.new_object();
    for name in LOG_METHODS {
        bind(scope, &console, name, log);
    }
    bind(scope, &console, "warn", warn);
    bind(scope, &console, "error", error);
    bind(scope, &console, "assert", assert);
    bind(scope, &console, "count", count);
    bind(scope, &console, "countReset", count_reset);
    bind(scope, &console, "time", time);
    bind(scope, &console, "timeEnd", time_end);
    bind(scope, &console, "timeLog", time_log);
    for name in NOOP_METHODS {
        bind(scope, &console, name, navigator::noop);
    }
    let memory = scope.new_object();
    let _ = scope.set(&console, "memory", memory);
    install_namespace(scope, global, "console", console, "console");
}
