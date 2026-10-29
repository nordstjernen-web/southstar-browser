//! Southstar — the calls loading a frame's document makes: fetching, decoding and parsing it, the engine state its load runs under and the page events it fires.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;

use southstar_dom::{Node, Tree};
use southstar_js_engine::Value;
use southstar_js_runtime::api::{self, ResourceInfo, Upload};

use super::ctx::Ctx;
use super::imports::nsr_realmdoc_deny_cookie;
use super::{Js, c_string};

pub(crate) use southstar_net::Response;

unsafe extern "C" {
    fn g_get_monotonic_time() -> i64;
}

pub(crate) fn monotonic_time() -> i64 {
    unsafe { g_get_monotonic_time() }
}

pub(crate) fn node_in_page(js: Js, node: Node<'_>) -> bool {
    api::node_in_page(js.ptr(), Some(node))
}

pub(crate) fn page_url(js: Js) -> Option<CString> {
    api::page_url(js.ptr())
}

pub(crate) fn document_origin(js: Js) -> Option<CString> {
    api::document_origin(js.ptr())
}

pub(crate) fn top_url(js: Js) -> CString {
    api::top_url(js.ptr())
}

pub(crate) fn node_frame_url(node: Node<'_>) -> Option<CString> {
    api::node_frame_url(node)
}

pub(crate) fn node_doc_base(js: Js, node: Node<'_>) -> Option<CString> {
    api::node_doc_base(js.ptr(), Some(node))
}

pub(crate) fn same_origin(a: &CStr, b: &CStr) -> bool {
    southstar_url::bytes::same_origin(a.to_bytes(), b.to_bytes())
}

pub(crate) fn is_http_or_https(url: Option<&CStr>) -> bool {
    url.is_some_and(|url| southstar_url::bytes::is_http_or_https(url.to_bytes()))
}

pub(crate) fn origin_from(url: Option<&CStr>) -> Option<CString> {
    url.and_then(|url| southstar_url::bytes::origin_from(url.to_bytes()))
        .map(c_string)
}

pub(crate) fn referer_for(
    url: Option<&CStr>,
    referrer_url: Option<&CStr>,
    policy: c_int,
) -> Option<CString> {
    southstar_net::referer_for(
        url.map(CStr::to_bytes),
        referrer_url.map(CStr::to_bytes),
        policy,
    )
    .and_then(|referer| CString::new(referer).ok())
}

#[derive(Clone, Copy)]
pub(crate) enum FrameKind {
    Frame,
    Object,
}

pub(crate) fn embedder_csp_allows(
    js: Js,
    iframe: Node<'_>,
    kind: FrameKind,
    url: &CStr,
    page: &CStr,
) -> bool {
    let kind = match kind {
        FrameKind::Frame => southstar_csp::FRAME,
        FrameKind::Object => southstar_csp::OBJECT,
    };
    api::with_node_csp(js.ptr(), Some(iframe), |csp| {
        csp.is_none_or(|csp| csp.allows(kind, url, Some(page), None, true))
    })
}

pub(crate) fn page_csp_allows_script(
    js: Js,
    url: &CStr,
    document: &CStr,
    nonce: Option<&[u8]>,
) -> bool {
    api::with_page_csp(js.ptr(), |csp| {
        csp.is_none_or(|csp| csp.allows(southstar_csp::SCRIPT, url, Some(document), nonce, true))
    })
}

pub(crate) fn page_csp_allows_inline_script(js: Js, text: &[u8], nonce: Option<&[u8]>) -> bool {
    api::with_page_csp(js.ptr(), |csp| {
        csp.is_none_or(|csp| csp.inline_script_allowed(text, nonce))
    })
}

pub(crate) fn csp_header_allows_framing(header: &CStr, embedder: &CStr, framed: &CStr) -> bool {
    southstar_csp::Csp::parse(header.to_bytes()).is_none_or(|csp| {
        csp.allows(
            southstar_csp::FRAME_ANCESTORS,
            embedder,
            Some(framed),
            None,
            true,
        )
    })
}

fn trimmed_at_nul(line: &[u8]) -> CString {
    let end = line.iter().position(|&b| b == 0).unwrap_or(line.len());
    CString::new(&line[..end]).unwrap_or_default()
}

pub(crate) fn log_enabled(js: Js) -> bool {
    api::log_enabled(js.ptr())
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    api::log_line(js.ptr(), &trimmed_at_nul(line));
}

pub(crate) fn start_frame_clock(js: Js, frame: Node<'_>) {
    southstar_js_perf::start_frame_clock(js.ptr(), frame);
}

pub(crate) fn fetch_resource(
    js: Js,
    url: &CStr,
    top_url: &CStr,
    header: &CStr,
    form_body: Option<&[u8]>,
) -> (Option<Response>, Option<CString>) {
    let upload = form_body.map(|body| Upload {
        method: c"POST",
        body,
        content_type: Some(c"application/x-www-form-urlencoded"),
    });
    let fetched = api::fetch_resource(js.ptr(), url, Some(top_url), &[header], upload);
    (fetched.response, fetched.error)
}

pub(crate) fn fetch_script(
    js: Js,
    script: Node<'_>,
    url: &CStr,
    top_url: &CStr,
) -> Option<Response> {
    let info = element_perf_info(js, script);
    let fetched = api::fetch_subresource(js.ptr(), url, Some(top_url), &[], Some(c"script"), &info);
    fetched.response
}

pub(crate) fn element_perf_info(js: Js, element: Node<'_>) -> ResourceInfo {
    southstar_js_loader::element_perf_info(js.ptr(), Some(element))
}

pub(crate) fn add_resource_timing(
    js: Js,
    info: &ResourceInfo,
    url: &CStr,
    initiator: &CStr,
    span: (i64, i64),
    resp: Option<&Response>,
) {
    southstar_js_perf::add_resource_timed(js.ptr(), Some(info), url, Some(initiator), span, resp);
}

pub(crate) fn header_is_nosniff(value: Option<&CStr>) -> bool {
    value.is_some_and(|v| southstar_net::header_is_nosniff(v.to_bytes()))
}

pub(crate) fn content_type_is_javascript(content_type: Option<&CStr>) -> bool {
    content_type.is_some_and(|ct| southstar_js_loader::content_type_is_javascript(ct.to_bytes()))
}

pub(crate) fn image_document(url: &CStr) -> Option<CString> {
    Some(c_string(southstar_html_util::image_document(
        url.to_bytes(),
    )))
}

pub(crate) fn decode_body(body: &[u8], content_type: Option<&CStr>) -> Vec<u8> {
    southstar_html_util::decode_body(body, content_type.map(CStr::to_bytes)).0
}

pub(crate) fn declared_charset(body: &[u8], content_type: Option<&CStr>) -> Option<CString> {
    southstar_html_util::declared_charset(Some(body), content_type.map(CStr::to_bytes))
        .map(|name| c_string(name.as_bytes().to_vec()))
}

pub(crate) fn html_parse(input: &[u8]) -> Tree {
    southstar_html_parser::parse_document(input, true)
}

pub(crate) fn xml_parse(input: &[u8]) -> Option<Tree> {
    southstar_xml::parse_document(input).ok()
}

pub(crate) fn xml_well_formed(input: &[u8]) -> (bool, Option<CString>) {
    let (well_formed, root_ns) = southstar_html_parser::xml_well_formed(input);
    (well_formed, root_ns.filter(|_| well_formed).map(c_string))
}

pub(crate) fn new_text(text: &[u8]) -> Node<'static> {
    southstar_dom::node::new_text(southstar_dom::Kind::Text, text)
}

pub(crate) fn mark_attr_dirty(node: Node<'_>, name: &CStr) {
    southstar_css::mark_attr_dirty(node, Some(name.to_bytes()), None);
}

pub(crate) fn dispatch_resource_event(js: Js, target: Node<'_>, kind: &CStr) {
    southstar_js_runtime::events::dispatch_resource_event(
        js.ptr(),
        target,
        &kind.to_string_lossy(),
    );
}

pub(crate) fn dispatch_event(js: Js, target: Node<'_>, kind: &CStr) {
    southstar_js_runtime::events::dispatch_event(js.ptr(), target, &kind.to_string_lossy());
}

pub(crate) fn fire_page_transition(js: Js, kind: &CStr) {
    (api::window_hooks().fire_page_transition)(js.ptr(), kind, false);
}

pub(crate) fn mark_scripts_already_started(root: Node<'_>) {
    southstar_js_loader::mark_scripts_already_started(root);
}

pub(crate) fn script_tasks(root: Node<'_>) -> Vec<Node<'static>> {
    southstar_js_loader::collect_script_tasks(root).nodes()
}

pub(crate) fn run_page_realm_scripts(js: Js, root: Node<'_>, origin: &CStr) {
    let tasks = southstar_js_loader::collect_script_tasks(root);
    let origin = Some(origin);
    southstar_js_loader::run_parser_blocking_scripts(js.ptr(), &tasks, origin);
    southstar_js_loader::run_script_schedule(
        js.ptr(),
        &tasks,
        southstar_js_loader::Schedule::Deferred,
        origin,
    );
    southstar_js_loader::run_script_schedule(
        js.ptr(),
        &tasks,
        southstar_js_loader::Schedule::Async,
        origin,
    );
}

pub(crate) fn schedule_pending_script_drain(js: Js) {
    southstar_js_loader::schedule_pending_script_drain(js.ptr());
}

pub(crate) fn script_type_supported(script: Node<'_>) -> bool {
    southstar_js_loader::script_type_supported(script)
}

pub(crate) fn script_skipped_by_nomodule(script: Node<'_>) -> bool {
    southstar_js_loader::script_skipped_by_nomodule(script)
}

pub(crate) fn script_type_is_module(script: Node<'_>) -> bool {
    southstar_js_loader::script_type_is_module(script)
}

pub(crate) fn deny_cookie(ctx: Ctx, doc: &Value) {
    ctx.try_enter(|scope| nsr_realmdoc_deny_cookie(scope, doc));
}

pub(crate) fn link_outward(js: Js, outward: &Value, realm_window: &Value) {
    (api::window_hooks().link_outward)(js.ptr(), outward, realm_window);
}

pub(crate) fn eval_budget_us() -> i64 {
    api::eval_budget_us()
}

pub(crate) fn in_frame_scope<R>(js: Js, realm: Ctx, frame: Node<'_>, f: impl FnOnce() -> R) -> R {
    api::in_frame_scope(js.ptr(), realm.handle(), frame, f)
}

pub(crate) fn during_frame_load<R>(
    js: Js,
    frame: Node<'_>,
    doc: Node<'_>,
    url: &CStr,
    realm_doc: &Value,
    f: impl FnOnce() -> R,
) -> R {
    api::during_frame_load(js.ptr(), frame, doc, url, realm_doc, f)
}

pub(crate) fn evaluating<R>(
    js: Js,
    script: Option<Node<'_>>,
    deadline_us: i64,
    f: impl FnOnce() -> R,
) -> R {
    api::frame_eval(js.ptr(), script, deadline_us, f)
}

pub(crate) fn purge_frame_callbacks(js: Js, frame: Node<'_>) {
    (api::lifecycle_hooks().purge_frame_rafs)(js.ptr(), frame);
    southstar_js_services::purge_frame_timers(js.ptr(), frame);
}

pub(crate) fn orphan_document(js: Js, doc: Node<'_>) {
    doc.detach();
    api::unpin_subtree(js.ptr(), doc);
    southstar_js_mutation::track_orphan(js.ptr(), doc);
}

pub(crate) fn sweep_orphans(js: Js) {
    api::sweep_orphans(js.ptr());
}
