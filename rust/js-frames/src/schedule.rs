//! Southstar — a frame's source and its load scheduling: the source a frame shows, whether that source is already loaded, lazy frames left outside the load range, and the frames of a parsed tree.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::{CStr, CString};

use southstar_dom::{MAX_DEPTH, Node};

use crate::ffi::{self, Js};
use crate::page;
use crate::queue;
use crate::realm::frame_of;

const LOAD_RANGE_MARGIN: f64 = 1500.0;

fn non_empty(text: Option<&CStr>) -> Option<&CStr> {
    text.filter(|text| !text.is_empty())
}

pub(crate) fn src_attr(node: Node<'_>) -> Option<&'static CStr> {
    match node.element_name() {
        Some(b"iframe") => Some(c"src"),
        Some(b"object") => Some(c"data"),
        _ => None,
    }
}

pub(crate) fn mark_source(js: Js, iframe: Node<'_>, origin: Option<&CStr>, abs_url: Option<&CStr>) {
    let srcdoc = non_empty(iframe.attr(c"srcdoc")).map(CStr::to_owned);
    let abs_url = non_empty(abs_url);
    let frame_url = match (&srcdoc, abs_url) {
        (None, Some(url)) => Some(url),
        _ => origin,
    };
    if !js.is_null() {
        let referrer = Some(crate::origin::frame_referrer(js, iframe, frame_url));
        let url = frame_url.map_or_else(CString::default, CStr::to_owned);
        page::update_frame(js, frame_of(iframe), |entry| {
            entry.url = Some(url);
            entry.referrer = referrer;
        });
    }
    if let Some(srcdoc) = srcdoc {
        ffi::set_attr(iframe, c"data-nd-frame-srcdoc", Some(&srcdoc));
        ffi::set_attr(iframe, c"data-nd-frame-url", origin);
        ffi::set_attr(iframe, c"data-nd-frame-doc-url", Some(c"about:srcdoc"));
        return;
    }
    ffi::set_attr(iframe, c"data-nd-frame-srcdoc", Some(c""));
    ffi::set_attr(iframe, c"data-nd-frame-url", abs_url.or(origin));
    let doc_url = if abs_url.is_some() {
        c""
    } else {
        c"about:blank"
    };
    ffi::set_attr(iframe, c"data-nd-frame-doc-url", Some(doc_url));
    if let Some(url) = abs_url {
        ffi::mark_visited(url);
    }
}

fn source_loaded(js: Js, iframe: Node<'_>) -> bool {
    if iframe.attr(c"data-nd-frame-loaded").is_none() {
        return false;
    }
    if let Some(srcdoc) = non_empty(iframe.attr(c"srcdoc")) {
        return iframe.attr(c"data-nd-frame-srcdoc") == Some(srcdoc);
    }
    let src = src_attr(iframe).and_then(|attr| iframe.attr(attr));
    let Some(src) = non_empty(src).filter(|src| !src.to_bytes().starts_with(b"about:")) else {
        return iframe.element_name() == Some(b"iframe");
    };
    let current = ffi::current_url(js);
    let origin = non_empty(current.as_deref()).unwrap_or(c"inline");
    let Some(abs_url) = ffi::resolve_url(origin, src) else {
        return false;
    };
    iframe.attr(c"data-nd-frame-url") == Some(abs_url.as_c_str())
}

pub(crate) fn beyond_load_range(js: Js, iframe: Node<'_>) -> bool {
    let Some(root) = ffi::layout_root(js) else {
        return true;
    };
    let limit = root.scroll_y() + ffi::viewport_height() + LOAD_RANGE_MARGIN;
    ffi::find_box(root, iframe).is_none_or(|found| found.y() > limit)
}

pub(crate) fn schedule(js: Js, iframe: Node<'_>, force: bool) {
    if js.is_null() || ffi::halted(js) {
        return;
    }
    if src_attr(iframe).is_none() {
        return;
    }
    if !force && source_loaded(js, iframe) {
        return;
    }
    let lazy = iframe
        .attr(c"loading")
        .is_some_and(|loading| loading.to_bytes().eq_ignore_ascii_case(b"lazy"));
    if !force && lazy && !ffi::in_frame_load(js) && beyond_load_range(js, iframe) {
        queue::add_deferred(js, frame_of(iframe));
        return;
    }
    if force {
        ffi::remove_attr(iframe, c"data-nd-doc-written");
        ffi::remove_attr(iframe, c"data-nd-frame-loaded");
        ffi::mark_attr_dirty(iframe, c"data-nd-frame-loaded", c"1");
    }
    queue::remove_deferred(js, frame_of(iframe));
    if !queue::add_pending(js, frame_of(iframe)) {
        return;
    }
    ffi::mark_mutated(js);
}

pub(crate) fn schedule_static(js: Js, node: Node<'_>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    if let Some(attr) = src_attr(node) {
        if node.attr(c"data-nd-frame-loaded").is_none() {
            let has_src = non_empty(node.attr(attr)).is_some();
            let has_srcdoc = non_empty(node.attr(c"srcdoc")).is_some();
            if has_src || has_srcdoc || node.element_name() == Some(b"iframe") {
                schedule(js, node, false);
            }
        }
        return;
    }
    for child in southstar_dom::children(node) {
        schedule_static(js, child, depth + 1);
    }
}

pub(crate) fn response_is_download(
    error: Option<&CStr>,
    disposition: Option<&CStr>,
    content_type: Option<&CStr>,
) -> bool {
    if error.is_some() {
        return false;
    }
    if let Some(disposition) = disposition {
        let disposition = disposition.to_bytes().trim_ascii_start();
        if disposition.len() >= 10 && disposition[..10].eq_ignore_ascii_case(b"attachment") {
            return true;
        }
    }
    let Some(content_type) = non_empty(content_type) else {
        return false;
    };
    let content_type = content_type.to_bytes();
    let end = content_type
        .iter()
        .position(|&b| b == b';')
        .unwrap_or(content_type.len());
    let mime = content_type[..end].to_ascii_lowercase();
    let mime = mime.trim_ascii();
    let shown = mime.is_empty()
        || ["text/", "image/", "video/", "audio/"]
            .iter()
            .any(|prefix| mime.starts_with(prefix.as_bytes()))
        || mime.ends_with(b"+xml")
        || matches!(
            mime,
            b"application/xml"
                | b"application/xhtml+xml"
                | b"application/json"
                | b"application/pdf"
        );
    !shown
}
