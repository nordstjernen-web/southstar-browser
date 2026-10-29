//! Southstar — the C ABI and nsr_ entry points of script, module and stylesheet loading, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_long};
use std::ffi::CString;

use southstar_dom::Node;
use southstar_glib::{self as glib, SourceId};
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;

use crate::{blocking, compile, eval, hold, import_map, page, schedule};

pub(crate) use southstar_js_runtime::api::ResourceInfo as PerfInfo;

pub(crate) const CSP_SCRIPT: usize = southstar_csp::SCRIPT;
pub(crate) const CSP_STYLE: usize = southstar_csp::STYLE;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(Option<Page>);

fn c_string(bytes: &[u8]) -> CString {
    CString::new(
        bytes
            .iter()
            .copied()
            .filter(|&b| b != 0)
            .collect::<Vec<u8>>(),
    )
    .unwrap_or_default()
}

pub(crate) fn is_named(n: Node, tag: &CStr) -> bool {
    n.element_name() == Some(tag.to_bytes())
}

pub(crate) fn hidden_child(n: Node) -> bool {
    southstar_dom::serialize::is_embedded_doc(n)
        || southstar_dom::serialize::is_shadow_root_marked(n)
}

pub(crate) fn node_remove(n: Node) {
    n.detach()
}

pub(crate) fn append_child(parent: Node, child: Node) {
    parent.append(child)
}

pub(crate) fn arm_invalidate(n: Option<Node>) {
    if let Some(n) = n {
        api::arm_js_invalidate(n);
    }
}

pub(crate) fn url_resolve(base: Option<&CStr>, href: &CStr) -> Option<CString> {
    let mut resolved = southstar_url::bytes::resolve(base.map(CStr::to_bytes), href.to_bytes())?;
    if let Some(nul) = resolved.iter().position(|&b| b == 0) {
        resolved.truncate(nul);
    }
    Some(CString::new(resolved).unwrap_or_default())
}

pub(crate) fn url_is_http_or_https(url: &CStr) -> bool {
    southstar_url::bytes::is_http_or_https(url.to_bytes())
}

pub(crate) fn decode_data_url(url: &CStr) -> Option<Vec<u8>> {
    southstar_net::decode_data_url(url.to_bytes(), crate::MAX_SCRIPT_BYTES as u64)
}

pub(crate) fn sri_check(integrity: Option<&CStr>, body: &[u8]) -> bool {
    southstar_sandbox::sri_check(integrity.map_or(&b""[..], CStr::to_bytes), Some(body))
}

pub(crate) struct Response(southstar_net::Response);

impl Response {
    pub(crate) fn status(&self) -> c_long {
        self.0.status()
    }

    pub(crate) fn body(&self) -> Option<&[u8]> {
        self.0.body()
    }

    pub(crate) fn error(&self) -> Option<&CStr> {
        self.0.error()
    }

    pub(crate) fn content_type(&self) -> Option<&CStr> {
        self.0.content_type()
    }

    pub(crate) fn final_url(&self) -> Option<&CStr> {
        self.0.final_url().filter(|url| !url.is_empty())
    }

    pub(crate) fn nosniff(&self) -> bool {
        self.0
            .x_content_type_options()
            .is_some_and(|value| southstar_net::header_is_nosniff(value.to_bytes()))
    }
}

pub(crate) struct Fetched {
    pub response: Option<Response>,
    pub error: Option<CString>,
}

impl Js {
    pub(crate) fn of(js: Option<Page>) -> Js {
        Js(js)
    }

    pub(crate) fn key(self) -> usize {
        self.0.map_or(0, Page::addr)
    }

    pub(crate) fn is_null(self) -> bool {
        self.0.is_none()
    }

    pub(crate) fn halted(self) -> bool {
        api::halted(self.0)
    }

    pub(crate) fn in_pump(self) -> bool {
        api::in_pump(self.0)
    }

    pub(crate) fn eval_depth(self) -> i32 {
        api::eval_depth(self.0)
    }

    pub(crate) fn callback_depth(self) -> i32 {
        api::callback_depth(self.0)
    }

    pub(crate) fn dispatch_depth(self) -> i32 {
        api::dispatch_depth(self.0)
    }

    pub(crate) fn current_doc(self) -> Option<Node<'static>> {
        api::current_document(self.0)
    }

    pub(crate) fn log(self, line: &[u8]) {
        api::log_line(self.0, &c_string(line));
    }

    pub(crate) fn mark_mutated(self) {
        api::mark_mutated(self.0);
    }

    pub(crate) fn in_page(self, n: Node) -> bool {
        api::node_in_page(self.0, Some(n))
    }

    pub(crate) fn base_url(self, n: Node) -> Option<CString> {
        (api::document_hooks().node_base_url)(self.0, n).and_then(|url| CString::new(url).ok())
    }

    pub(crate) fn dispatch_resource_event(self, n: Node, ty: &CStr) {
        southstar_js_runtime::events::dispatch_resource_event(self.0, n, &ty.to_string_lossy());
    }

    pub(crate) fn upgrade_all(self) {
        if let Some(doc) = self.current_doc() {
            southstar_js_custom_elements::upgrade_subtree_all(self.0, doc);
        }
    }

    pub(crate) fn ce_upgrading(self) -> bool {
        southstar_js_custom_elements::upgrading(self.0)
    }

    pub(crate) fn drain_microtasks(self) {
        api::drain_microtasks(self.0);
    }

    pub(crate) fn schedule_static_iframes(self, root: Node) {
        (api::frames_hooks().schedule_static_iframes)(self.0, root)
    }

    pub(crate) fn rescan_images(self, root: Node) {
        southstar_js_images::rescan_pending_images(self.0, Some(root));
    }

    pub(crate) fn attach_drain_timer(self) -> SourceId {
        api::attach_timeout_once(self.0, 4, move || {
            if !self.is_null() {
                schedule::drain_timer_fired(self);
            }
        })
    }

    pub(crate) fn source_remove(self, source: SourceId) {
        api::source_remove(self.0, source);
    }

    pub(crate) fn index_child_removed(self, parent: Option<Node>, removed: Node) {
        (api::mutation_hooks().index_child_removed)(self.0, parent, removed);
    }

    pub(crate) fn record_child_added(self, parent: Node, added: Node, prev: Option<Node>) {
        (api::mutation_hooks().record_child_added)(self.0, parent, added, prev);
    }

    pub(crate) fn eval_script_source(
        self,
        script: Node,
        source: &[u8],
        origin: Option<&CStr>,
        kind: crate::eval::ScriptKind,
    ) {
        crate::eval::eval_script_source(self, script, source, origin, kind);
    }

    pub(crate) fn forget_script_roots_in(self, root: Node) {
        page::forget_roots_in(self, root);
    }

    pub(crate) fn has_log(self) -> bool {
        api::log_enabled(self.0)
    }

    pub(crate) fn eval_depth_add(self, delta: i32) -> i32 {
        api::eval_depth_add(self.0, delta)
    }

    pub(crate) fn ignore_destructive_writes_add(self, delta: i32) {
        api::ignore_destructive_writes_add(self.0, delta);
    }

    pub(crate) fn current_script(self) -> Option<Node<'static>> {
        api::current_script(self.0)
    }

    pub(crate) fn swap_current_script(self, script: Option<Node>) -> Option<Node<'static>> {
        api::swap_current_script(self.0, script)
    }

    pub(crate) fn swap_current_doc(self, doc: Option<Node>) -> Option<Node<'static>> {
        api::swap_current_document(self.0, doc)
    }

    pub(crate) fn main_realm(self) -> Realm {
        Realm(api::main_context(self.0))
    }

    pub(crate) fn module_realm(self) -> Realm {
        Realm(api::module_context(self.0))
    }

    pub(crate) fn swap_module_realm(self, realm: Option<Realm>) -> Option<Realm> {
        let ctx = realm.map_or(ContextHandle::default(), |realm| realm.0);
        api::swap_module_context(self.0, ctx).non_null().map(Realm)
    }

    pub(crate) fn node_realm(self, n: Node) -> Option<Realm> {
        self.0
            .map_or(ContextHandle::NULL, |js| {
                (api::frames_hooks().node_realm_context)(js, n)
            })
            .non_null()
            .map(Realm)
    }

    pub(crate) fn iframe_document(self) -> Option<Value> {
        api::iframe_document(self.0)
    }

    pub(crate) fn frame_url_push(self, url: Option<&CStr>) -> FrameUrl {
        FrameUrl(api::enter_frame_url(self.0, url))
    }

    pub(crate) fn frame_url_pop(self, saved: FrameUrl) {
        if let Some(scope) = saved.0 {
            scope.leave();
        }
    }

    pub(crate) fn budget_enter(self) -> i64 {
        api::budget_enter(self.0)
    }

    pub(crate) fn budget_leave(self, saved: i64) {
        api::budget_leave(self.0, saved);
    }

    pub(crate) fn flush_document_write(self) {
        if let Some(js) = self.0 {
            (api::document_hooks().flush_write)(js);
        }
    }

    pub(crate) fn report_uncaught(self, exception: &Value, origin: Option<&CStr>) {
        let origin = origin.map(CStr::to_string_lossy);
        southstar_js_runtime::events::report_uncaught(self.0, exception, origin.as_deref())
    }

    pub(crate) fn content_scripts_for_url(
        self,
        url: Option<&CStr>,
        document_start: bool,
    ) -> Option<CString> {
        let script = self.with_main_scope(|scope| {
            let global = scope.global();
            southstar_extensions::content_scripts_for_url(
                scope,
                &global,
                url.map(CStr::to_bytes),
                document_start,
            )
        })??;
        Some(c_string(&script))
    }

    pub(crate) fn remove_lifecycle_tasks(self, mut doomed: impl FnMut(Option<Node>) -> bool) {
        crate::page::with_existing(self, |page| {
            if let Some(tasks) = page.lifecycle_tasks.as_mut() {
                tasks.retain(|task| !doomed(Some(task.node)));
            }
        });
    }

    pub(crate) fn with_blob<R>(
        self,
        url: &CStr,
        requester: Option<&CStr>,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        let requester = requester.map(CStr::to_bytes);
        southstar_js_url::blob_url_lookup(self.0, url.to_bytes(), requester)
            .map(|(bytes, _)| f(&bytes))
    }

    pub(crate) fn document_url(self, scope: &Scope<'_>) -> Option<CString> {
        api::realm_url(self.0, scope.handle()).or_else(|| api::page_url(self.0))
    }

    pub(crate) fn csp_allows(
        self,
        element: Option<Node>,
        kind: usize,
        url: &CStr,
        origin: Option<&CStr>,
        nonce: Option<&CStr>,
        parser_inserted: bool,
    ) -> bool {
        let nonce = nonce.map(CStr::to_bytes);
        api::with_node_csp(self.0, element, |csp| {
            csp.is_none_or(|csp| csp.allows(kind, url, origin, nonce, parser_inserted))
        })
    }

    pub(crate) fn inline_script_allowed(
        self,
        script: Node,
        text: &[u8],
        nonce: Option<&CStr>,
    ) -> bool {
        let nonce = nonce.map(CStr::to_bytes);
        api::with_node_csp(self.0, Some(script), |csp| {
            csp.is_none_or(|csp| csp.inline_script_allowed(text, nonce))
        })
    }

    pub(crate) fn element_perf_info(self, n: Node) -> PerfInfo {
        let mut info = PerfInfo::default();
        fill_element_perf_info(self.0, Some(n), &mut info);
        info
    }

    pub(crate) fn module_perf_info(self, ctx: ContextHandle) -> PerfInfo {
        PerfInfo {
            timeline: ctx.as_ptr().cast_const(),
            document_url: api::realm_document_url(self.0, ctx),
            cors_mode: glib::TRUE,
            ..PerfInfo::default()
        }
    }

    pub(crate) fn module_top_url(self) -> Option<CString> {
        if self.is_null() {
            return None;
        }
        if !api::is_worker(self.0) {
            return Some(api::current_url(self.0));
        }
        (api::workers_hooks().host_base_url)(self.0)
    }

    pub(crate) fn prefetch_script(self, url: &CStr, top_url: Option<&CStr>) {
        let headers: Vec<Vec<u8>> = script_accept_headers()
            .iter()
            .map(|line| line.to_bytes().to_vec())
            .collect();
        let request = southstar_net::FetchRequest {
            url: url.to_bytes(),
            top_url: top_url.map(CStr::to_bytes),
            method: Some(b"GET"),
            headers: &headers,
            ..southstar_net::FetchRequest::default()
        };
        if let Some(key) = southstar_net::request_key(
            request.url,
            request.top_url,
            request.method,
            request.headers,
        ) {
            southstar_net::preload_expect(&key);
        }
        southstar_net::request_with(&request, None, drop);
    }

    pub(crate) fn fetch(
        self,
        url: &CStr,
        top_url: Option<&CStr>,
        script_accept: bool,
        initiator: &CStr,
        info: &PerfInfo,
    ) -> Fetched {
        let headers = if script_accept {
            script_accept_headers()
        } else {
            Vec::new()
        };
        let fetched = api::fetch_subresource(self.0, url, top_url, &headers, Some(initiator), info);
        Fetched {
            response: fetched.response.map(Response),
            error: fetched.error,
        }
    }

    pub(crate) fn with_main_scope<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        api::main_context(self.0).with(f)
    }
}

fn script_accept_headers() -> Vec<&'static CStr> {
    southstar_net::FetchDest::Script.accept_lines().to_vec()
}

pub(crate) struct FrameUrl(Option<api::UrlScope>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Realm(ContextHandle);

impl Realm {
    pub(crate) fn enter<R: Default>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        self.0.with(f).unwrap_or_default()
    }
}

fn eval_source(js: Option<Page>, source: &[u8], origin: Option<&CStr>) {
    eval::eval(Js(js), source, origin);
}

fn parser_hold_forget(js: Option<Page>, n: Node<'_>) {
    if js.is_some() {
        hold::forget(Js(js), n);
    }
}

fn teardown(js: Option<Page>) {
    if js.is_some() {
        page::teardown(Js(js));
    }
}

fn purge_subtree_script_refs(js: Option<Page>, root: Node<'_>) {
    if js.is_some() {
        eval::purge_subtree_script_refs(Js(js), root);
    }
}

pub fn install_runtime_hooks() {
    api::set_loader_hooks(api::LoaderHooks {
        eval: eval_source,
        script_needs_prepare: crate::script_needs_prepare,
        schedule_pending_script_drain: |js| crate::schedule_pending_script_drain(Some(js)),
        purge_subtree_script_refs: |js, a1| purge_subtree_script_refs(Some(js), a1),
        parser_hold_forget: |js, a1| parser_hold_forget(Some(js), a1),
        teardown: |js| teardown(Some(js)),
        element_perf_info: crate::api::element_perf_info,
    });
    southstar_js_runtime::links::set_js_loader_links(southstar_js_runtime::links::JsLoaderLinks {
        js_module_load: crate::ffi::nsr_js_module_load,
        js_module_normalize: crate::ffi::nsr_js_module_normalize,
        js_module_set_import_meta: crate::ffi::nsr_js_module_set_import_meta,
        js_compile_module_cached: crate::ffi::nsr_js_compile_module_cached,
    });
}

pub(crate) fn monotonic_us() -> i64 {
    southstar_glib::monotonic_us()
}

pub(crate) fn fill_element_perf_info(js: Option<Page>, el: Option<Node>, info: &mut PerfInfo) {
    let frame = el.and_then(blocking::content_frame);
    let frame_ptr = Node::ptr_or_null(frame);
    let has_js = js.is_some();
    let frame_ctx = match (frame, js) {
        (Some(frame), Some(_)) => (api::frames_hooks().frame_context)(js, frame),
        _ => ContextHandle::default(),
    };
    info.timeline = if frame_ctx.is_null() {
        frame_ptr.cast()
    } else {
        frame_ctx.as_ptr().cast_const().cast()
    };
    let frame_url = match js {
        Some(js) if frame.is_some() => (api::frames_hooks().frame_url)(js, frame),
        _ => None,
    };
    info.document_url = if frame_url.is_some() {
        frame_url
    } else if has_js {
        api::page_url(js)
    } else {
        None
    };
    info.render_blocking = glib::boolean(el.is_some_and(blocking::render_blocking));
    info.cors_mode = glib::boolean(el.is_some_and(|el| el.attr(c"crossorigin").is_some()));
}

pub fn nsr_js_compile_module_cached(
    scope: &mut Scope<'_>,
    src: &[u8],
    name: &[u8],
) -> Result<Value, Value> {
    eval::compile_module_cached(scope, src, name)
}

pub fn nsr_js_module_set_import_meta(
    scope: &mut Scope<'_>,
    module: &Value,
    is_main: bool,
) -> Result<(), Value> {
    compile::set_import_meta(scope, module, is_main)
}

pub fn nsr_js_module_normalize(
    scope: &mut Scope<'_>,
    base: &str,
    name: &str,
) -> Result<String, Value> {
    let js = Js(Page::current(scope));
    let base = CString::new(base.replace('\0', "")).unwrap_or_default();
    let base = Some(base.as_c_str()).filter(|base| !base.is_empty());
    let name = CString::new(name.replace('\0', "")).unwrap_or_default();
    let resolved = import_map::resolve(js, name.to_bytes())
        .or_else(|| url_resolve(base, &name).map(CString::into_bytes))
        .unwrap_or_else(|| name.to_bytes().to_vec());
    Ok(String::from_utf8_lossy(&resolved).into_owned())
}

pub fn nsr_js_module_load(
    scope: &mut Scope<'_>,
    name: &str,
    attributes: &Value,
) -> Result<Value, Value> {
    let js = Js(Page::current(scope));
    let json = compile::attributes_type_is(scope, attributes, b"json");
    let name = CString::new(name.replace('\0', "")).unwrap_or_default();
    eval::load_module(js, scope, &name, json)
}
