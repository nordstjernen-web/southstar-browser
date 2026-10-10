//! Southstar — the worker global scope: its console, performance clock, location and the WebIDL shape of the global object.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::{Scope, Value};

use crate::ffi;
use crate::{JsResult, bind, set, set_str};

const GLOBAL_SHAPE: &str = concat!(
    "(function(G, scopeName){",
    "  'use strict';",
    "  var def = Object.defineProperty, gopd = Object.getOwnPropertyDescriptor,",
    "      setProto = Object.setPrototypeOf;",
    "  var ET = G.EventTarget, WGS = G.WorkerGlobalScope, Scope = G[scopeName];",
    "  if (typeof ET !== 'function' || typeof WGS !== 'function' ||",
    "      typeof Scope !== 'function') return;",
    "  function illegal(){ return new TypeError('Illegal invocation'); }",
    "  function self_of(t){ var s = t === undefined || t === null ? G : t;",
    "    if (s !== G) throw illegal(); return s; }",
    "  function tag(C, n){ def(C.prototype, Symbol.toStringTag, { value: n, configurable: true }); }",
    "  function accessor(name, get, set){",
    "    var o = {}; def(o, name, { get: get, set: set, configurable: true });",
    "    return gopd(o, name); }",
    "  function getter(name, fn){ return gopd({ get [name](){ return fn.call(this); } }, name).get; }",
    "  function setter(name, fn){ return gopd({ set [name](v){ fn.call(this, v); } }, name).set; }",
    "  setProto(WGS.prototype, ET.prototype); setProto(WGS, ET);",
    "  setProto(Scope.prototype, WGS.prototype); setProto(Scope, WGS);",
    "  tag(ET, 'EventTarget'); tag(WGS, 'WorkerGlobalScope'); tag(Scope, scopeName);",
    "  try { delete G[Symbol.toStringTag]; } catch (e) {}",
    "  function take(name){ var d = gopd(G, name); if (d) delete G[name]; return d; }",
    "  function method(proto, name, length){",
    "    var d = proto === G ? gopd(G, name) : take(name);",
    "    if (!d || typeof d.value !== 'function') return;",
    "    def(d.value, 'length', { value: length, configurable: true });",
    "    def(proto, name, { value: d.value, writable: true, enumerable: true, configurable: true }); }",
    "  method(ET.prototype, 'addEventListener', 2);",
    "  method(ET.prototype, 'removeEventListener', 2);",
    "  method(ET.prototype, 'dispatchEvent', 1);",
    "  [['atob',1],['btoa',1],['clearInterval',0],['clearTimeout',0],['createImageBitmap',1],['fetch',1],",
    "   ['importScripts',0],['queueMicrotask',1],['reportError',1],['setInterval',1],",
    "   ['setTimeout',1],['structuredClone',1]].forEach(function(m){ method(WGS.prototype, m[0], m[1]); });",
    "  method(G, 'postMessage', 1); method(G, 'close', 0);",
    "  function wrap(cls, fields, names){",
    "    var C = G[cls]; if (typeof C !== 'function') return null;",
    "    var o = Object.create(C.prototype);",
    "    names.forEach(function(k){",
    "      if (!(k in fields)) return;",
    "      def(C.prototype, k, { get: getter(k, function(){ if (this !== o) throw illegal(); return fields[k]; }),",
    "                            enumerable: true, configurable: true }); });",
    "    tag(C, cls); return o; }",
    "  var loc = gopd(G, 'location'), nav = gopd(G, 'navigator');",
    "  if (loc && loc.value && typeof loc.value === 'object') {",
    "    var lf = {}; var lsrc = loc.value;",
    "    ['href','origin','protocol','host','hostname','port','pathname','search','hash'].forEach(function(k){ lf[k] = lsrc[k]; });",
    "    var L = wrap('WorkerLocation', lf, ['hash','host','hostname','href','origin','pathname','port','protocol','search']);",
    "    if (L) { def(G.WorkerLocation.prototype, 'toString', { value: function toString(){",
    "        if (this !== L) throw illegal(); return lf.href; }, writable: true, enumerable: true, configurable: true });",
    "      def(G, 'location', { value: L, writable: true, enumerable: true, configurable: true }); } }",
    "  var uad = nav && nav.value && gopd(nav.value, 'userAgentData');",
    "  if (uad && uad.value && typeof G.NavigatorUAData === 'function') {",
    "    var uf = uad.value, UP = G.NavigatorUAData.prototype, U = Object.create(UP);",
    "    ['brands','mobile','platform'].forEach(function(k){ var v = uf[k];",
    "      def(UP, k, { get: getter(k, function(){ if (this !== U) throw illegal(); return v; }),",
    "                   enumerable: true, configurable: true }); });",
    "    ['getHighEntropyValues','toJSON'].forEach(function(k){ var f = uf[k];",
    "      if (typeof f === 'function') def(UP, k, { value: f, writable: true, enumerable: true, configurable: true }); });",
    "    tag(G.NavigatorUAData, 'NavigatorUAData');",
    "    def(nav.value, 'userAgentData', { value: U, writable: true, enumerable: true, configurable: true }); }",
    "  if (nav && nav.value && typeof nav.value === 'object') {",
    "    var nf = {}, nsrc = nav.value;",
    "    Object.getOwnPropertyNames(nsrc).forEach(function(k){ var d = gopd(nsrc, k); if (d && 'value' in d) nf[k] = d.value; });",
    "    var Nv = wrap('WorkerNavigator', nf, ['appCodeName','appName','appVersion','connection','deviceMemory',",
    "      'hardwareConcurrency','language','languages','locks','mediaCapabilities','onLine','permissions',",
    "      'platform','product','storage','userAgent','userAgentData']);",
    "    if (Nv) def(G, 'navigator', { value: Nv, writable: true, enumerable: true, configurable: true }); }",
    "  function readonly(name, replaceable){",
    "    var d = take(name); if (!d || !('value' in d)) { if (d) def(G, name, d); return; }",
    "    var value = d.value;",
    "    def(WGS.prototype, name, { get: getter(name, function(){ self_of(this); return value; }),",
    "      set: replaceable ? setter(name, function(v){ def(self_of(this), name,",
    "        { value: v, writable: true, enumerable: true, configurable: true }); }) : undefined,",
    "      enumerable: true, configurable: true }); }",
    "  ['caches','crossOriginIsolated','crypto','indexedDB','isSecureContext','location',",
    "   'navigator'].forEach(function(n){ readonly(n, false); });",
    "  ['origin','performance'].forEach(function(n){ readonly(n, true); });",
    "  take('self');",
    "  def(WGS.prototype, 'self', { get: getter('self', function(){ return self_of(this); }),",
    "    enumerable: true, configurable: true });",
    "  function handler(target, name){",
    "    var d = target === G ? gopd(G, name) : take(name);",
    "    var slot = d && 'value' in d && d.value !== undefined ? d.value : null;",
    "    def(target, name, {",
    "      get: getter(name, function(){ self_of(this); return slot; }),",
    "      set: setter(name, function(v){ self_of(this);",
    "        slot = typeof v === 'function' || (typeof v === 'object' && v !== null) ? v : null; }),",
    "      enumerable: true, configurable: true }); }",
    "  ['onerror','onlanguagechange','onrejectionhandled','onunhandledrejection'].forEach(function(n){",
    "    handler(WGS.prototype, n); });",
    "  if (scopeName === 'DedicatedWorkerGlobalScope') {",
    "    handler(G, 'onmessage'); handler(G, 'onmessageerror');",
    "    var nd = gopd(G, 'name'); var nameValue = nd && 'value' in nd ? nd.value : '';",
    "    def(G, 'name', { get: getter('name', function(){ self_of(this); return nameValue; }),",
    "      set: setter('name', function(v){ def(self_of(this), 'name',",
    "        { value: v, writable: true, enumerable: true, configurable: true }); }),",
    "      enumerable: true, configurable: true });",
    "    var rafId = 0, rafCallbacks = new Map(), rafTimer = 0;",
    "    function rafFlush(){ rafTimer = 0; var cbs = rafCallbacks; rafCallbacks = new Map();",
    "      var ts = G.performance.now();",
    "      cbs.forEach(function(cb){ try { cb.call(G, ts); } catch (e) { G.reportError(e); } }); }",
    "    def(G, 'requestAnimationFrame', { value: function requestAnimationFrame(cb){",
    "        if (typeof cb !== 'function') throw new TypeError(\"Failed to execute 'requestAnimationFrame' \" +",
    "          \"on 'DedicatedWorkerGlobalScope': The callback provided as parameter 1 is not a function.\");",
    "        var id = ++rafId; rafCallbacks.set(id, cb);",
    "        if (!rafTimer) rafTimer = G.setTimeout(rafFlush, 16);",
    "        return id; }, writable: true, enumerable: true, configurable: true });",
    "    def(G, 'cancelAnimationFrame', { value: function cancelAnimationFrame(id){",
    "        rafCallbacks.delete(Number(id)); }, writable: true, enumerable: true, configurable: true });",
    "  }",
    "  try { delete G.NodeFilter; } catch (e) {}",
    "  setProto(G, Scope.prototype);",
    "})",
);

fn console_emit(scope: &mut Scope<'_>, prefix: &str, args: &[Value]) {
    let js = ffi::js_of(scope);
    if js.is_null() {
        return;
    }
    let mut line = prefix.to_owned();
    for (i, value) in args.iter().enumerate() {
        if i > 0 || !prefix.is_empty() {
            line.push(' ');
        }
        if let Some(text) = crate::text(scope, value) {
            line.push_str(&text);
        }
    }
    ffi::log_line(js, &line);
}

fn console_log(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    console_emit(scope, "", args);
    Ok(Value::undefined())
}

fn console_warn(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    console_emit(scope, "[warn]", args);
    Ok(Value::undefined())
}

fn console_error(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    console_emit(scope, "[error]", args);
    Ok(Value::undefined())
}

fn console_assert(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    if args.is_empty() || scope.to_bool(&args[0]) {
        return Ok(Value::undefined());
    }
    console_emit(scope, "[assert]", &args[1..]);
    Ok(Value::undefined())
}

pub(crate) fn unhandled_rejection(scope: &mut Scope<'_>, reason: &Value) {
    console_emit(
        scope,
        "[unhandled rejection]",
        core::slice::from_ref(reason),
    );
}

pub(crate) fn install_console(scope: &mut Scope<'_>, global: &Value) {
    let console = scope.new_object();
    for name in [
        "log",
        "info",
        "debug",
        "trace",
        "table",
        "group",
        "groupCollapsed",
        "dir",
        "dirxml",
    ] {
        bind(scope, &console, name, 0, console_log);
    }
    bind(scope, &console, "warn", 0, console_warn);
    bind(scope, &console, "error", 0, console_error);
    bind(scope, &console, "assert", 0, console_assert);
    for name in [
        "count",
        "countReset",
        "time",
        "timeEnd",
        "timeLog",
        "groupEnd",
        "profile",
        "profileEnd",
        "timeStamp",
        "context",
        "clear",
    ] {
        bind(scope, &console, name, 0, crate::noop);
    }
    let memory = scope.new_object();
    set(scope, &console, "memory", memory);
    ffi::install_namespace_object(scope, global, "console", console, "console");
}

pub(crate) fn performance_now(scope: &mut Scope<'_>, _this: &Value, _args: &[Value]) -> JsResult {
    let js = ffi::js_of(scope);
    Ok(Value::number(if js.is_null() {
        0.0
    } else {
        ffi::worker_now_ms(js)
    }))
}

pub(crate) fn performance_entries(
    scope: &mut Scope<'_>,
    _this: &Value,
    _args: &[Value],
) -> JsResult {
    Ok(scope.new_array())
}

pub(crate) fn install_location(scope: &mut Scope<'_>, global: &Value, url: &str) {
    let location = scope.new_object();
    let parts = ffi::url_parts(url);
    set_str(scope, &location, "href", url);
    for (key, value) in [
        ("origin", parts.origin),
        ("protocol", parts.protocol),
        ("host", parts.host),
        ("hostname", parts.hostname),
        ("port", parts.port),
        ("pathname", parts.pathname),
        ("search", parts.search),
        ("hash", parts.hash),
    ] {
        set_str(scope, &location, key, &value);
    }
    set(scope, global, "location", location);
}

pub(crate) fn report_error(scope: &mut Scope<'_>, _this: &Value, args: &[Value]) -> JsResult {
    let Some(error) = args.first() else {
        return Err(scope.type_error("reportError: 1 argument required"));
    };
    crate::worker::report_exception(ffi::js_of(scope), error);
    Ok(Value::undefined())
}

pub(crate) fn shape_global(scope: &mut Scope<'_>, service_worker: bool) {
    if let Ok(shape) = scope.eval_native_script(GLOBAL_SHAPE, "<worker-global-shape>")
        && scope.is_function(&shape)
    {
        let global = scope.global();
        let name = scope.string(if service_worker {
            "ServiceWorkerGlobalScope"
        } else {
            "DedicatedWorkerGlobalScope"
        });
        let _ = scope.call(&shape, &Value::undefined(), &[global, name]);
    }
    ffi::link_interface_ctors(scope);
    ffi::lock_global_prototypes(scope);
}
