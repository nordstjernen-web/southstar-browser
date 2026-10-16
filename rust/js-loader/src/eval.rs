//! Southstar — script evaluation: classic scripts and modules in the page, frame and module realms, the module loader, content scripts and the error lines they log.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::{Kind, Node};
use southstar_js_engine::{PromiseState, Scope, ScriptOptions, Value};

use crate::compile;
use crate::ffi::{Js, Realm};
use crate::schedule;

const CONTENT_SCRIPT_NATIVES: [&str; 6] = [
    "__nd_ext_manifest",
    "__nd_ext_base",
    "__nd_ext_sread",
    "__nd_ext_swrite",
    "__nd_ext_platform",
    "__nd_ext_uilang",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScriptKind {
    Classic,
    InlineModule,
    ExternalModule,
}

impl ScriptKind {
    pub(crate) fn of(is_module: bool, external: bool) -> ScriptKind {
        match (is_module, external) {
            (false, _) => ScriptKind::Classic,
            (true, false) => ScriptKind::InlineModule,
            (true, true) => ScriptKind::ExternalModule,
        }
    }
}

fn source_text(source: &[u8]) -> Vec<u8> {
    let mut text = source.to_vec();
    if let Some(end) = text.iter().position(|&b| b == 0) {
        text[end..].fill(0);
    }
    text
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn c_message(scope: &mut Scope<'_>, value: &Value) -> Option<Vec<u8>> {
    scope
        .to_bytes(value)
        .ok()
        .map(|bytes| until_nul(&bytes).to_vec())
}

fn origin_is_engine_code(origin: Option<&CStr>) -> bool {
    origin.is_some_and(|origin| {
        let origin = origin.to_bytes();
        origin.first() == Some(&b'<') && origin != b"<inline>" && origin != b"<timer>"
    })
}

fn or_default<'a>(origin: Option<&'a CStr>, fallback: &'a CStr) -> &'a [u8] {
    origin.unwrap_or(fallback).to_bytes()
}

fn leave_eval(js: Js) {
    if js.eval_depth_add(-1) == 0 {
        js.flush_document_write();
        schedule::schedule_pending_drain(js);
    }
}

fn log_error_with_stack(
    js: Js,
    scope: &mut Scope<'_>,
    exception: &Value,
    origin: &[u8],
    skip_empty_stack: bool,
) {
    let Some(message) = c_message(scope, exception) else {
        return;
    };
    if !js.has_log() {
        return;
    }
    let stack = scope
        .get(exception, "stack")
        .ok()
        .and_then(|stack| c_message(scope, &stack));
    let mut line = b"JS error in ".to_vec();
    line.extend_from_slice(origin);
    line.extend_from_slice(b": ");
    line.extend_from_slice(&message);
    if let Some(stack) = stack {
        if !skip_empty_stack || !stack.is_empty() {
            line.push(b'\n');
        }
        line.extend_from_slice(&stack);
    }
    js.log(&line);
}

fn eval_in_iframe_document(
    js: Js,
    scope: &mut Scope<'_>,
    document: Value,
    src: &[u8],
    origin: Option<&CStr>,
    hide_source: bool,
) -> bool {
    let mut wrapped = b"(function(document){\n".to_vec();
    wrapped.extend_from_slice(src);
    wrapped.extend_from_slice(b"\n})");
    let name = or_default(origin, c"inline");
    let options = ScriptOptions {
        name: Some(name),
        line: 1,
        hide_source,
        ..ScriptOptions::default()
    };
    let Ok(function) = scope.eval_script_bytes(&wrapped, &options) else {
        return false;
    };
    if !scope.is_function(&function) {
        return false;
    }
    let global = scope.global();
    js.eval_depth_add(1);
    let result = scope.call(&function, &global, &[document]);
    leave_eval(js);
    if let Err(exception) = result {
        log_error_with_stack(js, scope, &exception, name, false);
    }
    true
}

pub(crate) fn eval(js: Js, src: &[u8], origin: Option<&CStr>) {
    let hide_source = origin_is_engine_code(origin);
    let budget = js.budget_enter();
    js.main_realm().enter(|scope| {
        if let Some(document) = js.iframe_document()
            && eval_in_iframe_document(js, scope, document, src, origin, hide_source)
        {
            return;
        }
        eval_classic(js, scope, src, origin, hide_source);
    });
    js.budget_leave(budget);
}

fn eval_classic(
    js: Js,
    scope: &mut Scope<'_>,
    src: &[u8],
    origin: Option<&CStr>,
    hide_source: bool,
) {
    let copy = source_text(src);
    let started = compile::profile_begin();
    js.eval_depth_add(1);
    let inline_script = js
        .current_script()
        .filter(|script| script.attr(c"src").is_none());
    let (line, column) = inline_script.map_or((1, 1), |script| {
        let (line, column) = script.source_position();
        (
            if line > 0 { line } else { 1 },
            if column > 0 { column } else { 1 },
        )
    });
    let cached = compile::cached_script(scope, &copy);
    let cache_hit = cached.is_some();
    let result = match cached {
        Some(function) => scope.evaluate(function),
        None => {
            let options = ScriptOptions {
                name: origin.map(CStr::to_bytes),
                line,
                column,
                hide_source,
            };
            match scope.compile_script(&copy, &options) {
                Ok(function) => {
                    compile::store_script(scope, &function, &copy);
                    scope.evaluate(function)
                }
                Err(error) => Err(error),
            }
        }
    };
    drop(copy);
    leave_eval(js);
    compile::profile_eval(started, src.len(), cache_hit, origin);
    if let Err(exception) = &result {
        log_error_with_stack(js, scope, exception, or_default(origin, c"inline"), true);
        js.report_uncaught(exception, origin);
    }
    drop(result);
    js.drain_microtasks();
}

pub(crate) fn compile_module_cached(
    scope: &mut Scope<'_>,
    src: &[u8],
    name: &[u8],
) -> Result<Value, Value> {
    if let Some(cached) = compile::cached_module(scope, name, src) {
        return cached;
    }
    let module = scope.compile_module(src, name)?;
    compile::store_module(scope, &module, name, src);
    compile::set_import_meta(scope, &module, false)?;
    Ok(module)
}

pub(crate) fn load_module(
    js: Js,
    scope: &mut Scope<'_>,
    name: &CStr,
    json: bool,
) -> Result<Value, Value> {
    let body = crate::fetch::module_source(js, scope, name, json)?;
    let module = if json {
        let text = source_text(&body);
        let label = String::from_utf8_lossy(name.to_bytes()).into_owned();
        scope
            .parse_json(&text, &label)
            .and_then(|value| scope.value_module(name.to_bytes(), value))
    } else {
        compile_module_cached(scope, &body, name.to_bytes())
    };
    drop(body);
    if let Err(exception) = &module
        && !js.is_null()
        && js.has_log()
    {
        let message = c_message(scope, exception).unwrap_or_else(|| b"(no message)".to_vec());
        let mut line = b"module compile failed ".to_vec();
        line.extend_from_slice(name.to_bytes());
        line.extend_from_slice(b": ");
        line.extend_from_slice(&message);
        js.log(&line);
    }
    module
}

fn script_module(
    scope: &mut Scope<'_>,
    source: &[u8],
    name: &[u8],
    external: bool,
) -> Result<Value, Value> {
    let specifier = String::from_utf8_lossy(name);
    if external && let Some(module) = scope.mapped_module(&specifier) {
        return Ok(module);
    }
    let module = compile_module_cached(scope, source, name)?;
    compile::set_import_meta(scope, &module, true)?;
    if external {
        scope.map_module(&specifier, &module);
    }
    Ok(module)
}

pub(crate) fn eval_module(js: Js, src: &[u8], origin: Option<&CStr>, external: bool) {
    let budget = js.budget_enter();
    js.module_realm().enter(|scope| {
        let name = or_default(origin, c"module");
        let copy = source_text(src);
        let started = compile::profile_begin();
        js.eval_depth_add(1);
        let module = script_module(scope, &copy, name, external);
        js.ignore_destructive_writes_add(1);
        let result = module.and_then(|module| scope.evaluate(module));
        js.ignore_destructive_writes_add(-1);
        drop(copy);
        leave_eval(js);
        compile::profile_module(started, src.len(), origin);
        match &result {
            Err(exception) => {
                if let Some(message) = c_message(scope, exception)
                    && js.has_log()
                {
                    let mut line = b"JS module error in ".to_vec();
                    line.extend_from_slice(name);
                    line.extend_from_slice(b": ");
                    line.extend_from_slice(&message);
                    js.log(&line);
                }
            }
            Ok(promise) => {
                if let PromiseState::Rejected(reason) = scope.promise_state(promise)
                    && let Some(message) = c_message(scope, &reason)
                    && js.has_log()
                {
                    let mut line = b"JS module rejected in ".to_vec();
                    line.extend_from_slice(name);
                    line.extend_from_slice(b": ");
                    line.extend_from_slice(&message);
                    js.log(&line);
                }
            }
        }
        drop(result);
    });
    js.drain_microtasks();
    js.budget_leave(budget);
}

fn eval_in_realm(js: Js, realm: Realm, source: &[u8], origin: Option<&CStr>) {
    let budget = js.budget_enter();
    let copy = source_text(source);
    js.eval_depth_add(1);
    realm.enter(|scope| {
        let name = or_default(origin, c"inline");
        let options = ScriptOptions {
            name: Some(name),
            line: 1,
            ..ScriptOptions::default()
        };
        let result = scope.eval_script_bytes(&copy, &options);
        leave_eval(js);
        if let Err(exception) = &result {
            log_error_with_stack(js, scope, exception, name, true);
        }
    });
    js.drain_microtasks();
    js.budget_leave(budget);
}

pub(crate) fn eval_script_source(
    js: Js,
    script: Node,
    source: &[u8],
    origin: Option<&CStr>,
    kind: ScriptKind,
) {
    let is_module = kind != ScriptKind::Classic;
    let external = kind == ScriptKind::ExternalModule;
    let realm = js.node_realm(script);
    let Some(realm) = realm.filter(|&realm| realm != js.main_realm()) else {
        if is_module {
            eval_module(js, source, origin, external);
        } else {
            let previous = js.swap_current_script(Some(script));
            eval(js, source, origin);
            js.swap_current_script(previous);
        }
        return;
    };
    let document = core::iter::successors(Some(script), |n| n.parent())
        .find(|n| matches!(n.kind(), Kind::Document));
    let previous_doc = js.current_doc();
    js.swap_current_doc(document.or(previous_doc));
    let previous_script = js.swap_current_script(Some(script));
    let frame_url = js.frame_url_push(origin);
    if is_module {
        let previous_module_realm = js.swap_module_realm(Some(realm));
        eval_module(js, source, origin, external);
        js.swap_module_realm(previous_module_realm);
    } else {
        eval_in_realm(js, realm, source, origin);
    }
    js.frame_url_pop(frame_url);
    js.swap_current_script(previous_script);
    js.swap_current_doc(previous_doc);
}

pub(crate) fn eval_source(js: Js, src: &CStr, origin: Option<&CStr>) -> Option<Vec<u8>> {
    if js.halted() || js.in_pump() {
        return None;
    }
    let budget = js.budget_enter();
    let out = js.main_realm().enter(|scope| {
        let options = ScriptOptions {
            name: Some(or_default(origin, c"console")),
            line: 1,
            ..ScriptOptions::default()
        };
        match scope.eval_script_bytes(src.to_bytes(), &options) {
            Err(exception) => {
                let message =
                    c_message(scope, &exception).unwrap_or_else(|| b"(no message)".to_vec());
                let mut out = b"error: ".to_vec();
                out.extend_from_slice(&message);
                out
            }
            Ok(value) => c_message(scope, &value).unwrap_or_else(|| b"undefined".to_vec()),
        }
    });
    js.drain_microtasks();
    js.budget_leave(budget);
    Some(out)
}

pub(crate) fn run_content_scripts(js: Js, url: Option<&CStr>, document_start: bool) {
    if let Some(script) = js.content_scripts_for_url(url, document_start) {
        run_content_script(js, &script);
    }
}

fn run_content_script(js: Js, src: &CString) {
    drop(eval_source(js, src, Some(c"content-script")));
    if js.is_null() {
        return;
    }
    js.with_main_scope(|scope| {
        let global = scope.global();
        for name in CONTENT_SCRIPT_NATIVES {
            let _ = scope.delete(&global, name);
        }
    });
}

pub(crate) fn purge_subtree_script_refs(js: Js, root: Node) {
    js.forget_script_roots_in(root);
    js.remove_lifecycle_tasks(|task| {
        core::iter::successors(task, |n| n.parent()).any(|n| n.as_ptr() == root.as_ptr())
    });
}
