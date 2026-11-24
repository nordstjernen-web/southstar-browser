//! Southstar — loading `<link>` elements: stylesheets and preloads of every `as` fetched in the background, their load and error events, and the fetches several links share.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell};
use core::ffi::{CStr, c_long};
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Node, WeakNode};
use southstar_glib::SourceId;
use southstar_js_runtime::api;

use crate::ffi::{
    self, CSP_CONNECT, CSP_FONT, CSP_IMG, CSP_MEDIA, CSP_SCRIPT, CSP_STYLE, Js, PerfInfo,
};
use crate::{FLAG_LINK_LOAD_FIRED, FLAG_NOT_PARSER_INSERTED, page, schedule};

const RESULTS_CAP: usize = 256;
const HELD_RETRY_MS: u32 = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    None,
    Stylesheet,
    Preload,
}

struct AsEntry {
    name: &'static [u8],
    csp: usize,
    lenient: bool,
}

const AS_TABLE: [AsEntry; 6] = [
    AsEntry {
        name: b"style",
        csp: CSP_STYLE,
        lenient: false,
    },
    AsEntry {
        name: b"script",
        csp: CSP_SCRIPT,
        lenient: false,
    },
    AsEntry {
        name: b"font",
        csp: CSP_FONT,
        lenient: false,
    },
    AsEntry {
        name: b"image",
        csp: CSP_IMG,
        lenient: false,
    },
    AsEntry {
        name: b"track",
        csp: CSP_MEDIA,
        lenient: true,
    },
    AsEntry {
        name: b"fetch",
        csp: CSP_CONNECT,
        lenient: true,
    },
];

const FONT_TYPES: [&[u8]; 11] = [
    b"font/woff2",
    b"font/woff",
    b"font/ttf",
    b"font/otf",
    b"font/sfnt",
    b"application/font-woff",
    b"application/font-woff2",
    b"application/x-font-woff",
    b"application/x-font-ttf",
    b"application/x-font-otf",
    b"application/font-sfnt",
];

pub(crate) struct LinkFetch {
    nodes: RefCell<Vec<WeakNode>>,
    url: RefCell<Option<Vec<u8>>>,
    info: RefCell<PerfInfo>,
    start_us: Cell<i64>,
    status: Cell<i64>,
    sheet: Cell<bool>,
    live: Cell<bool>,
    source: Cell<Option<SourceId>>,
}

#[allow(clippy::unnecessary_cast)]
fn long(value: c_long) -> i64 {
    value as i64
}

fn is_space(b: &u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn has_token(list: Option<&[u8]>, token: &[u8]) -> bool {
    list.is_some_and(|list| {
        list.split(is_space)
            .any(|part| part.eq_ignore_ascii_case(token))
    })
}

fn as_entry(as_: Option<&[u8]>) -> Option<&'static AsEntry> {
    let as_ = as_?;
    AS_TABLE
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(as_))
}

pub(crate) fn kind_for(rel: Option<&[u8]>, as_: Option<&[u8]>) -> Kind {
    if has_token(rel, b"stylesheet") && !has_token(rel, b"alternate") {
        Kind::Stylesheet
    } else if has_token(rel, b"preload") && as_entry(as_).is_some() {
        Kind::Preload
    } else {
        Kind::None
    }
}

fn attr<'a>(n: Node<'a>, name: &CStr) -> Option<&'a [u8]> {
    n.attr(name).map(CStr::to_bytes)
}

pub(crate) fn kind_of(n: Node) -> Kind {
    kind_for(attr(n, c"rel"), attr(n, c"as"))
}

fn preload_entry(n: Node) -> Option<&'static AsEntry> {
    (kind_of(n) == Kind::Preload)
        .then(|| as_entry(attr(n, c"as")))
        .flatten()
}

fn csp_kind(n: Node) -> usize {
    preload_entry(n).map_or(CSP_STYLE, |entry| entry.csp)
}

fn mime_in(content_type: &[u8], accepted: &[&[u8]]) -> bool {
    let start = content_type
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(content_type.len());
    let rest = &content_type[start..];
    let end = rest
        .iter()
        .position(|&b| matches!(b, b';' | b' ' | b'\t'))
        .unwrap_or(rest.len());
    accepted
        .iter()
        .any(|ok| ok.eq_ignore_ascii_case(&rest[..end]))
}

fn type_supported(as_: &[u8], content_type: Option<&[u8]>) -> bool {
    let Some(ty) = content_type.filter(|ty| !ty.is_empty()) else {
        return true;
    };
    match as_.to_ascii_lowercase().as_slice() {
        b"image" => southstar_css::image_type_supported(ty),
        b"script" => crate::content_type_is_javascript(ty),
        b"style" => mime_in(ty, &[b"text/css"]),
        b"font" => mime_in(ty, &FONT_TYPES),
        b"track" => mime_in(ty, &[b"text/vtt"]),
        _ => true,
    }
}

fn preload_applies(n: Node) -> bool {
    if attr(n, c"media")
        .is_some_and(|media| !media.is_empty() && !southstar_css_media::query_matches(media))
    {
        return false;
    }
    type_supported(attr(n, c"as").unwrap_or_default(), attr(n, c"type"))
}

fn srcset_only_image(n: Node) -> bool {
    attr(n, c"imagesrcset").is_some_and(|set| !set.is_empty())
        && attr(n, c"as").is_some_and(|as_| as_.eq_ignore_ascii_case(b"image"))
}

fn has_source(n: Node, kind: Kind) -> bool {
    attr(n, c"href").is_some_and(|href| !href.is_empty())
        || (kind == Kind::Preload && srcset_only_image(n))
}

pub(crate) fn load_pending(n: Node) -> bool {
    if !n.is_tag(southstar_dom::Tag::Link) || n.flags() & FLAG_LINK_LOAD_FIRED != 0 {
        return false;
    }
    let kind = kind_of(n);
    kind != Kind::None && (kind != Kind::Preload || preload_applies(n)) && has_source(n, kind)
}

fn target_url(js: Js, n: Node, origin: Option<&CStr>) -> Option<CString> {
    let candidate = (kind_of(n) == Kind::Preload && srcset_only_image(n))
        .then(|| {
            southstar_layout::select_srcset(
                attr(n, c"imagesrcset").unwrap_or_default(),
                attr(n, c"imagesizes"),
            )
        })
        .flatten()
        .and_then(|url| CString::new(url).ok());
    let reference = candidate
        .as_deref()
        .or(n.attr(c"href"))
        .filter(|reference| !reference.is_empty())?;
    let base = js.base_url(n);
    ffi::url_resolve(base.as_deref().or(origin), reference)
}

fn status_ok(lenient: bool, status: i64) -> bool {
    if lenient {
        status > 0
    } else {
        (200..300).contains(&status)
    }
}

fn loaded_with(n: Node, status: i64) -> bool {
    status_ok(preload_entry(n).is_some_and(|entry| entry.lenient), status)
}

fn result_reusable(st: &LinkFetch, status: i64) -> bool {
    !st.sheet.get() || status_ok(false, status)
}

fn new_fetch(js: Js, n: Node) -> Rc<LinkFetch> {
    let st = Rc::new(LinkFetch {
        nodes: RefCell::new(vec![n.downgrade()]),
        url: RefCell::new(None),
        info: RefCell::new(js.element_perf_info(n)),
        start_us: Cell::new(0),
        status: Cell::new(0),
        sheet: Cell::new(kind_of(n) == Kind::Stylesheet),
        live: Cell::new(true),
        source: Cell::new(None),
    });
    page::with_page(js, |page| page.link_fetches.push(Rc::clone(&st)));
    st
}

fn is_node(held: WeakNode, n: Node) -> bool {
    held.get().is_some_and(|held| held.as_ptr() == n.as_ptr())
}

fn join(js: Js, n: Node, abs_url: &[u8]) -> bool {
    let joined = page::with_existing(js, |page| {
        page.link_fetches
            .iter()
            .find(|st| st.url.borrow().as_deref() == Some(abs_url))
            .cloned()
    })
    .flatten();
    let Some(st) = joined else {
        return false;
    };
    let mut nodes = st.nodes.borrow_mut();
    nodes.retain(|&held| held.get().is_some() && !is_node(held, n));
    nodes.push(n.downgrade());
    if kind_of(n) == Kind::Stylesheet {
        st.sheet.set(true);
    }
    true
}

fn in_tree(held: WeakNode, root: Node<'_>) -> bool {
    held.get()
        .is_some_and(|n| southstar_dom::ancestors_and_self(n).any(|p| p.as_ptr() == root.as_ptr()))
}

pub(crate) fn drop_in(js: Js, root: Node<'_>) {
    page::with_existing(js, |page| {
        for st in &page.link_fetches {
            st.nodes.borrow_mut().retain(|&held| !in_tree(held, root));
        }
    });
}

pub(crate) fn disconnected(js: Js, node: Node<'_>) {
    if !node.is_tag(southstar_dom::Tag::Link) {
        return;
    }
    node.remove_flags(FLAG_LINK_LOAD_FIRED);
    drop_in(js, node);
}

pub(crate) fn detach(js: Js) {
    let fetches = page::with_existing(js, |page| {
        page.link_results.clear();
        core::mem::take(&mut page.link_fetches)
    })
    .unwrap_or_default();
    for st in fetches {
        st.live.set(false);
        st.nodes.borrow_mut().clear();
        if let Some(source) = st.source.take() {
            js.source_remove(source);
        }
    }
}

pub(crate) fn fetches_pending(js: Js) -> bool {
    page::peek_page(js, |page| !page.link_fetches.is_empty()).unwrap_or(false)
}

pub(crate) fn sheets_pending(js: Js) -> bool {
    page::peek_page(js, |page| page.link_fetches.iter().any(|st| st.sheet.get())).unwrap_or(false)
}

fn schedule_delivery(js: Js, st: &Rc<LinkFetch>, delay_ms: u32) {
    let held = Rc::clone(st);
    let source = api::attach_timeout_once(js.page(), delay_ms, move || deliver(js, &held));
    st.source.set(Some(source));
}

fn next_node<'a>(st: &LinkFetch) -> Option<Node<'a>> {
    let mut nodes = st.nodes.borrow_mut();
    while !nodes.is_empty() {
        if let Some(n) = nodes.remove(0).get() {
            return Some(n);
        }
    }
    None
}

fn deliver(js: Js, st: &Rc<LinkFetch>) {
    st.source.set(None);
    if !st.live.get() {
        return;
    }
    if !js.halted() && js.tasks_held() {
        schedule_delivery(js, st, HELD_RETRY_MS);
        return;
    }
    st.url.borrow_mut().take();
    while !js.halted() && st.live.get() {
        let Some(n) = next_node(st) else {
            break;
        };
        let event = if loaded_with(n, st.status.get()) {
            c"load"
        } else {
            c"error"
        };
        js.dispatch_resource_event(n, event);
    }
    if st.live.get() {
        page::with_existing(js, |page| {
            page.link_fetches.retain(|held| !Rc::ptr_eq(held, st));
        });
    }
}

fn settle(js: Js, st: &Rc<LinkFetch>, status: i64) {
    st.status.set(status);
    schedule_delivery(js, st, 0);
}

fn record(
    js: Js,
    st: &LinkFetch,
    url: &[u8],
    status: i64,
    end_us: i64,
    resp: Option<&southstar_net::Response>,
) {
    if status > 0 {
        page::with_page(js, |page| {
            if page.link_results.len() >= RESULTS_CAP {
                page.link_results.clear();
            }
            page.link_results.insert(url.to_vec(), status);
        });
    }
    let Ok(url) = CString::new(url) else {
        return;
    };
    let mut info = st.info.borrow().clone();
    info.status = status as _;
    info.preload = southstar_glib::boolean(!st.sheet.get());
    if !southstar_js_perf::has_resource_timing(js.page(), info.timeline, &url, Some(c"link")) {
        southstar_js_perf::add_resource_timed(
            js.page(),
            Some(&info),
            &url,
            Some(c"link"),
            (st.start_us.get(), end_us),
            resp,
        );
    }
}

fn finished(js: Js, st: &Rc<LinkFetch>, outcome: southstar_net::Outcome) {
    if !st.live.get() {
        return;
    }
    let url = st.url.borrow().clone().unwrap_or_default();
    let (status, resp) = match &outcome {
        Ok(resp) => (long(resp.status()), Some(resp)),
        Err(_) => (0, None),
    };
    st.status.set(status);
    record(js, st, &url, status, southstar_glib::monotonic_us(), resp);
    if st.sheet.get() && !status_ok(false, status) {
        let why: &[u8] = match &outcome {
            Err(message) if !message.is_empty() => message,
            Ok(resp) => resp.error().map_or(&b"non-200 status"[..], CStr::to_bytes),
            Err(_) => b"fetch failed",
        };
        js.log(&[b"stylesheet ", url.as_slice(), b": ", why].concat());
    }
    schedule_delivery(js, st, 0);
}

fn allowed(js: Js, n: Node, abs_url: &CStr, origin: Option<&CStr>) -> bool {
    let mixed = origin.is_some_and(|o| o.to_bytes().starts_with(b"https://"))
        && abs_url.to_bytes().starts_with(b"http://");
    let blocked_by_csp = !mixed
        && !js.csp_allows(
            Some(n),
            csp_kind(n),
            abs_url,
            origin,
            n.attr(c"nonce"),
            n.flags() & FLAG_NOT_PARSER_INSERTED == 0,
        );
    if mixed || blocked_by_csp {
        let reason: &[u8] = if mixed {
            b"mixed-content blocked: link "
        } else {
            b"CSP blocked: link "
        };
        js.log(&[reason, abs_url.to_bytes()].concat());
    }
    !mixed && !blocked_by_csp
}

fn start(js: Js, st: &Rc<LinkFetch>, n: Node, abs_url: &CStr, origin: Option<&CStr>) {
    let url = abs_url.to_bytes();
    let seen = page::with_existing(js, |page| page.link_results.get(url).copied()).flatten();
    if let Some(status) = seen.filter(|&status| result_reusable(st, status)) {
        settle(js, st, status);
        return;
    }
    if let Some(known) =
        southstar_engine::preload_result_for(url).filter(|known| result_reusable(st, known.status))
    {
        st.start_us.set(known.start_us);
        record(js, st, url, known.status, known.end_us, None);
        settle(js, st, known.status);
        return;
    }
    let headers = if kind_of(n) == Kind::Preload {
        southstar_engine::preload_accept_headers(n.attr(c"rel"), n.attr(c"as"))
    } else {
        southstar_engine::style_accept_headers()
    };
    *st.url.borrow_mut() = Some(url.to_vec());
    st.start_us.set(southstar_glib::monotonic_us());
    let request = southstar_net::FetchRequest {
        url,
        top_url: origin.map(CStr::to_bytes),
        method: Some(b"GET"),
        headers: &headers,
        ..southstar_net::FetchRequest::default()
    };
    let held = Rc::clone(st);
    southstar_net::request_with(&request, None, move |outcome| finished(js, &held, outcome));
}

pub(crate) fn load_element(js: Js, n: Node, origin: Option<&CStr>) {
    if n.flags() & FLAG_LINK_LOAD_FIRED != 0 {
        return;
    }
    n.add_flags(FLAG_LINK_LOAD_FIRED);
    let abs_url = target_url(js, n, origin).filter(|abs_url| allowed(js, n, abs_url, origin));
    let Some(abs_url) = abs_url else {
        let st = new_fetch(js, n);
        settle(js, &st, 0);
        return;
    };
    if join(js, n, abs_url.to_bytes()) {
        return;
    }
    let st = new_fetch(js, n);
    if abs_url.to_bytes().starts_with(b"data:") {
        settle(js, &st, 200);
    } else {
        start(js, &st, n, &abs_url, origin);
    }
}

pub(crate) fn sheet_known(js: Js, link: Node, origin: Option<&CStr>) -> bool {
    kind_of(link) == Kind::Stylesheet
        && target_url(js, link, origin)
            .is_some_and(|abs| southstar_engine::linked_css_known(abs.to_bytes()))
}

fn rel_change_needs_load(link: Node, old: Option<&[u8]>) -> bool {
    let was = kind_for(old, attr(link, c"as"));
    let now = kind_of(link);
    was != now && !(was == Kind::Stylesheet && now == Kind::Preload)
}

fn attr_change_needs_load(link: Node, name: &[u8], old: Option<&[u8]>) -> bool {
    if name.eq_ignore_ascii_case(b"rel") {
        return rel_change_needs_load(link, old);
    }
    let kind = kind_of(link);
    if kind == Kind::None {
        return false;
    }
    if name.eq_ignore_ascii_case(b"as") {
        return kind == Kind::Preload
            && !old
                .unwrap_or_default()
                .eq_ignore_ascii_case(attr(link, c"as").unwrap_or_default());
    }
    name.eq_ignore_ascii_case(b"href") && old != attr(link, c"href")
}

pub(crate) fn attribute_changed(js: Js, link: Node, name: &CStr, old: Option<&CStr>) {
    if js.is_null()
        || !link.is_tag(southstar_dom::Tag::Link)
        || !js.in_page(link)
        || !attr_change_needs_load(link, name.to_bytes(), old.map(CStr::to_bytes))
    {
        return;
    }
    link.remove_flags(FLAG_LINK_LOAD_FIRED);
    drop_in(js, link);
    if load_pending(link) {
        schedule::schedule_deferred_root(js, link);
    }
}
