//! Southstar — the page-level window glue: fragment navigation, viewport scrolling, resize, pageshow/pagehide and the document origin resources are checked against.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashSet;

use southstar_js_engine::Value;

use crate::ffi::{self, Js};

thread_local! {
    static SCROLL_DISPATCHING: RefCell<HashSet<Js>> = RefCell::new(HashSet::new());
}

pub(crate) fn anchor_fragment_navigate(js: Js, abs_url: &[u8]) -> bool {
    let current = ffi::current_url(js);
    if current.is_empty() {
        return false;
    }
    let Some(hash) = abs_url.iter().position(|&b| b == b'#') else {
        return false;
    };
    let current_base = current.split(|&b| b == b'#').next().unwrap_or_default();
    if current_base != &abs_url[..hash] {
        return false;
    }
    if current == abs_url {
        ffi::fragment_navigated(js, abs_url);
        return true;
    }
    ffi::set_current_url(js, abs_url);
    ffi::soft_navigate(js, &ffi::current_url(js), false);
    ffi::fragment_navigated(js, &ffi::current_url(js));
    let new_url = ffi::current_url(js);
    ffi::dispatch_hashchange(js, &current, &new_url);
    true
}

pub(crate) fn note_viewport_scroll(js: Js, x: f64, y: f64) {
    if js.is_null() || ffi::halted(js) {
        return;
    }
    let Some(dispatch) = ffi::with_main_context(js, |scope| {
        if ffi::window_scroll_prop(scope, c"scrollX") != x
            || ffi::window_scroll_prop(scope, c"scrollY") != y
        {
            ffi::queue_scrollend_document(js);
        }
        ffi::set_hit_viewport(x, y);
        let global = scope.global();
        for (key, value) in [
            ("scrollX", x),
            ("scrollY", y),
            ("pageXOffset", x),
            ("pageYOffset", y),
        ] {
            let _ = scope.set(&global, key, Value::number(value));
        }
        !SCROLL_DISPATCHING.with(|set| set.borrow().contains(&js))
    }) else {
        return;
    };
    if !dispatch {
        return;
    }
    SCROLL_DISPATCHING.with(|set| set.borrow_mut().insert(js));
    match ffi::current_document(js) {
        Some(doc) => ffi::dispatch_node_event(js, doc, c"scroll"),
        None => {
            if let Some(event) =
                ffi::with_main_context(js, |scope| ffi::make_window_event(scope, "scroll"))
            {
                ffi::dispatch_window_only_event(js, None, c"scroll", event);
            }
        }
    }
    SCROLL_DISPATCHING.with(|set| set.borrow_mut().remove(&js));
    ffi::schedule_observer_tick(js);
}

pub(crate) fn dispatch_resize(js: Js) {
    let Some(event) = ffi::with_main_context(js, |scope| ffi::make_window_event(scope, "resize"))
    else {
        return;
    };
    ffi::dispatch_window_only_event(js, ffi::current_document(js), c"resize", event);
    ffi::reeval_media_queries(js);
}

pub(crate) fn fire_page_transition(js: Js, kind: &core::ffi::CStr, persisted: bool) {
    let doc = ffi::current_document(js);
    let Some(event) = ffi::with_main_context(js, |scope| {
        let event = ffi::make_node_event(scope, kind, doc);
        let _ = scope.set(&event, "persisted", Value::boolean(persisted));
        event
    }) else {
        return;
    };
    ffi::dispatch_window_only_event(js, doc, kind, event);
}

fn share_http_authority(a: &[u8], b: &[u8]) -> bool {
    if !ffi::url_is_http_or_https(a) {
        return false;
    }
    let Some(scheme_end) = a.windows(3).position(|w| w == b"://") else {
        return false;
    };
    let authority = scheme_end + 3;
    let authority_len = a[authority..]
        .iter()
        .position(|c| b"/?#\\".contains(c))
        .unwrap_or(a.len() - authority);
    if authority_len == 0 {
        return false;
    }
    let n = authority + authority_len;
    b.len() >= n && a[..n] == b[..n] && (b.len() == n || matches!(b[n], b'/' | b'?' | b'#'))
}

fn urls_same_origin(a: &[u8], b: Option<&[u8]>) -> bool {
    let Some(b) = b else {
        return false;
    };
    if share_http_authority(a, b) {
        return true;
    }
    if ffi::url_is_http_or_https(a) || ffi::url_is_http_or_https(b) {
        return ffi::url_same_origin(a, b);
    }
    let colon = |s: &[u8]| s.iter().position(|&c| c == b':');
    match (colon(a), colon(b)) {
        (Some(ca), Some(cb)) => ca == cb && a[..ca].eq_ignore_ascii_case(&b[..cb]),
        _ => false,
    }
}

pub(crate) fn resource_origin_clean(
    caller_document_url: Option<Vec<u8>>,
    url: &[u8],
    cors_allow_origin: Option<&[u8]>,
) -> bool {
    if url.starts_with(b"data:") || url.starts_with(b"blob:") {
        return true;
    }
    let doc_url = caller_document_url.as_deref();
    if urls_same_origin(url, doc_url) {
        return true;
    }
    cors_allow_origin.is_some_and(|allow| ffi::cors_allows(doc_url, url, allow))
}
