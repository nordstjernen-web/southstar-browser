//! Southstar — fetching and running one script element, loading one stylesheet link, and fetching a module's source for the module loader.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::Node;
use southstar_js_engine::{Scope, Value};

use crate::eval::ScriptKind;
use crate::ffi::{self, CSP_SCRIPT, CSP_STYLE, Fetched, Js};
use crate::{
    ALREADY_STARTED, EMPTY_SOURCE, FLAG_LINK_LOAD_FIRED, FLAG_NOT_PARSER_INSERTED,
    MAX_SCRIPT_BYTES, content_type_is_javascript, page, skipped_by_nomodule, source_is_empty,
    type_is_module, type_supported,
};

const MODULE_LOAD_MAX_COUNT: u32 = 1024;
const MODULE_LOAD_MAX_BYTES: usize = 128 * 1024 * 1024;

fn shown(s: Option<&CStr>) -> &[u8] {
    s.map_or(b"(null)", CStr::to_bytes)
}

fn line(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

fn failure_reason(fetched: &Fetched, missing: &'static [u8]) -> Vec<u8> {
    if let Some(error) = &fetched.error {
        return error.to_bytes().to_vec();
    }
    match &fetched.response {
        Some(resp) => resp
            .error()
            .map_or_else(|| missing.to_vec(), |error| error.to_bytes().to_vec()),
        None => b"fetch failed".to_vec(),
    }
}

fn starts_with(s: &CStr, prefix: &[u8]) -> bool {
    s.to_bytes().starts_with(prefix)
}

fn run_external(
    js: Js,
    n: Node,
    src: &CStr,
    origin: Option<&CStr>,
    nonce: Option<&CStr>,
    integrity: Option<&CStr>,
    is_module: bool,
) {
    if src.is_empty() {
        js.dispatch_resource_event(n, c"error");
        return;
    }
    if starts_with(src, b"data:") {
        match ffi::decode_data_url(src) {
            Some(body) => {
                js.eval_script_source(n, &body, Some(c"data:"), ScriptKind::of(is_module, false));
                js.dispatch_resource_event(n, c"load");
            }
            None => js.dispatch_resource_event(n, c"error"),
        }
        return;
    }
    if starts_with(src, b"blob:") {
        match js.with_blob(src, <[u8]>::to_vec) {
            Some(body) => {
                js.eval_script_source(n, &body, Some(src), ScriptKind::of(is_module, true));
                js.dispatch_resource_event(n, c"load");
            }
            None => js.dispatch_resource_event(n, c"error"),
        }
        return;
    }
    let base = js.base_url(n);
    let Some(abs_url) = ffi::url_resolve(base.as_deref().or(origin), src) else {
        return;
    };
    if origin.is_some_and(|origin| starts_with(origin, b"https://"))
        && starts_with(&abs_url, b"http://")
    {
        js.log(&line(&[
            b"mixed-content blocked: script ",
            abs_url.to_bytes(),
            b" on https page",
        ]));
        js.dispatch_resource_event(n, c"error");
        return;
    }
    let parser_inserted = n.flags() & FLAG_NOT_PARSER_INSERTED == 0;
    if !js.csp_allows(CSP_SCRIPT, &abs_url, origin, nonce, parser_inserted) {
        js.log(&line(&[b"CSP blocked: script ", abs_url.to_bytes()]));
        js.dispatch_resource_event(n, c"error");
        return;
    }
    let info = js.element_perf_info(n);
    let fetched = js.fetch(&abs_url, origin, true, c"script", &info);
    if let Some(resp) = &fetched.response
        && resp.nosniff()
        && !resp
            .content_type()
            .is_some_and(|ct| content_type_is_javascript(ct.to_bytes()))
    {
        let ct = resp
            .content_type()
            .filter(|ct| !ct.is_empty())
            .map_or(&b"(none)"[..], CStr::to_bytes);
        js.log(&line(&[
            b"nosniff blocked: script ",
            abs_url.to_bytes(),
            b" (Content-Type ",
            ct,
            b")",
        ]));
        drop(fetched);
        js.dispatch_resource_event(n, c"error");
        return;
    }
    let mut loaded = false;
    let body = fetched
        .response
        .as_ref()
        .filter(|resp| resp.status() == 200)
        .and_then(|resp| resp.body())
        .filter(|body| body.len() <= MAX_SCRIPT_BYTES);
    match body {
        Some(body) if !ffi::sri_check(integrity, body) => {
            js.log(&line(&[
                b"SRI mismatch: script ",
                abs_url.to_bytes(),
                b" (integrity=\"",
                shown(integrity),
                b"\")",
            ]));
        }
        Some(body) => {
            if !body.is_empty() {
                js.eval_script_source(n, body, Some(&abs_url), ScriptKind::of(is_module, true));
            }
            loaded = true;
        }
        None => {
            let why = failure_reason(&fetched, b"non-200 status");
            js.log(&line(&[b"script ", abs_url.to_bytes(), b": ", &why]));
        }
    }
    drop(fetched);
    js.dispatch_resource_event(n, if loaded { c"load" } else { c"error" });
}

pub(crate) fn run_script_element(js: Js, n: Node, origin: Option<&CStr>) {
    if n.attr(ALREADY_STARTED).is_some() {
        return;
    }
    let src = n.attr(c"src").map(CStr::to_owned);
    crate::mark(n, ALREADY_STARTED);
    if src.is_none() && source_is_empty(n) {
        n.add_flags(FLAG_NOT_PARSER_INSERTED);
        crate::mark(n, EMPTY_SOURCE);
        return;
    }
    if !type_supported(n) || skipped_by_nomodule(n) {
        return;
    }
    let nonce = n.attr(c"nonce").map(CStr::to_owned);
    let integrity = n.attr(c"integrity").map(CStr::to_owned);
    let is_module = type_is_module(n);
    if let Some(src) = src {
        run_external(
            js,
            n,
            &src,
            origin,
            nonce.as_deref(),
            integrity.as_deref(),
            is_module,
        );
        return;
    }
    for c in southstar_dom::children(n) {
        let (true, Some(text)) = (c.is_text(), c.text()) else {
            continue;
        };
        let text = text.to_bytes();
        if !js.inline_script_allowed(text, nonce.as_deref()) {
            js.log(&line(&[b"CSP blocked: inline <script> on ", shown(origin)]));
            continue;
        }
        js.eval_script_source(n, text, origin, ScriptKind::of(is_module, false));
    }
}

pub(crate) fn load_stylesheet_element(js: Js, n: Node, origin: Option<&CStr>) {
    if n.flags() & FLAG_LINK_LOAD_FIRED != 0 {
        return;
    }
    n.add_flags(FLAG_LINK_LOAD_FIRED);
    let Some(href) = n
        .attr(c"href")
        .filter(|href| !href.is_empty())
        .map(CStr::to_owned)
    else {
        js.dispatch_resource_event(n, c"error");
        return;
    };
    if starts_with(&href, b"data:") {
        js.dispatch_resource_event(n, c"load");
        return;
    }
    let base = js.base_url(n);
    let Some(abs_url) = ffi::url_resolve(base.as_deref().or(origin), &href) else {
        js.dispatch_resource_event(n, c"error");
        return;
    };
    let mut loaded = false;
    if !js.csp_allows(CSP_STYLE, &abs_url, origin, None, true) {
        js.log(&line(&[b"CSP blocked: stylesheet ", abs_url.to_bytes()]));
    } else {
        let info = js.element_perf_info(n);
        let fetched = js.fetch(&abs_url, origin, false, c"link", &info);
        if fetched
            .response
            .as_ref()
            .is_some_and(|resp| resp.status() == 200)
        {
            loaded = true;
        } else {
            let why = failure_reason(&fetched, b"non-200 status");
            js.log(&line(&[b"stylesheet ", abs_url.to_bytes(), b": ", &why]));
        }
    }
    js.dispatch_resource_event(n, if loaded { c"load" } else { c"error" });
}

fn count_load(js: Js, count: bool, bytes: usize) {
    if js.is_null() {
        return;
    }
    page::with_page(js, |page| {
        if count {
            page.module_load_count += 1;
        }
        page.module_load_bytes += bytes;
    });
}

fn over_limit(js: Js, name: &CStr) -> bool {
    if js.is_null() {
        return false;
    }
    let first = page::with_page(js, |page| {
        if page.module_load_count <= MODULE_LOAD_MAX_COUNT
            && page.module_load_bytes <= MODULE_LOAD_MAX_BYTES
        {
            return None;
        }
        Some(!core::mem::replace(&mut page.module_load_capped, true))
    });
    let Some(first) = first else {
        return false;
    };
    if first {
        js.log(&line(&[
            b"module load limit reached, refusing ",
            name.to_bytes(),
        ]));
    }
    true
}

fn reference_error(scope: &mut Scope<'_>, message: &[u8]) -> Value {
    scope.reference_error(&String::from_utf8_lossy(message))
}

pub(crate) fn module_source(
    js: Js,
    scope: &mut Scope<'_>,
    name: &CStr,
    json: bool,
) -> Result<Vec<u8>, Value> {
    if over_limit(js, name) {
        return Err(scope.range_error("module load limit exceeded"));
    }
    if starts_with(name, b"data:") {
        let Some(body) = ffi::decode_data_url(name) else {
            return Err(reference_error(scope, b"invalid data: module URL"));
        };
        count_load(js, false, body.len());
        return Ok(body);
    }
    if starts_with(name, b"blob:") {
        let Some(body) = js.with_blob(name, <[u8]>::to_vec) else {
            let message = line(&[b"blob module URL not found: ", name.to_bytes()]);
            return Err(reference_error(scope, &message));
        };
        if body.len() > MAX_SCRIPT_BYTES {
            return Err(scope.range_error("blob module exceeds script size limit"));
        }
        count_load(js, true, body.len());
        return Ok(body);
    }
    if !ffi::url_is_http_or_https(name) {
        let message = line(&[
            b"module specifier must be absolute http/https URL: ",
            name.to_bytes(),
        ]);
        return Err(reference_error(scope, &message));
    }
    count_load(js, true, 0);
    let info = js.module_perf_info(scope.handle());
    let fetched = js.fetch(name, js.module_top_url(), !json, c"script", &info);
    let body = fetched
        .response
        .as_ref()
        .filter(|resp| resp.error().is_none())
        .and_then(|resp| resp.body())
        .filter(|body| !body.is_empty() && body.len() <= MAX_SCRIPT_BYTES)
        .map(<[u8]>::to_vec);
    let Some(body) = body else {
        if !js.is_null() {
            let why = failure_reason(&fetched, b"fetch failed");
            js.log(&line(&[b"module ", name.to_bytes(), b": ", &why]));
        }
        let message = line(&[b"module fetch failed: ", name.to_bytes()]);
        return Err(reference_error(scope, &message));
    };
    count_load(js, false, body.len());
    Ok(body)
}
