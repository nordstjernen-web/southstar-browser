//! Southstar — a frame's origin as its embedder sees it: the URL its document shows, whether it is cross-origin to the embedder and the referrer its document starts with.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;
use std::ffi::{CStr, CString};

use southstar_dom::{Kind, Node};

use crate::ffi::ctx::Ctx;
use crate::ffi::load;
use crate::ffi::{self, Js};
use crate::page;
use crate::realm::frame_of;
use crate::sandbox;

const REFERER_NO_REFERRER: c_int = 0;
const REFERER_SAME_ORIGIN: c_int = 1;
const REFERER_STRICT_ORIGIN_WHEN_CROSS: c_int = 2;
const REFERER_UNSAFE_URL: c_int = 3;

pub(crate) fn doc_url(iframe: Option<Node<'_>>) -> Option<&CStr> {
    iframe
        .and_then(|iframe| iframe.attr(c"data-nd-frame-doc-url"))
        .filter(|url| !url.is_empty())
}

pub(crate) fn opaque_by_sandbox(flags: u32) -> bool {
    flags & sandbox::ACTIVE != 0 && flags & sandbox::ALLOW_SAME_ORIGIN == 0
}

pub(crate) fn is_cross_origin(js: Js, iframe: Node<'_>) -> bool {
    if opaque_by_sandbox(sandbox::effective(iframe)) {
        return true;
    }
    let Some(frame_url) = iframe
        .attr(c"data-nd-frame-url")
        .filter(|url| !url.is_empty())
    else {
        return false;
    };
    let embedder = load::node_frame_url(iframe)
        .or_else(|| load::document_origin(js))
        .or_else(|| load::page_url(js));
    embedder.is_none_or(|embedder| !load::same_origin(frame_url, &embedder))
}

enum RealmOrigin {
    Unrelated,
    Opaque,
    Tuple(CString),
}

fn embedder_origin(js: Js, iframe: Node<'_>) -> RealmOrigin {
    let embedder = load::node_frame_url(iframe)
        .or_else(|| load::document_origin(js))
        .or_else(|| load::page_url(js));
    match load::origin_from(embedder.as_deref()) {
        Some(origin) => RealmOrigin::Tuple(origin),
        None => RealmOrigin::Opaque,
    }
}

fn realm_origin(js: Js, realm: Ctx) -> RealmOrigin {
    if realm == ffi::main_realm(js) {
        let page = load::document_origin(js).or_else(|| load::page_url(js));
        return match load::origin_from(page.as_deref()) {
            Some(origin) => RealmOrigin::Tuple(origin),
            None => RealmOrigin::Opaque,
        };
    }
    let Some(iframe) = page::frame_of_realm(js, realm).and_then(ffi::frame_node) else {
        return RealmOrigin::Unrelated;
    };
    if opaque_by_sandbox(sandbox::effective(iframe)) {
        return RealmOrigin::Opaque;
    }
    match iframe
        .attr(c"data-nd-frame-url")
        .filter(|url| !url.is_empty() && !url.to_bytes().starts_with(b"about:"))
    {
        Some(url) => match load::origin_from(Some(url)) {
            Some(origin) => RealmOrigin::Tuple(origin),
            None => RealmOrigin::Opaque,
        },
        None => embedder_origin(js, iframe),
    }
}

pub(crate) fn realms_same_origin(js: Js, a: Ctx, b: Ctx) -> bool {
    match (realm_origin(js, a), realm_origin(js, b)) {
        (RealmOrigin::Unrelated, _) | (_, RealmOrigin::Unrelated) => true,
        (RealmOrigin::Tuple(a), RealmOrigin::Tuple(b)) => load::same_origin(&a, &b),
        _ => false,
    }
}

fn holder_url(js: Js, iframe: Node<'_>) -> Option<CString> {
    let mut holder = iframe.parent();
    while let Some(node) = holder {
        if node.kind() == Kind::Document {
            break;
        }
        holder = node.parent();
    }
    match holder.and_then(|holder| holder.parent()) {
        Some(frame) => page::frame_entry(js, frame_of(frame), |entry| entry.url.clone()).flatten(),
        None => Some(load::top_url(js)),
    }
}

fn referrer_policy(iframe: Node<'_>) -> c_int {
    let configured = southstar_config::get().map_or(REFERER_STRICT_ORIGIN_WHEN_CROSS, |config| {
        config.referer_policy
    });
    let Some(attr) = iframe.attr(c"referrerpolicy").map(CStr::to_bytes) else {
        return configured;
    };
    if attr.eq_ignore_ascii_case(b"no-referrer") {
        REFERER_NO_REFERRER
    } else if attr.eq_ignore_ascii_case(b"same-origin") {
        REFERER_SAME_ORIGIN
    } else if attr.eq_ignore_ascii_case(b"unsafe-url") {
        REFERER_UNSAFE_URL
    } else {
        configured
    }
}

pub(crate) fn frame_referrer(js: Js, iframe: Node<'_>, frame_url: Option<&CStr>) -> CString {
    let holder_url = holder_url(js, iframe);
    let policy = referrer_policy(iframe);
    let has_srcdoc = iframe
        .attr(c"srcdoc")
        .is_some_and(|srcdoc| !srcdoc.is_empty());
    if has_srcdoc && policy != REFERER_UNSAFE_URL {
        let origin = (policy == REFERER_STRICT_ORIGIN_WHEN_CROSS
            && load::is_http_or_https(holder_url.as_deref()))
        .then(|| load::origin_from(holder_url.as_deref()))
        .flatten();
        return match origin {
            Some(origin) => {
                let mut bytes = origin.into_bytes();
                bytes.push(b'/');
                CString::new(bytes).unwrap_or_default()
            }
            None => CString::default(),
        };
    }
    load::referer_for(frame_url, holder_url.as_deref(), policy).unwrap_or_default()
}

pub(crate) fn mark_cross_origin(js: Js, iorigin: &CStr, flags: u32) -> u32 {
    let page_url = load::page_url(js);
    let cross_url = page_url
        .as_deref()
        .is_some_and(|page_url| !load::same_origin(iorigin, page_url));
    if cross_url || opaque_by_sandbox(flags) {
        flags | CROSS_ORIGIN
    } else {
        flags
    }
}

pub(crate) const CROSS_ORIGIN: u32 = 1 << 13;
