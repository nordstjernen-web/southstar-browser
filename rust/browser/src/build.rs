//! Southstar — bringing up a page from its parsed document (caches, cascade, keyframes, the script engine, declarative refresh, settling, the fragment target) and its form submissions.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::{Kind, Node, children};
use std::ffi::CString;

use crate::callbacks;
use crate::ffi::{self, NavigationTiming, NsBrowser, StrBufOwned};
use crate::{page, scroll, settle};

const MAX_DEPTH: c_int = 1024;
const LOAD_DELAY_US: i64 = 10 * 1_000_000;

pub struct Setup<'a> {
    pub viewport_width: c_int,
    pub viewport_height: f64,
    pub settle_ms: c_int,
    pub bfcache_ok: bool,
    pub refresh_header: Option<CString>,
    pub csp_header: Option<CString>,
    pub url: Option<&'a CStr>,
    pub navigation_timing: Option<NavigationTiming>,
}

pub fn init() {
    ffi::init_subsystems();
}

fn find_meta_refresh(n: Node<'_>, depth: c_int) -> Option<&CStr> {
    if depth > MAX_DEPTH {
        return None;
    }
    if ffi::is_named(n, c"meta")
        && n.attr(c"http-equiv")
            .is_some_and(|e| e.to_bytes().eq_ignore_ascii_case(b"refresh"))
        && let Some(content) = n.attr(c"content").filter(|c| !c.is_empty())
    {
        return Some(content);
    }
    children(n).find_map(|c| find_meta_refresh(c, depth + 1))
}

fn arm_declarative_refresh(b: &NsBrowser, header: Option<&CStr>) {
    let parsed = header.and_then(ffi::parse_refresh).or_else(|| {
        b.doc()
            .and_then(|doc| find_meta_refresh(doc, 0))
            .and_then(ffi::parse_refresh)
    });
    let Some((seconds, target)) = parsed else {
        return;
    };
    match target {
        Some(target) => {
            b.refresh_url
                .adopt(callbacks::resolve_navigation(b, &target));
            if !b.refresh_url.is_set() {
                return;
            }
        }
        None => b.refresh_url.set(b.base_url.get().as_deref()),
    }
    b.refresh_due_us
        .set(ffi::monotonic_us() + (seconds * 1e6) as i64);
}

fn apply_meta_csp(js: ffi::Js, node: Node<'_>, depth: c_int) {
    if depth > MAX_DEPTH {
        return;
    }
    for c in children(node) {
        let is_meta = c.kind() == Kind::Element
            && c.name()
                .is_some_and(|name| name.to_bytes().eq_ignore_ascii_case(b"meta"));
        if is_meta
            && c.attr(c"http-equiv").is_some_and(|he| {
                he.to_bytes()
                    .eq_ignore_ascii_case(b"content-security-policy")
            })
            && let Some(content) = c.attr(c"content").filter(|c| !c.is_empty())
        {
            js.add_csp_header(Some(content));
        }
        apply_meta_csp(js, c, depth + 1);
    }
}

pub fn build(b: &NsBrowser, setup: Setup<'_>) {
    let vw = if setup.viewport_width > 0 {
        setup.viewport_width
    } else {
        1000
    };
    let vh = if setup.viewport_height > 0.0 {
        setup.viewport_height
    } else {
        f64::from(vw) * 0.75
    };
    ffi::css_set_viewport(f64::from(vw), vh);
    let fragment = scroll::url_fragment(setup.url);
    ffi::css_set_target_fragment(if fragment.present {
        fragment.target()
    } else {
        None
    });
    if southstar_config::get().is_some_and(|c| c.speculative_preload != 0) {
        b.speculative_preload();
    }
    b.pending_scroll_x.set(-1);
    b.dppx.set(ffi::css_device_pixel_ratio());
    ffi::css_set_doc_language(b.doc_language.get().as_deref());
    b.vw.set(vw);
    b.vh.set(vh);
    b.bfcache_ok.set(setup.bfcache_ok);
    b.caret_paint_visible.set(true);
    ffi::attach_caches(b);
    if let Some(videos) = b.videos() {
        videos.set_base(b.base_url.get().as_deref().map(CStr::to_bytes));
    }
    b.compute_cascade();
    let anim = ffi::Anim::create();
    b.load_keyframes(&anim);
    ffi::set_anim(b, anim);
    if let Some(js) = ffi::attach_js(b, setup.navigation_timing) {
        b.load_delay_deadline_us
            .set(ffi::monotonic_us() + LOAD_DELAY_US);
        ffi::wire_js_callbacks(b, js);
        js.add_csp_header(setup.csp_header.as_deref());
        if let Some(doc) = b.doc() {
            apply_meta_csp(js, doc, 0);
        }
        if southstar_config::get().is_none_or(|c| c.javascript_enabled != 0) {
            js.run_scripts_in_doc(b.doc(), b.base_url.get().as_deref());
        }
    }
    drop(setup.csp_header);
    if let Some(videos) = b.videos() {
        ffi::wire_video_callbacks(b, videos);
    }
    arm_declarative_refresh(b, setup.refresh_header.as_deref());
    drop(setup.refresh_header);
    if b.layout().is_none() || b.dirty.get() {
        page::relayout(b);
    }
    settle::settle(b, setup.settle_ms);
    if b.layout().is_none() || b.dirty.get() {
        page::relayout(b);
    }
    match fragment
        .target()
        .and_then(|id| ffi::find_fragment_target(b.doc(), id))
    {
        Some(target) => scroll::queue_scroll_to(b, target, true),
        None if fragment.present && fragment.target().is_none() => {
            b.pending_scroll_y.set(0);
            b.pending_scroll.set(true);
        }
        None => {}
    }
}

fn submitter_attr<'a>(clicked: Option<Node<'a>>, from_text: bool, name: &CStr) -> Option<&'a CStr> {
    clicked
        .filter(|_| !from_text)
        .and_then(|c| c.attr(name))
        .filter(|v| !v.is_empty())
}

fn perform_form_navigation(b: &NsBrowser, form: Node<'_>, clicked: Option<Node<'_>>) {
    let Some(doc) = b.doc() else {
        return;
    };
    let from_text = clicked.is_some_and(ffi::is_text_input);
    let method = submitter_attr(clicked, from_text, c"formmethod").or_else(|| form.attr(c"method"));
    let is_post = method.is_some_and(|m| m.to_bytes().eq_ignore_ascii_case(b"post"));
    let action = submitter_attr(clicked, from_text, c"formaction").or_else(|| form.attr(c"action"));
    let abs_action = match action.filter(|a| !a.is_empty()) {
        Some(action) => ffi::url_resolve(b.base_url.get().as_deref(), action),
        None => b.base_url.get().as_deref().and_then(ffi::dup_text),
    };
    let Some(abs_action) = abs_action else {
        return;
    };
    if !callbacks::allows_navigation_url(b, Some(&abs_action)) {
        return;
    }
    if b.js()
        .is_some_and(|js| !js.csp_form_action_allowed(form, &abs_action))
    {
        return;
    }
    let accept_charset = form.attr(c"accept-charset").filter(|c| !c.is_empty());
    let doc_charset = b.doc_charset.get();
    ffi::set_submission_charset(accept_charset.or(doc_charset.as_deref()));
    let body = StrBufOwned::new();
    body.append(&southstar_forms::urlencoded_inputs(form, doc, clicked));
    ffi::set_submission_charset(None);
    if is_post {
        b.set_post(body, c"application/x-www-form-urlencoded");
        b.pending_nav.adopt(Some(abs_action));
        return;
    }
    let action = abs_action.to_bytes();
    let action = &action[..action
        .iter()
        .position(|&c| c == b'#')
        .unwrap_or(action.len())];
    let mut full = action.to_vec();
    let body = body.into_bytes();
    if !body.is_empty() {
        full.push(if action.contains(&b'?') { b'&' } else { b'?' });
        full.extend_from_slice(&body);
    }
    b.pending_nav.adopt(ffi::owned_text(&full));
}

pub fn js_form_submit(b: &NsBrowser, form: Node<'_>, submitter: Option<Node<'_>>) {
    perform_form_navigation(b, form, Some(submitter.unwrap_or(form)));
}

pub fn submit_form(b: &NsBrowser, clicked: Option<Node<'_>>) {
    let (Some(clicked), Some(doc)) = (clicked, b.doc()) else {
        return;
    };
    let from_text = ffi::is_text_input(clicked);
    let from_form = ffi::is_named(clicked, c"form");
    if !from_text && !from_form && !southstar_forms::is_submit_trigger(clicked) {
        return;
    }
    let form = if from_form {
        Some(clicked)
    } else {
        ffi::form_owner(clicked)
    };
    let Some(form) = form else {
        return;
    };
    if form.attr(c"novalidate").is_none()
        && clicked.attr(c"formnovalidate").is_none()
        && let Some(bad) = southstar_forms::first_invalid(form, doc)
    {
        if let Some(js) = b.js() {
            js.dispatch_event(bad, c"invalid");
        }
        return;
    }
    if let Some(js) = b.js() {
        let prevented = js.dispatch_submit_event(form, clicked);
        if js.consume_mutated() {
            b.dirty.set(true);
        }
        if prevented {
            return;
        }
    }
    match b.js() {
        Some(js) => js.submit_form(form, clicked),
        None => perform_form_navigation(b, form, Some(clicked)),
    }
}
