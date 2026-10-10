//! Southstar — uncaught errors and unhandled rejections: the ErrorEvent fired at the window, reportError(), and the unhandledrejection and rejectionhandled events with their console fallback.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::quickjs::{self, JSContext};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js};
use crate::{JsResult, get, set};

const HIDDEN: Attributes = Attributes {
    writable: false,
    enumerable: false,
    configurable: false,
};

pub(crate) struct Rejection {
    seq: u64,
    ctx: *mut JSContext,
    promise: Value,
    reason: Value,
}

pub(crate) fn exception_message(scope: &mut Scope<'_>, exception: &Value) -> String {
    scope
        .to_string(exception)
        .unwrap_or_else(|_| "Script error.".to_owned())
}

fn interface_prototype(scope: &mut Scope<'_>, global: &Value, name: &str) -> Value {
    let ctor = get(scope, global, name);
    if ctor.is_object() {
        get(scope, &ctor, "prototype")
    } else {
        Value::undefined()
    }
}

pub(crate) fn adopt_global_proto(scope: &mut Scope<'_>, object: &Value, name: &str) {
    let global = scope.global();
    let proto = interface_prototype(scope, &global, name);
    if proto.is_object() {
        let _ = scope.set_prototype(object, &proto);
    }
}

pub(crate) fn report_error_event(
    js: Js,
    message: Option<&str>,
    filename: Option<&str>,
    lineno: i32,
    colno: i32,
    error: &Value,
) -> bool {
    if js.is_null() || js.ctx().is_null() || crate::peek(js, |page| page.in_error_report) {
        return false;
    }
    crate::with(js, |page| page.in_error_report = true);
    let event = js.scope(|scope| {
        let event = ffi::make_event(scope, "error", None);
        set(scope, &event, "bubbles", Value::boolean(false));
        set(scope, &event, "cancelable", Value::boolean(true));
        let _ = scope.define(&event, "__ndErrorEvent", Value::boolean(true), HIDDEN);
        let global = scope.global();
        set(scope, &event, "target", global);
        adopt_global_proto(scope, &event, "ErrorEvent");
        let message = scope.string(message.unwrap_or(""));
        set(scope, &event, "message", message);
        let filename = scope.string(filename.unwrap_or(""));
        set(scope, &event, "filename", filename);
        set(scope, &event, "lineno", Value::int(lineno));
        set(scope, &event, "colno", Value::int(colno));
        set(scope, &event, "error", error.clone());
        event
    });
    let prevented = match event {
        Some(event) => {
            crate::dispatch::dispatch_window_only(js, js.current_document(), "error", event)
        }
        None => false,
    };
    crate::with(js, |page| page.in_error_report = false);
    prevented
}

pub(crate) fn report_exception_at(
    js: Js,
    exception: &Value,
    filename: Option<&str>,
    lineno: i32,
    colno: i32,
) -> bool {
    if js.is_null() || js.ctx().is_null() || crate::peek(js, |page| page.in_error_report) {
        return false;
    }
    if js.is_worker() {
        return js.worker_report_exception(exception);
    }
    let Some(message) = js.scope(|scope| exception_message(scope, exception)) else {
        return false;
    };
    report_error_event(js, Some(&message), filename, lineno, colno, exception)
}

pub(crate) fn report_handler_exception(
    js: Js,
    scope: &mut Scope<'_>,
    kind: Option<&str>,
    exception: &Value,
) {
    let message = scope.to_string(exception).ok();
    if !js.is_null() && js.log_enabled() {
        let stack = get(scope, exception, "stack");
        let stack = scope.to_string(&stack).ok().unwrap_or_default();
        let separator = if stack.is_empty() { "" } else { "\n" };
        js.log(&format!(
            "JS error in {} handler: {}{separator}{stack}",
            kind.unwrap_or("event"),
            message.as_deref().unwrap_or("exception"),
        ));
    }
    if !js.is_null() {
        report_exception_at(js, exception, Some("event-handler"), 0, 0);
    }
}

fn stack_line_location(line: &str) -> Option<&str> {
    let line = line.trim_start_matches([' ', '\t']);
    let rest = line.strip_prefix("at ")?;
    if rest.ends_with(')')
        && let Some(open) = rest.find('(')
    {
        return Some(&rest[open + 1..rest.len() - 1]);
    }
    Some(rest)
}

fn leading_number(text: &str) -> i32 {
    let digits = text.bytes().take_while(u8::is_ascii_digit).count();
    text[..digits].parse().unwrap_or(0)
}

fn parse_location(location: &str) -> Option<(String, i32, i32)> {
    let c2 = location.rfind(':')?;
    let c1 = location[..c2].rfind(':')?;
    let starts_digit = |at: usize| {
        location[at + 1..]
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_digit())
    };
    if !starts_digit(c1) || !starts_digit(c2) {
        return None;
    }
    Some((
        location[..c1].to_owned(),
        leading_number(&location[c1 + 1..]),
        leading_number(&location[c2 + 1..]),
    ))
}

fn caller_position(scope: &mut Scope<'_>) -> Option<(String, i32, i32)> {
    let error = scope.new_error();
    let stack = get(scope, &error, "stack");
    if !stack.is_string() {
        return None;
    }
    let text = scope.to_string(&stack).ok()?;
    text.split('\n')
        .filter(|line| !line.is_empty())
        .find_map(|line| stack_line_location(line).and_then(parse_location))
}

pub(crate) fn report_error(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(error) = args.first() else {
        return Err(scope.type_error("reportError: 1 argument required"));
    };
    let js = ffi::js_of(scope);
    if js.is_null() {
        return Ok(Value::undefined());
    }
    let position = caller_position(scope);
    let (file, line, col) = match &position {
        Some((file, line, col)) => (Some(file.clone()), *line, *col),
        None => (None, 0, 0),
    };
    let filename = file.clone().or_else(|| js.current_url());
    let prevented = report_exception_at(js, error, filename.as_deref(), line, col);
    if !prevented && js.log_enabled() {
        let message = exception_message(scope, error);
        js.log(&format!(
            "Uncaught {message} ({}:{line}:{col})",
            file.as_deref().unwrap_or("")
        ));
    }
    Ok(Value::undefined())
}

fn dispatch_rejection_event(
    js: Js,
    ctx: *mut JSContext,
    kind: &str,
    promise: &Value,
    reason: &Value,
    cancelable: bool,
) -> bool {
    let (event, target_doc) = ffi::in_context(ctx, |scope| {
        let global = scope.global();
        let event = crate::target::make_event(scope, &global, kind);
        adopt_global_proto(scope, &event, "PromiseRejectionEvent");
        set(scope, &event, "promise", promise.clone());
        set(scope, &event, "reason", reason.clone());
        set(scope, &event, "cancelable", Value::boolean(cancelable));
        let doc = ffi::window_document_for(scope, &global);
        (event, doc)
    });
    crate::dispatch::dispatch_window_only(js, target_doc, kind, event)
}

pub(crate) fn track_rejection(
    scope: &mut Scope<'_>,
    promise: &Value,
    reason: &Value,
    handled: bool,
) {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return;
    }
    let key = quickjs::identity(promise);
    if handled {
        let pending = crate::with(js, |page| page.pending_rejections.remove(&key));
        if pending.is_some() {
            return;
        }
        let reported = crate::with(js, |page| page.reported_rejections.remove(&key));
        if reported.is_some() {
            dispatch_rejection_event(
                js,
                ffi::ctx_of(scope),
                "rejectionhandled",
                promise,
                reason,
                false,
            );
        }
        return;
    }
    let rejection = Rejection {
        seq: 0,
        ctx: ffi::ctx_of(scope),
        promise: promise.clone(),
        reason: reason.clone(),
    };
    let replaced = crate::with(js, |page| {
        page.rejection_seq += 1;
        let rejection = Rejection {
            seq: page.rejection_seq,
            ..rejection
        };
        page.pending_rejections.insert(key, rejection)
    });
    drop(replaced);
}

pub(crate) fn report_pending_rejections(js: Js) {
    let batch = crate::with(js, |page| std::mem::take(&mut page.pending_rejections));
    if batch.is_empty() {
        return;
    }
    let mut batch: Vec<(usize, Rejection)> = batch.into_iter().collect();
    batch.sort_by_key(|(_, rejection)| rejection.seq);
    for (key, rejection) in batch {
        let prevented = dispatch_rejection_event(
            js,
            rejection.ctx,
            "unhandledrejection",
            &rejection.promise,
            &rejection.reason,
            true,
        );
        let ctx = rejection.ctx;
        let reason = rejection.reason.clone();
        let replaced = crate::with(js, |page| page.reported_rejections.insert(key, rejection));
        drop(replaced);
        if prevented || !js.log_enabled() {
            continue;
        }
        let prefix = "[unhandled rejection] ";
        ffi::in_context(ctx, |scope| {
            if scope.is_error(&reason) || (reason.is_object() && !scope.is_function(&reason)) {
                js.console_emit(scope, prefix, &reason);
            } else {
                let text = scope.to_string(&reason).unwrap_or_default();
                js.log(&format!("{prefix}{text}"));
            }
        });
    }
}

pub(crate) fn drop_rejections(js: Js) {
    let taken = crate::with(js, |page| {
        (
            std::mem::take(&mut page.pending_rejections),
            std::mem::take(&mut page.reported_rejections),
        )
    });
    drop(taken);
}
