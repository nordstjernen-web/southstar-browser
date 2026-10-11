//! Southstar — loading a frame's document: fetching its source under the page's CSP and framing rules, building the document the response describes, and running it with its scripts and load events.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::{CStr, CString};

use southstar_dom::{Kind, Node};
use southstar_js_engine::Value;

use crate::ffi::load::{self, FrameKind, Response};
use crate::ffi::{self, Js};
use crate::globals;
use crate::origin::{self, opaque_by_sandbox};
use crate::owner;
use crate::sandbox;
use crate::schedule;
use crate::scripts;

const MAX_FRAME_DEPTH: usize = 20;
const MAX_SAME_URL_ANCESTORS: usize = 2;
const XHTML_NAMESPACE: &[u8] = b"http://www.w3.org/1999/xhtml";
const PARSE_ERROR_DOCUMENT: &CStr = c"<html><head><title>Parse Error</title></head><body><p>This XML document is not well-formed.</p></body></html>";

fn non_empty(text: Option<&CStr>) -> Option<&CStr> {
    text.filter(|text| !text.is_empty())
}

fn contains(haystack: &CStr, needle: &[u8]) -> bool {
    haystack
        .to_bytes()
        .windows(needle.len())
        .any(|window| window == needle)
}

fn concat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn glib_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn glib_utf8_valid(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && core::str::from_utf8(bytes).is_ok()
}

fn is_frame_owner(node: Node<'_>) -> bool {
    matches!(
        node.element_name(),
        Some(b"iframe") | Some(b"frame") | Some(b"object")
    )
}

fn framing_blocked(embedder: &CStr, framed: &CStr, resp: &Response) -> bool {
    if let Some(options) = non_empty(resp.xframe_options()) {
        let mut value = options.to_bytes();
        while let [first, rest @ ..] = value {
            if !glib_space(*first) {
                break;
            }
            value = rest;
        }
        while let [rest @ .., last] = value {
            if !glib_space(*last) {
                break;
            }
            value = rest;
        }
        if value.eq_ignore_ascii_case(b"deny") {
            return true;
        }
        if value.eq_ignore_ascii_case(b"sameorigin") && !load::same_origin(framed, embedder) {
            return true;
        }
    }
    if let Some(header) = non_empty(resp.csp_header())
        && !load::csp_header_allows_framing(header, embedder, framed)
    {
        return true;
    }
    false
}

fn kind_name(kind: FrameKind) -> (&'static [u8], &'static [u8]) {
    match kind {
        FrameKind::Object => (b"object", b"object-src"),
        FrameKind::Frame => (b"iframe", b"frame-src"),
    }
}

fn csp_blocks(js: Js, kind: FrameKind, url: &CStr, origin: &CStr, redirected: bool) -> bool {
    if load::page_csp_allows(js, kind, url, origin) {
        return false;
    }
    if load::log_enabled(js) {
        let (element, directive) = kind_name(kind);
        let verb: &[u8] = if redirected { b" redirected to " } else { b" " };
        load::log_line(
            js,
            &concat(&[
                b"Blocked ",
                element,
                verb,
                url.to_bytes(),
                b" by Content-Security-Policy ",
                directive,
            ]),
        );
    }
    true
}

fn nesting_blocks(js: Js, iframe: Node<'_>, url: &CStr) -> bool {
    let mut depth = 0usize;
    let mut same_url = 0usize;
    let mut cursor = iframe.parent();
    while let Some(node) = cursor {
        cursor = node.parent();
        if node.kind() != Kind::Element || !is_frame_owner(node) {
            continue;
        }
        depth += 1;
        if node.attr(c"data-nd-frame-url") == Some(url) {
            same_url += 1;
        }
    }
    if depth < MAX_FRAME_DEPTH && same_url < MAX_SAME_URL_ANCESTORS {
        return false;
    }
    if load::log_enabled(js) {
        let line = format!(" (depth={depth} same-url={same_url})");
        load::log_line(
            js,
            &concat(&[
                b"Blocked recursive/over-nested iframe ",
                url.to_bytes(),
                line.as_bytes(),
            ]),
        );
    }
    true
}

fn fetch_dest(iframe: Node<'_>, kind: FrameKind) -> &'static CStr {
    match kind {
        FrameKind::Object => c"X-ND-Fetch-Dest: object",
        FrameKind::Frame
            if iframe
                .name()
                .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"frame")) =>
        {
            c"X-ND-Fetch-Dest: frame"
        }
        FrameKind::Frame => c"X-ND-Fetch-Dest: iframe",
    }
}

struct Fetched {
    abs_url: Option<CString>,
    decoded: Option<CString>,
    resp: Option<Response>,
}

fn decode_response(iframe: Node<'_>, abs_url: &CStr, resp: &Response) -> Option<CString> {
    let body = resp.body()?;
    let content_type = resp.content_type();
    if !body.is_empty()
        && content_type.is_some_and(|ct| {
            ct.to_bytes().len() >= 6
                && ct.to_bytes()[..6].eq_ignore_ascii_case(b"image/")
                && !contains(ct, b"xml")
        })
    {
        let shown = resp.final_url().unwrap_or(abs_url);
        let decoded = load::image_document(shown);
        ffi::set_attr(iframe, c"data-nd-frame-charset", Some(c"UTF-8"));
        return decoded;
    }
    if !body.is_empty() {
        let decoded = load::decode_body(body, content_type);
        let declared = load::declared_charset(body, content_type);
        ffi::set_attr(
            iframe,
            c"data-nd-frame-charset",
            Some(declared.as_deref().unwrap_or(c"UTF-8")),
        );
        return decoded;
    }
    if content_type.is_some_and(|ct| contains(ct, b"xml")) {
        ffi::set_attr(iframe, c"data-nd-frame-charset", Some(c"UTF-8"));
        return Some(CString::default());
    }
    None
}

fn fetch_source(js: Js, iframe: Node<'_>, src: &CStr, origin: &CStr) -> Fetched {
    let mut fetched = Fetched {
        abs_url: ffi::resolve_url(origin, src),
        decoded: None,
        resp: None,
    };
    let kind = if iframe.element_name() == Some(b"object") {
        FrameKind::Object
    } else {
        FrameKind::Frame
    };
    if let Some(url) = &fetched.abs_url
        && csp_blocks(js, kind, url, origin, false)
    {
        fetched.abs_url = None;
    }
    if let Some(url) = &fetched.abs_url
        && nesting_blocks(js, iframe, url)
    {
        fetched.abs_url = None;
    }
    let Some(mut abs_url) = fetched.abs_url.take() else {
        return fetched;
    };
    let start_us = load::monotonic_time();
    let (mut resp, error) = load::fetch_resource(js, &abs_url, origin, fetch_dest(iframe, kind));
    let download = resp.as_ref().is_some_and(|resp| {
        schedule::response_is_download(
            resp.error(),
            resp.content_disposition(),
            resp.content_type(),
        )
    });
    if !download {
        let info = load::element_perf_info(js, iframe);
        let initiator: &CStr = match kind {
            FrameKind::Object => c"object",
            FrameKind::Frame => c"iframe",
        };
        load::add_resource_timing(
            js,
            &info,
            &abs_url,
            initiator,
            (start_us, load::monotonic_time()),
            resp.as_ref(),
        );
    }
    let redirected = resp
        .as_ref()
        .and_then(|resp| non_empty(resp.final_url()))
        .filter(|final_url| *final_url != abs_url.as_c_str())
        .map(CStr::to_owned);
    if let Some(final_url) = redirected {
        abs_url = final_url;
        if csp_blocks(js, kind, &abs_url, origin, true) {
            resp = None;
        }
    }
    match &resp {
        Some(response) if framing_blocked(origin, &abs_url, response) => {
            if load::log_enabled(js) {
                load::log_line(
                    js,
                    &concat(&[
                        b"Blocked framing of ",
                        abs_url.to_bytes(),
                        b" (X-Frame-Options / frame-ancestors)",
                    ]),
                );
            }
        }
        Some(response)
            if response.body().is_some()
                && (response.status() == 200 || response.status() == 0)
                && response.error().is_none() =>
        {
            fetched.decoded = decode_response(iframe, &abs_url, response);
        }
        _ if load::log_enabled(js) => {
            let reason = error
                .as_deref()
                .or_else(|| resp.as_ref().and_then(Response::error))
                .map_or(&b"load failed"[..], CStr::to_bytes);
            load::log_line(
                js,
                &concat(&[b"iframe ", abs_url.to_bytes(), b": ", reason]),
            );
        }
        _ => {}
    }
    fetched.abs_url = Some(abs_url);
    fetched.resp = resp;
    fetched
}

pub(crate) fn clear_content(js: Js, iframe: Node<'_>) {
    load::purge_frame_callbacks(js, iframe);
    globals::scrub(js, iframe);
    let mut child = iframe.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        if node.kind() == Kind::Document {
            load::orphan_document(js, node);
        }
    }
}

struct XmlState {
    suppress_scripts: bool,
    malformed: bool,
    empty: bool,
}

fn xml_state(resp: Option<&Response>, decoded: Option<&CStr>, is_xml_content: bool) -> XmlState {
    let mut state = XmlState {
        suppress_scripts: false,
        malformed: false,
        empty: false,
    };
    if !is_xml_content {
        return state;
    }
    let Some(decoded) = decoded else {
        return state;
    };
    if let Some(resp) = resp
        && let Some(body) = resp.body()
    {
        let charset = load::declared_charset(body, resp.content_type());
        let utf8_declared = charset.as_deref().is_none_or(|charset| {
            let charset = charset.to_bytes();
            charset.eq_ignore_ascii_case(b"UTF-8") || charset.eq_ignore_ascii_case(b"UTF8")
        });
        if utf8_declared && !glib_utf8_valid(body) {
            state.malformed = true;
            state.suppress_scripts = true;
        }
    }
    if state.malformed {
        return state;
    }
    if decoded.to_bytes().iter().all(|&byte| glib_space(byte)) {
        state.empty = true;
        state.suppress_scripts = true;
        return state;
    }
    let (well_formed, root_ns) = load::xml_well_formed(decoded);
    if !well_formed {
        state.malformed = true;
        state.suppress_scripts = true;
    } else if root_ns.as_deref().map(CStr::to_bytes) != Some(XHTML_NAMESPACE) {
        state.suppress_scripts = true;
    }
    state
}

struct Built<'a> {
    root: Node<'a>,
    doc: Node<'a>,
    is_xml: bool,
}

fn attach(iframe: Node<'_>, doc: Node<'_>) {
    load::own_strings_deep(doc);
    ffi::append_child(iframe, doc);
}

fn plaintext_document(iframe: Node<'_>, text: &CStr) -> Built<'static> {
    let doc = ffi::new_document();
    let html = ffi::new_element(c"html");
    let head = ffi::new_element(c"head");
    let body = ffi::new_element(c"body");
    let pre = ffi::new_element(c"pre");
    ffi::append_child(pre, load::new_text(text));
    ffi::append_child(body, pre);
    ffi::append_child(html, head);
    ffi::append_child(html, body);
    ffi::append_child(doc, html);
    attach(iframe, doc);
    Built {
        root: html,
        doc,
        is_xml: false,
    }
}

fn xml_document(iframe: Node<'_>, decoded: &CStr) -> Option<Built<'static>> {
    let doc = load::xml_parse(decoded)?;
    let Some(root) = southstar_dom::children(doc).find(|child| child.kind() == Kind::Element)
    else {
        load::free_node(doc);
        return None;
    };
    attach(iframe, doc);
    Some(Built {
        root,
        doc,
        is_xml: true,
    })
}

fn html_document(iframe: Node<'_>, decoded: &CStr) -> Option<Built<'static>> {
    let doc = load::html_parse(decoded)?;
    let Some(root) = southstar_dom::index::find_first_element(doc, c"html") else {
        load::free_node(doc);
        return None;
    };
    attach(iframe, doc);
    Some(Built {
        root,
        doc,
        is_xml: false,
    })
}

fn build_document(
    iframe: Node<'_>,
    resp: Option<&Response>,
    decoded: Option<&CStr>,
    xml: &XmlState,
    is_xml_content: bool,
) -> Option<Built<'static>> {
    if xml.empty {
        let doc = ffi::new_document();
        ffi::append_child(iframe, doc);
        return Some(Built {
            root: doc,
            doc,
            is_xml: true,
        });
    }
    let decoded = decoded?;
    if is_xml_content
        && !xml.malformed
        && let Some(built) = xml_document(iframe, decoded)
    {
        return Some(built);
    }
    let is_plaintext = resp.and_then(Response::content_type).is_some_and(|ct| {
        ct.to_bytes().len() >= 10 && ct.to_bytes()[..10].eq_ignore_ascii_case(b"text/plain")
    });
    if is_plaintext {
        return Some(plaintext_document(iframe, decoded));
    }
    let source = if xml.malformed {
        PARSE_ERROR_DOCUMENT
    } else {
        decoded
    };
    html_document(iframe, source)
}

struct Shown {
    realm_doc: Value,
    realm_scope: Value,
}

fn make_realm_document(
    js: Js,
    iframe: Node<'_>,
    built: &Built<'_>,
    iorigin: &CStr,
    content_type: Option<&CStr>,
    sandbox: u32,
) -> Shown {
    let ctx = ffi::current_realm(js);
    let charset = non_empty(iframe.attr(c"data-nd-frame-charset"))
        .unwrap_or(c"UTF-8")
        .to_owned();
    let doc_url = origin::doc_url(Some(iframe)).map(CStr::to_owned);
    let realm_doc = ctx
        .enter(|scope| {
            let doc = ffi::make_realm_document(
                scope,
                built.doc,
                Some(iorigin),
                Some(&charset),
                content_type,
                built.is_xml,
            );
            if let Ok(doc) = &doc {
                owner::show_url(scope, doc, doc_url.as_deref());
            }
            doc
        })
        .unwrap_or_else(|_| Value::null());
    if opaque_by_sandbox(sandbox) {
        load::deny_cookie(ctx, &realm_doc);
    }
    let realm_scope = if realm_doc.is_object() {
        crate::realm::make_scope(
            ctx,
            &realm_doc,
            Some(iorigin.to_bytes()),
            doc_url.as_deref().map(CStr::to_bytes),
            sandbox,
        )
    } else {
        Value::null()
    };
    if realm_doc.is_object() && realm_scope.is_object() {
        let window = ctx.get(&realm_scope, "window");
        if window.is_object() {
            ctx.set(&realm_doc, "defaultView", window);
        }
        let location = ctx.get(&realm_scope, "location");
        if location.is_object() {
            ctx.set(&realm_doc, "location", location);
        }
    }
    if realm_doc.is_object() {
        let element = ffi::make_element(ctx, iframe);
        ctx.set(&element, "__ndRealmDoc", realm_doc.clone());
        if realm_scope.is_object() {
            let window = ctx.get(&realm_scope, "window");
            if window.is_object() {
                ctx.set(&element, "__ndRealmWindow", window);
            }
        }
    }
    Shown {
        realm_doc,
        realm_scope,
    }
}

fn run_document(
    js: Js,
    iframe: Node<'_>,
    built: &Built<'_>,
    iorigin: &CStr,
    shown: &Shown,
    run: (u32, bool, bool),
) {
    let (sandbox, scripts_ok, suppress_scripts) = run;
    let before = globals::snapshot(js);
    load::during_frame_load(js, iframe, built.doc, iorigin, &shown.realm_doc, || {
        ffi::set_doc_ready_state(js, built.doc, 0);
        if suppress_scripts {
            load::mark_scripts_already_started(built.root);
        }
        if !scripts_ok {
            load::mark_scripts_already_started(built.root);
            if load::log_enabled(js) {
                load::log_line(
                    js,
                    &concat(&[
                        b"Blocked scripts in sandboxed iframe ",
                        iorigin.to_bytes(),
                        b" (missing allow-scripts)",
                    ]),
                );
            }
        } else if shown.realm_doc.is_object() {
            scripts::run(
                js,
                built.root,
                iorigin,
                &shown.realm_doc,
                &shown.realm_scope,
                sandbox,
                Some(iframe),
            );
        } else {
            load::run_page_realm_scripts(js, built.root, iorigin);
        }
        ffi::set_doc_ready_state(js, built.doc, 1);
        load::dispatch_event(js, built.doc, c"readystatechange");
        load::dispatch_event(js, built.doc, c"DOMContentLoaded");
        ffi::set_doc_ready_state(js, built.doc, 2);
        load::dispatch_event(js, built.doc, c"readystatechange");
        load::dispatch_event(js, built.doc, c"load");
        load::fire_page_transition(js, c"pageshow");
    });
    globals::record(js, iframe, &before);
}

fn finish_written_document(js: Js, iframe: Node<'_>) -> bool {
    if iframe.attr(c"data-nd-doc-written").is_none() {
        return false;
    }
    let src = schedule::src_attr(iframe).and_then(|attr| iframe.attr(attr));
    if non_empty(src).is_some() || non_empty(iframe.attr(c"srcdoc")).is_some() {
        return false;
    }
    ffi::set_attr(iframe, c"data-nd-frame-loaded", Some(c"1"));
    load::mark_attr_dirty(iframe, c"data-nd-frame-loaded");
    load::dispatch_resource_event(js, iframe, c"load");
    true
}

pub(crate) fn load_now(js: Js, iframe: Node<'_>) {
    if js.is_null() || ffi::halted(js) || ffi::current_document(js).is_none() {
        return;
    }
    if !load::node_in_page(js, iframe) {
        return;
    }
    if finish_written_document(js, iframe) {
        return;
    }
    let mut sandbox = sandbox::effective(iframe);
    let scripts_ok = sandbox & sandbox::ACTIVE == 0 || sandbox & sandbox::ALLOW_SCRIPTS != 0;
    let origin = non_empty(load::page_url(js).as_deref())
        .unwrap_or(c"inline")
        .to_owned();
    let src = schedule::src_attr(iframe)
        .and_then(|attr| iframe.attr(attr))
        .map(CStr::to_owned);
    let srcdoc = iframe.attr(c"srcdoc").map(CStr::to_owned);

    load::start_frame_clock(js, iframe);
    let fetched = match (non_empty(srcdoc.as_deref()), non_empty(src.as_deref())) {
        (Some(srcdoc), _) => Fetched {
            abs_url: Some(origin.clone()),
            decoded: Some(srcdoc.to_owned()),
            resp: None,
        },
        (None, Some(src)) if !src.to_bytes().starts_with(b"about:") => {
            fetch_source(js, iframe, src, &origin)
        }
        _ => Fetched {
            abs_url: None,
            decoded: None,
            resp: None,
        },
    };
    let Fetched {
        mut abs_url,
        decoded,
        resp,
    } = fetched;
    let content_type = resp.as_ref().and_then(Response::content_type);

    if iframe.element_name() == Some(b"object") {
        let doc_type = content_type.is_some_and(|ct| contains(ct, b"html") || contains(ct, b"xml"));
        if !doc_type {
            return;
        }
    }

    clear_content(js, iframe);
    load::sweep_orphans(js);

    let is_plain_xml =
        content_type.is_some_and(|ct| contains(ct, b"xml") && !contains(ct, b"html"));
    let is_xhtml = content_type.is_some_and(|ct| contains(ct, b"xhtml"));
    let is_xml_content = is_xhtml || is_plain_xml;
    let xml = xml_state(resp.as_ref(), decoded.as_deref(), is_xml_content);
    let mut built = build_document(
        iframe,
        resp.as_ref(),
        decoded.as_deref(),
        &xml,
        is_xml_content,
    );

    let mut blank_frame = false;
    if built.is_none() && decoded.is_none() && iframe.element_name() == Some(b"iframe") {
        let root = owner::ensure_content_root(iframe);
        built = root.parent().map(|doc| Built {
            root,
            doc,
            is_xml: false,
        });
        if abs_url.is_none() {
            abs_url = Some(origin.clone());
            blank_frame = true;
        }
    }

    if let Some(built) = &built {
        ffi::set_attr(iframe, c"data-nd-frame-loaded", Some(c"1"));
        load::mark_attr_dirty(iframe, c"data-nd-frame-loaded");
        let iorigin = non_empty(abs_url.as_deref()).unwrap_or(&origin).to_owned();
        let marked_url = if blank_frame {
            None
        } else {
            abs_url.as_deref()
        };
        schedule::mark_source(js, iframe, Some(&origin), marked_url);
        sandbox = origin::mark_cross_origin(js, &iorigin, sandbox);
        let shown = make_realm_document(js, iframe, built, &iorigin, content_type, sandbox);
        run_document(
            js,
            iframe,
            built,
            &iorigin,
            &shown,
            (sandbox, scripts_ok, xml.suppress_scripts),
        );
    }

    ffi::mark_mutated(js);
    if let Some(built) = &built {
        schedule::schedule_static(js, built.root, 0);
    }
    load::schedule_pending_script_drain(js);
    load::dispatch_resource_event(js, iframe, c"load");
}
