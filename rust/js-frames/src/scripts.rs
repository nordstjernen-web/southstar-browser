//! Southstar — running a loaded frame document's scripts: in the frame's own realm when it gets one, otherwise in the page realm under the frame's scope, then its module scripts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CStr;

use southstar_csp::Csp;
use southstar_dom::{Kind, MAX_DEPTH, Node};
use southstar_js_engine::{Scope, Value};

use crate::exposed::ExposedNames;
use crate::ffi::ctx::Ctx;
use crate::ffi::load;
use crate::ffi::{self, Js};
use crate::origin::{self, CROSS_ORIGIN};
use crate::page::{self, FramePolicy};
use crate::realm::{self, FrameRealm, frame_of};
use crate::restore;

const ALREADY_STARTED: &CStr = c"data-nd-script-already-started";
const EXPOSE_SCAN_CAP: usize = 131072;
const WRAPPER_HEAD: &str =
    "(function(window,self,globalThis,top,parent,document,location,history){\nwith(window){\n";

enum ScriptPolicy {
    Page,
    Own(Option<Csp>),
}

impl ScriptPolicy {
    fn allows_url(&self, js: Js, url: &CStr, document: &CStr, nonce: Option<&[u8]>) -> bool {
        match self {
            ScriptPolicy::Page => load::page_csp_allows_script(js, url, document, nonce),
            ScriptPolicy::Own(csp) => csp.as_ref().is_none_or(|csp| {
                csp.allows(southstar_csp::SCRIPT, url, Some(document), nonce, true)
            }),
        }
    }

    fn allows_inline(&self, js: Js, text: &[u8], nonce: Option<&[u8]>) -> bool {
        match self {
            ScriptPolicy::Page => load::page_csp_allows_inline_script(js, text, nonce),
            ScriptPolicy::Own(csp) => csp
                .as_ref()
                .is_none_or(|csp| csp.inline_script_allowed(text, nonce)),
        }
    }
}

fn embedding_frame(iframe: Node<'_>) -> Option<Node<'_>> {
    let mut node = iframe.parent();
    while let Some(holder) = node {
        if holder.kind() == Kind::Document {
            return holder.parent();
        }
        node = holder.parent();
    }
    None
}

fn script_policy(js: Js, iframe: Option<Node<'_>>) -> ScriptPolicy {
    let mut frame = iframe;
    for _ in 0..MAX_DEPTH {
        let Some(node) = frame else {
            return ScriptPolicy::Page;
        };
        let own = page::frame_entry(js, frame_of(node), |entry| match &entry.policy {
            FramePolicy::Own(header) => Some(header.clone()),
            FramePolicy::Inherit => None,
        })
        .flatten();
        if let Some(header) = own {
            return ScriptPolicy::Own(header.as_deref().and_then(Csp::parse));
        }
        frame = embedding_frame(node);
    }
    ScriptPolicy::Page
}

fn log_blocked(js: Js, what: &[u8], url: &[u8], reason: &[u8]) {
    if load::log_enabled(js) {
        load::log_line(
            js,
            &[b"Blocked ", what, b" ", url, b" in a frame by ", reason].concat(),
        );
    }
}

struct ClassicSource {
    text: Vec<u8>,
    url: Vec<u8>,
    script: Node<'static>,
}

struct Collected {
    classic: Vec<ClassicSource>,
    modules: Vec<Node<'static>>,
    concat: Vec<u8>,
    exposed: ExposedNames,
}

fn add_classic(
    sources: &mut Vec<ClassicSource>,
    text: &[u8],
    url: Option<&[u8]>,
    script: Node<'static>,
) {
    if text.is_empty() {
        return;
    }
    let url = url.filter(|url| !url.is_empty()).unwrap_or(b"inline");
    sources.push(ClassicSource {
        text: text.to_vec(),
        url: url.to_vec(),
        script,
    });
}

fn fetch_external(
    js: Js,
    script: Node<'static>,
    src: &CStr,
    origin: &CStr,
    policy: &ScriptPolicy,
    out: &mut Collected,
) {
    let Some(abs_url) = ffi::resolve_url(origin, src) else {
        return;
    };
    let nonce = script.attr(c"nonce").map(CStr::to_bytes);
    if !policy.allows_url(js, &abs_url, origin, nonce) {
        log_blocked(
            js,
            b"script",
            abs_url.to_bytes(),
            b"Content-Security-Policy script-src",
        );
        return;
    }
    let Some(resp) = load::fetch_script(js, script, &abs_url, origin) else {
        return;
    };
    let nosniff_blocked = load::header_is_nosniff(resp.x_content_type_options())
        && !load::content_type_is_javascript(resp.content_type());
    let ok_status = resp.status() == 200 || resp.status() == 0;
    let Some(body) = resp.body().filter(|body| !body.is_empty()) else {
        return;
    };
    if nosniff_blocked || resp.error().is_some() || !ok_status {
        return;
    }
    let integrity = script.attr(c"integrity").map_or(&b""[..], CStr::to_bytes);
    if !southstar_sandbox::sri_check(integrity, Some(body)) {
        log_blocked(js, b"script", abs_url.to_bytes(), b"Subresource Integrity");
        return;
    }
    if body.len() <= EXPOSE_SCAN_CAP {
        out.exposed.scan(body);
    }
    add_classic(&mut out.classic, body, Some(abs_url.to_bytes()), script);
    out.concat.extend_from_slice(body);
    out.concat.extend_from_slice(b"\n;\n");
}

fn collect_inline(
    js: Js,
    script: Node<'static>,
    origin: &CStr,
    policy: &ScriptPolicy,
    out: &mut Collected,
) {
    let mut source = Vec::new();
    for child in southstar_dom::children(script) {
        if child.kind() != Kind::Text {
            continue;
        }
        if let Some(text) = child.text() {
            source.extend_from_slice(text.to_bytes());
        }
    }
    let nonce = script.attr(c"nonce").map(CStr::to_bytes);
    if !policy.allows_inline(js, &source, nonce) {
        log_blocked(
            js,
            b"inline script",
            origin.to_bytes(),
            b"Content-Security-Policy script-src",
        );
        return;
    }
    out.exposed.scan(&source);
    out.concat.extend_from_slice(&source);
    add_classic(&mut out.classic, &source, Some(origin.to_bytes()), script);
    out.concat.extend_from_slice(b"\n;\n");
}

fn collect(js: Js, content_root: Node<'_>, origin: &CStr, iframe: Option<Node<'_>>) -> Collected {
    let policy = script_policy(js, iframe);
    let mut out = Collected {
        classic: Vec::new(),
        modules: Vec::new(),
        concat: Vec::new(),
        exposed: ExposedNames::default(),
    };
    for script in load::script_tasks(content_root) {
        if script.attr(ALREADY_STARTED).is_some() {
            continue;
        }
        if !load::script_type_supported(script) || load::script_skipped_by_nomodule(script) {
            ffi::set_attr(script, ALREADY_STARTED, Some(c"1"));
            continue;
        }
        if load::script_type_is_module(script) {
            out.modules.push(script);
            continue;
        }
        ffi::set_attr(script, ALREADY_STARTED, Some(c"1"));
        match script.attr(c"src").filter(|src| !src.is_empty()) {
            Some(src) => {
                let src = src.to_owned();
                fetch_external(js, script, &src, origin, &policy, &mut out);
            }
            None => collect_inline(js, script, origin, &policy, &mut out),
        }
    }
    out
}

fn has_inline_handlers(node: Node<'_>, depth: i32) -> bool {
    if depth >= MAX_DEPTH {
        return false;
    }
    if node.kind() == Kind::Element
        && node.attrs().any(|attr| {
            let named_on = attr.name().is_some_and(|name| {
                name.to_bytes().len() >= 2 && name.to_bytes()[..2].eq_ignore_ascii_case(b"on")
            });
            named_on && attr.value().is_some_and(|value| !value.is_empty())
        })
    {
        return true;
    }
    southstar_dom::children(node).any(|child| has_inline_handlers(child, depth + 1))
}

pub(crate) fn log_exception(js: Js, scope: &mut Scope<'_>, exception: &Value, origin: &[u8]) {
    let Ok(message) = scope.to_string(exception) else {
        return;
    };
    if !load::log_enabled(js) {
        return;
    }
    let stack = scope
        .get(exception, "stack")
        .ok()
        .and_then(|stack| scope.to_string(&stack).ok());
    let mut line = b"JS error in ".to_vec();
    line.extend_from_slice(origin);
    line.extend_from_slice(b": ");
    line.extend_from_slice(message.as_bytes());
    if let Some(stack) = stack {
        line.push(b'\n');
        line.extend_from_slice(stack.as_bytes());
    }
    load::log_line(js, &line);
}

fn outward_window(js: Js, iframe: Node<'_>) -> Value {
    realm::stored_window(js, iframe)
        .filter(Value::is_object)
        .unwrap_or_else(Value::null)
}

fn publish_realm_window(
    js: Js,
    iframe: Option<Node<'_>>,
    iframe_doc: &Value,
    window: &Value,
    sandbox: u32,
) {
    let ctx = ffi::current_realm(js);
    let outward = match iframe {
        Some(iframe) if sandbox & CROSS_ORIGIN != 0 => outward_window(js, iframe),
        _ => Value::null(),
    };
    let preserve_outward = outward.is_object() && !outward.same_object(window);
    if preserve_outward {
        load::link_outward(js, &outward, window);
    }
    if iframe_doc.is_object() {
        ctx.set(iframe_doc, "defaultView", window.clone());
    }
    let shown = if preserve_outward { &outward } else { window };
    let Some(iframe) = iframe else {
        return;
    };
    realm::store_window(js, iframe, shown);
}

fn run_in_realm(js: Js, made: &FrameRealm, iframe: Option<Node<'_>>, sources: &[ClassicSource]) {
    let deadline_us = load::monotonic_time() + load::eval_budget_us();
    let run = || {
        for source in sources {
            if load::monotonic_time() >= deadline_us {
                break;
            }
            let text = String::from_utf8_lossy(&source.text);
            let name = String::from_utf8_lossy(&source.url);
            load::evaluating(js, Some(source.script), deadline_us, || {
                made.ctx.enter(|scope| {
                    if let Err(exception) = scope.eval_script(&text, &name) {
                        log_exception(js, scope, &exception, &source.url);
                    }
                })
            });
        }
    };
    match iframe {
        Some(iframe) => load::in_frame_scope(js, made.ctx, iframe, run),
        None => run(),
    }
}

struct PageRealmRun<'a> {
    iframe_doc: &'a Value,
    iframe_scope: &'a Value,
    origin: &'a CStr,
    doc_url: Option<&'a CStr>,
    sandbox: u32,
}

fn wrapper_scope(ctx: Ctx, run: &PageRealmRun<'_>) -> (Value, Value, Value) {
    let global = ctx.global();
    let scope = if run.iframe_scope.is_object() {
        run.iframe_scope.clone()
    } else {
        realm::make_scope(
            ctx,
            run.iframe_doc,
            Some(run.origin.to_bytes()),
            run.doc_url.map(CStr::to_bytes),
            run.sandbox,
        )
    };
    let field = |name: &str| {
        if scope.is_object() {
            ctx.get(&scope, name)
        } else {
            Value::null()
        }
    };
    let mut window = field("window");
    let mut location = field("location");
    let mut history = field("history");
    if !window.is_object() {
        window = global.clone();
    }
    if !location.is_object() {
        location = ctx.get(&global, "location");
    }
    if !history.is_object() {
        history = ctx.get(&global, "history");
    }
    (window, location, history)
}

fn run_in_page_realm(js: Js, concat: &[u8], exposed: &ExposedNames, run: &PageRealmRun<'_>) {
    let mut source = String::from(WRAPPER_HEAD);
    source.push_str(&String::from_utf8_lossy(concat));
    source.push_str("\n}\n");
    if let Some(exposing) = exposed.exposing_script() {
        source.push_str(&exposing);
    }
    source.push_str("\n})");
    let ctx = ffi::current_realm(js);
    let origin = run.origin.to_bytes();
    let name = String::from_utf8_lossy(origin);
    let Ok(function) = ctx.enter(|scope| scope.eval_script(&source, &name)) else {
        return;
    };
    if !ctx.is_function(&function) {
        return;
    }
    let global = ctx.global();
    let (window, location, history) = wrapper_scope(ctx, run);
    let args = [
        window.clone(),
        window.clone(),
        global.clone(),
        global,
        window.clone(),
        run.iframe_doc.clone(),
        location,
        history,
    ];
    let deadline_us = load::monotonic_time() + load::eval_budget_us();
    load::evaluating(js, None, deadline_us, || {
        ctx.enter(|scope| {
            if let Err(exception) = scope.call(&function, &window, &args) {
                log_exception(js, scope, &exception, origin);
            }
        })
    });
}

fn module_scope(ctx: Ctx, made: &FrameRealm) -> Value {
    let scope = ctx.new_object();
    ctx.set(&scope, "window", made.window.clone());
    ctx.set(&scope, "location", made.location.clone());
    ctx.set(&scope, "history", made.history.clone());
    scope
}

pub(crate) fn run(
    js: Js,
    content_root: Node<'_>,
    origin: &CStr,
    iframe_doc: &Value,
    iframe_scope: &Value,
    sandbox: u32,
    iframe: Option<Node<'_>>,
) {
    let proto_snapshot = restore::proto_snapshot(ffi::current_realm(js));
    restore::clear_global_zone(ffi::current_realm(js));
    restore::restore_globals(ffi::current_realm(js));
    let collected = collect(js, content_root, origin, iframe);

    let has_handlers = has_inline_handlers(content_root, 0);
    let initial_blank = iframe.and_then(|iframe| {
        page::peek_mut(js, |page| page.initial_blank.remove(&frame_of(iframe))).flatten()
    });
    let reuse_blank = initial_blank.filter(|blank| {
        sandbox & CROSS_ORIGIN == 0
            && iframe.is_some_and(|iframe| {
                page::frame_entry(js, frame_of(iframe), |entry| entry.realm).flatten()
                    == Some(*blank)
            })
    });
    let doc_url = origin::doc_url(iframe).map(CStr::to_owned);
    let made = if reuse_blank.is_some()
        || !collected.classic.is_empty()
        || !collected.modules.is_empty()
        || has_handlers
    {
        realm::make_realm_context(
            js,
            iframe,
            iframe_doc,
            Some(origin.to_bytes()),
            doc_url.as_deref().map(CStr::to_bytes),
            sandbox,
            reuse_blank,
        )
    } else {
        None
    };
    let realm_window = made.as_ref().filter(|made| made.window.is_object());

    match realm_window {
        Some(made) if !collected.classic.is_empty() || has_handlers || reuse_blank.is_some() => {
            publish_realm_window(js, iframe, iframe_doc, &made.window, sandbox);
            run_in_realm(js, made, iframe, &collected.classic);
        }
        _ if !collected.concat.is_empty() => run_in_page_realm(
            js,
            &collected.concat,
            &collected.exposed,
            &PageRealmRun {
                iframe_doc,
                iframe_scope,
                origin,
                doc_url: doc_url.as_deref(),
                sandbox,
            },
        ),
        _ => {}
    }

    let origin_bytes = Some(origin.to_bytes());
    match realm_window {
        Some(made) if !collected.modules.is_empty() => {
            let scope = module_scope(ffi::current_realm(js), made);
            realm::run_modules(
                js,
                &collected.modules,
                origin_bytes,
                iframe_doc,
                &scope,
                sandbox,
            );
        }
        _ => realm::run_modules(
            js,
            &collected.modules,
            origin_bytes,
            iframe_doc,
            iframe_scope,
            sandbox,
        ),
    }
    restore::proto_cleanup(ffi::current_realm(js), &proto_snapshot);
    restore::restore_globals(ffi::current_realm(js));
}
