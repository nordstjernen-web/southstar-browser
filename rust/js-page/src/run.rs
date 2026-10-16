//! Southstar — running a freshly parsed document: lifecycle reset, installation, early injected scripts, content scripts, its frames, images and stylesheet timings, then its scripts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::MAX_DEPTH;

use crate::Element;
use crate::ffi::{self, Js};
use crate::install::install_document;

const IMG_LOAD_FIRED: u32 = 1 << 3;

pub(crate) fn run_scripts_in_doc(js: Js, doc: Element, base_url: Option<&CStr>) {
    if ffi::profile_enabled() {
        let mut line = format!(
            "[profile] js run_scripts_in_doc start halted={} in_pump={} url=",
            i32::from(js.halted()),
            i32::from(js.in_pump())
        )
        .into_bytes();
        line.extend_from_slice(base_url.map_or(&b"(null)"[..], CStr::to_bytes));
        line.push(b'\n');
        southstar_glib::stderr_write(&line);
    }
    if js.halted() || js.in_pump() {
        return;
    }
    js.clear_lifecycle();
    let start_us = ffi::monotonic_us();
    install_document(js, Some(doc), base_url);
    js.snapshot_platform_globals();
    run_early_injected(js);
    let page_url = base_url.filter(|url| !url.is_empty());
    js.run_content_scripts(page_url);
    js.schedule_static_iframes(doc);
    track_document_images(js, doc, 0);
    js.record_stylesheet_timings(doc, base_url);
    js.run_document_scripts(doc, page_url.unwrap_or(c"inline"), start_us);
}

fn run_early_injected(js: Js) {
    if let Some(path) = std::env::var_os("NS_EARLY_JS_FILE").filter(|path| !path.is_empty())
        && let Ok(mut bytes) = std::fs::read(path)
    {
        if let Some(end) = bytes.iter().position(|&b| b == 0) {
            bytes.truncate(end);
        }
        if let Ok(source) = CString::new(bytes) {
            js.eval_source(&source, c"early-inject");
        }
    }
    if let Some(source) = js.early_inject_src().filter(|src| !src.is_empty()) {
        js.eval_source(&source, c"early-inject");
    }
}

fn is_named(node: Option<Element>, tag: &[u8]) -> bool {
    node.is_some_and(|node| node.element_name() == Some(tag))
}

fn track_document_images(js: Js, node: Element, depth: i32) {
    if depth >= MAX_DEPTH || (depth > 0 && ffi::hidden_child(node)) {
        return;
    }
    if is_named(Some(node), b"img") {
        let loading = node.attr(c"loading");
        if let Some(src) = node.attr(c"src").filter(|src| !src.is_empty())
            && node.attr(c"srcset").is_none()
            && !loading.is_some_and(|l| l.to_bytes().eq_ignore_ascii_case(b"lazy"))
            && !is_named(node.parent(), b"picture")
            && !js.image_load_tracked(node)
            && node.flags() & IMG_LOAD_FIRED == 0
        {
            js.start_image_load(node, src);
        }
        return;
    }
    if is_named(Some(node), b"template") {
        return;
    }
    let mut child = node.first_child();
    while let Some(c) = child {
        track_document_images(js, c, depth + 1);
        child = c.next_sibling();
    }
}
