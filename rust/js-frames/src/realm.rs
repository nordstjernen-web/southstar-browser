//! Southstar — frame realms and scopes: the global bootstrap that makes a frame's own window, the scope a frame's scripts see when they run in the page realm, the cross-origin window proxy and a frame's module scripts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::Node;
use southstar_js_engine::Value;

use crate::bootstrap;
use crate::cloner;
use crate::ffi::qjs::{Ctx, PROP_CONFIGURABLE, PROP_WRITABLE};
use crate::ffi::{self, Js};
use crate::page::{self, Frame};
use crate::sandbox;

pub(crate) struct FrameRealm {
    pub ctx: Ctx,
    pub window: Value,
    pub location: Value,
    pub history: Value,
}

pub(crate) fn frame_of(node: Node<'_>) -> Frame {
    Frame(node.as_ptr() as usize)
}

fn text_or_undefined(ctx: Ctx, text: Option<&[u8]>) -> Value {
    match text {
        Some(text) if !text.is_empty() => ctx.enter(|scope| scope.string_from_bytes(text)),
        _ => Value::undefined(),
    }
}

fn initial_url_value(ctx: Ctx, url: Option<&[u8]>) -> Value {
    ctx.enter(|scope| scope.string_from_bytes(url.unwrap_or(b"about:blank")))
}

pub(crate) fn make_scope(
    ctx: Ctx,
    document: &Value,
    initial_url: Option<&[u8]>,
    doc_url: Option<&[u8]>,
    sandbox: u32,
) -> Value {
    let global = ctx.global();
    let Some(maker) = ctx.eval_hidden(bootstrap::SCOPE, c"<iframe-scope>") else {
        return Value::null();
    };
    if !ctx.is_function(&maker) {
        return Value::null();
    }
    let args = [
        global,
        document.clone(),
        initial_url_value(ctx, initial_url),
        Value::int(sandbox as i32),
        text_or_undefined(ctx, doc_url),
    ];
    ctx.call(&maker, &Value::undefined(), &args)
        .unwrap_or_else(Value::null)
}

pub(crate) fn cross_origin_window(ctx: Ctx, target: Value) -> Value {
    let global = ctx.global();
    let Some(maker) = ctx.eval_hidden(bootstrap::CROSS_WINDOW, c"<cross-window>") else {
        return Value::null();
    };
    if !ctx.is_function(&maker) {
        return Value::null();
    }
    ctx.call(&maker, &Value::undefined(), &[global, target])
        .unwrap_or_else(Value::null)
}

fn parent_realm(js: Js, iframe: Option<Node<'_>>) -> Ctx {
    iframe
        .and_then(|iframe| iframe.parent())
        .and_then(|parent| node_realm(js, parent))
        .unwrap_or_else(|| ffi::main_realm(js))
}

pub(crate) fn node_frame(node: Node<'_>) -> Option<Node<'_>> {
    southstar_dom::ancestors_and_self(node).find(|n| {
        matches!(
            n.element_name(),
            Some(b"iframe") | Some(b"frame") | Some(b"object")
        )
    })
}

pub(crate) fn node_realm(js: Js, node: Node<'_>) -> Option<Ctx> {
    let frame = node_frame(node)?;
    page::frame_entry(js, frame_of(frame), |entry| entry.realm).flatten()
}

struct FrameUrls<'a> {
    initial_url: Option<&'a [u8]>,
    doc_url: Option<&'a [u8]>,
    sandbox: u32,
}

fn run_global_bootstrap(
    js: Js,
    fctx: Ctx,
    iframe: Option<Node<'_>>,
    document: &Value,
    window: &Value,
    parent_global: &Value,
    urls: FrameUrls<'_>,
) -> Option<(Value, Value)> {
    let FrameUrls {
        initial_url,
        doc_url,
        sandbox,
    } = urls;
    let maker = fctx.eval_hidden(bootstrap::GLOBAL, c"<iframe-global>")?;
    if !fctx.is_function(&maker) {
        return None;
    }
    let url = initial_url_value(fctx, initial_url);
    let platform = ffi::platform_names(js, fctx);
    let frame_element = match iframe {
        Some(iframe) => ffi::make_element(ffi::current_realm(js), iframe),
        None => Value::null(),
    };
    let frame_name = iframe
        .and_then(|iframe| iframe.attr(c"name"))
        .map_or(&b""[..], |name| name.to_bytes());
    let frame_name = fctx.enter(|scope| scope.string_from_bytes(frame_name));
    let child_frame_of = ffi::child_frame_of_function(fctx);
    let post_message = ffi::make_post_message(fctx, window);
    let doc = text_or_undefined(fctx, doc_url);
    let realm_clone = ffi::realm_clone_function(fctx);
    let window_events = ffi::frame_window_events(fctx);
    let args = [
        window.clone(),
        parent_global.clone(),
        document.clone(),
        url,
        Value::int(sandbox as i32),
        platform,
        frame_element,
        frame_name,
        child_frame_of,
        post_message,
        doc,
        realm_clone,
        window_events,
    ];
    let result = fctx.call(&maker, &Value::undefined(), &args)?;
    if !result.is_object() {
        return None;
    }
    Some((fctx.get(&result, "location"), fctx.get(&result, "history")))
}

pub(crate) fn make_realm_context(
    js: Js,
    iframe: Option<Node<'_>>,
    document: &Value,
    initial_url: Option<&[u8]>,
    doc_url: Option<&[u8]>,
    sandbox: u32,
    reuse: Option<Ctx>,
) -> Option<FrameRealm> {
    let initial_url = initial_url.map(<[u8]>::to_vec);
    let doc_url = doc_url.map(<[u8]>::to_vec);
    let fctx = match reuse {
        Some(fctx) => fctx,
        None => {
            let fctx = ffi::new_frame_context(js)?;
            if let Some(iframe) = iframe {
                page::update_frame(js, frame_of(iframe), |entry| entry.realm = Some(fctx));
                ffi::move_timeline(js, iframe, fctx);
            }
            fctx
        }
    };
    ffi::adopt_frame_clock(js, iframe, fctx);
    cloner::prepare_frame_realm(js, fctx, document);
    let window = fctx.global();
    let parent_global = parent_realm(js, iframe).global();
    let (location, history) = run_global_bootstrap(
        js,
        fctx,
        iframe,
        document,
        &window,
        &parent_global,
        FrameUrls {
            initial_url: initial_url.as_deref(),
            doc_url: doc_url.as_deref(),
            sandbox,
        },
    )?;
    cloner::install_singletons(js, fctx, &parent_global, &window);
    ffi::finish_frame_realm(js, fctx, &window, &parent_global);
    Some(FrameRealm {
        ctx: fctx,
        window,
        location,
        history,
    })
}

pub(crate) fn store_window(js: Js, iframe: Node<'_>, window: &Value) {
    if js.is_null() || !window.is_object() {
        return;
    }
    let old = page::update_frame(js, frame_of(iframe), |entry| {
        entry.window.replace(window.clone())
    });
    drop(old);
}

pub(crate) fn stored_window(js: Js, iframe: Node<'_>) -> Option<Value> {
    if js.is_null() {
        return None;
    }
    page::frame_entry(js, frame_of(iframe), |entry| entry.window.clone()).flatten()
}

fn frame_doc_url(iframe: Node<'_>) -> Option<Vec<u8>> {
    iframe
        .attr(c"data-nd-frame-doc-url")
        .map(|url| url.to_bytes().to_vec())
        .filter(|url| !url.is_empty())
}

fn build_lite_window(ctx: Ctx, element: &Value, iframe: Node<'_>) -> Value {
    let js = Js::of_ctx(ctx);
    if js.is_null() {
        return ctx.global();
    }
    let document = ffi::content_document(ctx, element, iframe);
    if !document.is_object() {
        return ctx.global();
    }
    let mut url = iframe
        .attr(c"data-nd-frame-url")
        .map(|url| url.to_bytes().to_vec())
        .filter(|url| !url.is_empty());
    let mut shown = frame_doc_url(iframe);
    if url.is_none() {
        url = ffi::node_doc_base(js, iframe);
        shown = Some(b"about:blank".to_vec());
    }
    let url = url.filter(|url| !url.is_empty());
    let Some(realm) = make_realm_context(
        js,
        Some(iframe),
        &document,
        Some(url.as_deref().unwrap_or(b"about:blank")),
        shown.as_deref(),
        sandbox::effective(iframe),
        None,
    ) else {
        return ctx.global();
    };
    if !realm.window.is_object() {
        return ctx.global();
    }
    ctx.set(&document, "defaultView", realm.window.clone());
    store_window(js, iframe, &realm.window);
    ctx.set(element, "__ndRealmWindow", realm.window.clone());
    ctx.set(element, "__ndRealmDoc", document);
    page::with(js, |page| {
        page.initial_blank.insert(frame_of(iframe), realm.ctx)
    });
    realm.window
}

pub(crate) fn realm_window(ctx: Ctx, element: &Value, iframe: Node<'_>) -> Value {
    let realm = ctx.get(element, "__ndRealmWindow");
    if realm.is_object() {
        return realm;
    }
    let js = Js::of_ctx(ctx);
    if let Some(stored) = stored_window(js, iframe) {
        return stored;
    }
    build_lite_window(ctx, element, iframe)
}

const SWAPPED_GLOBALS: [&str; 8] = [
    "window", "self", "top", "parent", "frames", "document", "location", "history",
];

const ZONE_BRIDGE: &str = "Object.defineProperty(globalThis,'Zone',{configurable:true,get:function(){var d=Object.getOwnPropertyDescriptor(window,'Zone');return d&&('value' in d)?d.value:void 0;},set:function(v){Object.defineProperty(window,'Zone',{configurable:true,writable:true,value:v});}})";

pub(crate) fn run_modules(
    js: Js,
    modules: &[Node<'_>],
    origin: Option<&[u8]>,
    document: &Value,
    iframe_scope: &Value,
    sandbox: u32,
) {
    if js.is_null() || modules.is_empty() {
        return;
    }
    let ctx = ffi::current_realm(js);
    let scope = if iframe_scope.is_object() {
        iframe_scope.clone()
    } else {
        make_scope(ctx, document, origin, None, sandbox)
    };
    if !scope.is_object() {
        for module in modules {
            ffi::run_script_element(js, *module, origin);
        }
        return;
    }
    let global = ctx.global();
    let mut window = ctx.get(&scope, "window");
    let mut location = ctx.get(&scope, "location");
    let mut history = ctx.get(&scope, "history");
    if !window.is_object() {
        window = global.clone();
    }
    if !location.is_object() {
        location = ctx.get(&global, "location");
    }
    if !history.is_object() {
        history = ctx.get(&global, "history");
    }
    let values = [
        window.clone(),
        window.clone(),
        global.clone(),
        global.clone(),
        window.clone(),
        document.clone(),
        location,
        history,
    ];
    let flags = PROP_WRITABLE | PROP_CONFIGURABLE;
    let mut saved = Vec::with_capacity(SWAPPED_GLOBALS.len());
    for (name, value) in SWAPPED_GLOBALS.iter().zip(values) {
        saved.push(ctx.get(&global, name));
        ctx.define_value_str(&global, name, value, flags);
    }
    let old_zone = ctx.get(&global, "Zone");
    drop(ctx.eval_hidden(ZONE_BRIDGE, c"<iframe-zone>"));
    for module in modules {
        ffi::run_script_element(js, *module, origin);
    }
    let zone = ctx.get(&window, "Zone");
    let restored_zone = if zone.is_undefined() { old_zone } else { zone };
    ctx.define_value_str(&global, "Zone", restored_zone, flags);
    for (name, value) in SWAPPED_GLOBALS.iter().zip(saved) {
        ctx.define_value_str(&global, name, value, flags);
    }
}
