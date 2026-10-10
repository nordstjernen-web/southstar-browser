//! Southstar — what a frame's scripts run in the page realm may change and the page gets back afterwards: the intrinsic prototypes' enumerable keys, Zone, and the native event, timer and custom element bindings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_js_engine::Value;

use crate::ffi::qjs::Ctx;
use crate::ffi::{self, Binding, Js};

const PROTO_SNAPSHOT: &str = "(function(){var p=[Object.prototype,Array.prototype,Function.prototype,String.prototype,Number.prototype,Boolean.prototype,RegExp.prototype,Date.prototype,typeof Promise==='function'?Promise.prototype:null,typeof Map==='function'?Map.prototype:null,typeof Set==='function'?Set.prototype:null,typeof WeakMap==='function'?WeakMap.prototype:null,typeof WeakSet==='function'?WeakSet.prototype:null];return p.map(function(o){var m=Object.create(null);if(!o)return m;Object.getOwnPropertyNames(o).forEach(function(k){try{if(Object.prototype.propertyIsEnumerable.call(o,k))m[k]=1;}catch(e){}});return m;});})()";

const PROTO_CLEANUP: &str = "(function(b){var p=[Object.prototype,Array.prototype,Function.prototype,String.prototype,Number.prototype,Boolean.prototype,RegExp.prototype,Date.prototype,typeof Promise==='function'?Promise.prototype:null,typeof Map==='function'?Map.prototype:null,typeof Set==='function'?Set.prototype:null,typeof WeakMap==='function'?WeakMap.prototype:null,typeof WeakSet==='function'?WeakSet.prototype:null];for(var i=0;i<p.length;i++){var o=p[i],m=b&&b[i];if(!o)continue;Object.getOwnPropertyNames(o).forEach(function(k){try{if(m&&m[k])return;if(!Object.prototype.propertyIsEnumerable.call(o,k))return;var d=Object.getOwnPropertyDescriptor(o,k);if(!d||d.configurable===false)return;d.enumerable=false;Object.defineProperty(o,k,d);}catch(e){}});}})";

const EVENT_TARGET_METHODS: &str = "(function(){var ET=typeof EventTarget!=='undefined'&&EventTarget.prototype;var m=globalThis.__ndEventTargetMethods;if(!ET||!m)return;[['addEventListener',m.add],['removeEventListener',m.remove],['dispatchEvent',m.dispatch]].forEach(function(e){Object.defineProperty(ET,e[0],{value:e[1],writable:true,enumerable:true,configurable:true});});})()";

const EVENT_CARRIERS: &[&str] = &[
    "Node",
    "Element",
    "HTMLElement",
    "SVGElement",
    "SVGAElement",
    "SVGSVGElement",
    "Document",
    "HTMLDocument",
    "DocumentFragment",
];

pub(crate) fn proto_snapshot(ctx: Ctx) -> Value {
    ctx.eval_hidden(PROTO_SNAPSHOT, c"<iframe-proto-snapshot>")
        .unwrap_or_else(Value::null)
}

pub(crate) fn proto_cleanup(ctx: Ctx, before: &Value) {
    if !before.is_object() {
        return;
    }
    let Some(cleanup) = ctx.eval_hidden(PROTO_CLEANUP, c"<iframe-proto-cleanup>") else {
        return;
    };
    if ctx.is_function(&cleanup) {
        drop(ctx.call(&cleanup, &Value::undefined(), core::slice::from_ref(before)));
    }
}

pub(crate) fn clear_global_zone(ctx: Ctx) {
    let global = ctx.global();
    if ctx.delete(&global, c"Zone") <= 0 {
        ctx.set(&global, "Zone", Value::undefined());
    }
}

fn restore_custom_elements(ctx: Ctx, global: &Value) {
    let registry = ctx.get(global, "customElements");
    if !registry.is_object() {
        return;
    }
    for binding in [
        Binding::CustomElementsDefine,
        Binding::CustomElementsGet,
        Binding::CustomElementsUpgrade,
        Binding::CustomElementsWhenDefined,
        Binding::CustomElementsGetName,
    ] {
        ffi::bind(ctx, &registry, binding);
    }
}

fn restore_event_targets(ctx: Ctx, global: &Value) {
    for binding in [
        Binding::WindowAddEventListener,
        Binding::WindowRemoveEventListener,
        Binding::WindowDispatchEvent,
    ] {
        ffi::bind(ctx, global, binding);
    }
    let adopt = ctx.get(global, "__ndAdoptWindowEventOps");
    if ctx.is_function(&adopt) {
        drop(ctx.call(&adopt, &Value::undefined(), core::slice::from_ref(global)));
    }
    for carrier in EVENT_CARRIERS {
        let constructor = ctx.get(global, carrier);
        if !constructor.is_object() {
            continue;
        }
        let proto = ctx.get(&constructor, "prototype");
        if !proto.is_object() {
            continue;
        }
        for binding in [
            Binding::ElementAddEventListener,
            Binding::ElementRemoveEventListener,
            Binding::ElementDispatchEvent,
        ] {
            ffi::bind(ctx, &proto, binding);
        }
    }
    drop(ctx.eval_hidden(EVENT_TARGET_METHODS, c"<iframe-events>"));
}

fn restore_scheduling(ctx: Ctx, global: &Value) {
    let js = Js::of_ctx(ctx);
    if !js.is_null() {
        let promise = ffi::pristine_promise(js, ctx);
        if promise.is_object() {
            ctx.set(global, "Promise", promise);
        }
    }
    for binding in [
        Binding::SetTimeout,
        Binding::SetInterval,
        Binding::ClearTimeout,
        Binding::ClearInterval,
        Binding::RequestAnimationFrame,
        Binding::CancelAnimationFrame,
        Binding::QueueMicrotask,
    ] {
        ffi::bind(ctx, global, binding);
    }
    ffi::bind_post_message(ctx, global);
    ctx.delete(global, c"setImmediate");
    ctx.delete(global, c"clearImmediate");
}

pub(crate) fn restore_globals(ctx: Ctx) {
    let global = ctx.global();
    restore_custom_elements(ctx, &global);
    restore_event_targets(ctx, &global);
    restore_scheduling(ctx, &global);
}
