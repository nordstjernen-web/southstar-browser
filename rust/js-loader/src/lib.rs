//! Southstar — script, module and stylesheet loading: which scripts run when, import maps, module fetching and the load and error events of script and link elements.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod api;
mod blocking;
mod compile;
mod eval;
mod fetch;
pub(crate) mod ffi;
mod hold;
mod import_map;
mod link;
mod page;
mod prefetch;
mod scan;
mod schedule;

use core::ffi::CStr;

use southstar_dom::Node;
use southstar_js_runtime::api::Page;

use crate::ffi::Js;

pub use api::*;
pub use ffi::install_runtime_hooks;

pub(crate) const ALREADY_STARTED: &CStr = c"data-nd-script-already-started";
pub(crate) const EMPTY_SOURCE: &CStr = c"data-nd-script-empty-source";
pub(crate) const MAX_SCRIPT_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const FLAG_LINK_LOAD_FIRED: u32 = 1 << 8;
pub(crate) const FLAG_NOT_PARSER_INSERTED: u32 = 1 << 13;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Schedule {
    Blocking,
    Deferred,
    Async,
}

#[derive(Clone, Copy)]
pub(crate) struct Task<'a> {
    pub node: Node<'a>,
    pub schedule: Schedule,
}

fn eq_ignore_case(value: Option<&CStr>, want: &[u8]) -> bool {
    value.is_some_and(|value| value.to_bytes().eq_ignore_ascii_case(want))
}

pub(crate) fn type_is_module(n: Node) -> bool {
    eq_ignore_case(n.attr(c"type"), b"module")
}

pub(crate) fn type_supported(n: Node) -> bool {
    match n.attr(c"type").map(CStr::to_bytes) {
        None | Some(b"") => true,
        Some(ty) => [
            &b"text/javascript"[..],
            b"application/javascript",
            b"module",
        ]
        .iter()
        .any(|ok| ty.eq_ignore_ascii_case(ok)),
    }
}

pub(crate) fn content_type_is_json(ct: &[u8]) -> bool {
    let essence = ct
        .split(|&b| b == b';')
        .next()
        .unwrap_or_default()
        .trim_ascii()
        .to_ascii_lowercase();
    essence == b"application/json" || essence == b"text/json" || essence.ends_with(b"+json")
}

pub fn content_type_is_javascript(ct: &[u8]) -> bool {
    const OK: [&[u8]; 10] = [
        b"text/javascript",
        b"application/javascript",
        b"application/ecmascript",
        b"text/ecmascript",
        b"application/x-javascript",
        b"text/x-javascript",
        b"application/x-ecmascript",
        b"text/x-ecmascript",
        b"text/jscript",
        b"text/livescript",
    ];
    let start = ct
        .iter()
        .position(|&b| b != b' ' && b != b'\t')
        .unwrap_or(ct.len());
    let ct = &ct[start..];
    let end = ct
        .iter()
        .position(|&b| matches!(b, b';' | b' ' | b'\t'))
        .unwrap_or(ct.len());
    let essence = &ct[..end];
    !essence.is_empty() && OK.iter().any(|ok| essence.eq_ignore_ascii_case(ok))
}

pub(crate) fn skipped_by_nomodule(n: Node) -> bool {
    !type_is_module(n) && n.attr(c"nomodule").is_some()
}

pub(crate) fn schedule_for(n: Node) -> Schedule {
    let is_module = type_is_module(n);
    let has_src = n.attr(c"src").is_some();
    let has_async = n.attr(c"async").is_some();
    let has_defer = n.attr(c"defer").is_some();
    if has_async && (has_src || is_module) {
        Schedule::Async
    } else if is_module || (has_src && has_defer) {
        Schedule::Deferred
    } else {
        Schedule::Blocking
    }
}

pub fn link_attribute_changed(js: Option<Page>, link: Node, name: &CStr, old: Option<&CStr>) {
    link::attribute_changed(Js::of(js), link, name, old);
}

pub fn link_disconnected(js: Option<Page>, node: Node<'_>) {
    let js = Js::of(js);
    if !js.is_null() {
        link::disconnected(js, node);
    }
}

pub fn detach_link_fetches(js: Option<Page>) {
    let js = Js::of(js);
    if !js.is_null() {
        link::detach(js);
    }
}

pub fn link_fetches_pending(js: Option<Page>) -> bool {
    let js = Js::of(js);
    !js.is_null() && link::fetches_pending(js)
}

pub fn link_sheets_pending(js: Option<Page>) -> bool {
    let js = Js::of(js);
    !js.is_null() && link::sheets_pending(js)
}

pub fn fire_parsed_stylesheet_loads(js: Option<Page>, doc: Node) {
    let js = Js::of(js);
    if !js.is_null() {
        fetch::fire_parsed_stylesheet_loads(js, doc);
    }
}

pub(crate) fn mark(n: Node, name: &CStr) {
    southstar_dom::attrs::set_len(n, name, Some(b"1"));
}

pub(crate) fn unmark(n: Node, name: &CStr) {
    southstar_dom::attrs::remove(n, name);
}

pub(crate) fn source_is_empty(n: Node) -> bool {
    southstar_dom::children(n).all(|c| !c.is_text() || c.text().is_none_or(CStr::is_empty))
}
