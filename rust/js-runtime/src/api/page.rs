//! Southstar — the page record's state for binding crates: its documents, URLs and realms, the scheduling depths and flags, user activation and the page's CSP, cookies and timing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_int, c_void};
use core::ptr::{self, NonNull};
use std::ffi::CString;
use std::rc::Rc;

use southstar_anim::AnimHandle;
use southstar_css::StyleMap;
use southstar_dom::Node;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_layout::BoxRef;

use crate::budget;
use crate::ffi::state::PageState;
use crate::ffi::{self, Js, NavigationTiming, NsJs, glue, record, scopes, wrapper};
use crate::{scheduling, worker};

const TRANSIENT_ACTIVATION_US: i64 = 5_000_000;

fn state<R>(js: Option<Page>, fallback: R, f: impl FnOnce(&PageState) -> R) -> R {
    Js::of(js).map_or(fallback, |js| f(js.st()))
}

fn add(js: Option<Page>, depth: impl FnOnce(&PageState) -> &Cell<c_int>, delta: i32) -> i32 {
    state(js, 0, |state| scheduling::add_depth(depth(state), delta))
}

pub fn active() -> Option<Page> {
    record::active()
}

pub fn halted(js: Option<Page>) -> bool {
    Js::of(js).is_some_and(Js::halted)
}

pub fn halt(js: Option<Page>) {
    if let Some(js) = Js::of(js) {
        js.halt();
    }
}

pub fn in_pump(js: Option<Page>) -> bool {
    state(js, false, |state| state.in_pump.get())
}

pub fn is_worker(js: Option<Page>) -> bool {
    state(js, false, |state| !state.worker_host.get().is_null())
}

pub fn worker_host(js: Option<Page>) -> *mut c_void {
    state(js, ptr::null_mut(), |state| state.worker_host.get().cast())
}

pub fn log_enabled(js: Option<Page>) -> bool {
    state(js, false, |state| state.log.is_set())
}

pub fn log_line(js: Option<Page>, line: &CStr) {
    if let Some(js) = Js::of(js) {
        glue::log_line(js, line);
    }
}

pub fn main_context(js: Option<Page>) -> ContextHandle {
    Js::of(js).map_or(ContextHandle::NULL, Js::ctx)
}

pub fn main_realm(js: Option<Page>) -> ContextHandle {
    Js::of(js).map_or(ContextHandle::NULL, Js::main_realm)
}

pub fn pattern_context() -> ContextHandle {
    glue::active_js().map_or(ContextHandle::NULL, Js::main_or_current)
}

pub fn module_context(js: Option<Page>) -> ContextHandle {
    Js::of(js).map_or(ContextHandle::NULL, |js| {
        js.st().module_ctx.get().or(js.ctx())
    })
}

pub fn swap_module_context(js: Option<Page>, ctx: ContextHandle) -> ContextHandle {
    state(js, ContextHandle::NULL, |state| {
        state.module_ctx.replace(ctx)
    })
}

pub fn set_realm(js: Option<Page>, ctx: ContextHandle, doc: Option<Node<'_>>) {
    if let Some(js) = Js::of(js) {
        js.set_ctx(ctx);
        js.set_current_doc(Node::ptr_or_null(doc).cast_mut());
    }
}

pub fn current_document(js: Option<Page>) -> Option<Node<'static>> {
    record::node(Js::of(js)?.current_doc())
}

pub fn swap_current_document(js: Option<Page>, doc: Option<Node<'_>>) -> Option<Node<'static>> {
    let js = Js::of(js)?;
    let previous = js.current_doc();
    js.set_current_doc(Node::ptr_or_null(doc).cast_mut());
    record::node(previous)
}

pub fn current_script(js: Option<Page>) -> Option<Node<'static>> {
    record::node(Js::of(js)?.st().current_script.get())
}

pub fn swap_current_script(js: Option<Page>, script: Option<Node<'_>>) -> Option<Node<'static>> {
    let previous = Js::of(js)?
        .st()
        .current_script
        .replace(Node::ptr_or_null(script).cast_mut());
    record::node(previous)
}

pub fn ce_main_doc(js: Option<Page>) -> Option<Node<'static>> {
    record::ce_main_doc(js)
}

pub fn iframe_document_node(js: Option<Page>) -> Option<Node<'static>> {
    Js::of(js)?
        .st()
        .iframe_doc
        .borrow()
        .as_ref()
        .and_then(wrapper::unwrap)
}

pub fn current_url(js: Option<Page>) -> CString {
    page_url(js).unwrap_or_default()
}

pub fn page_url(js: Option<Page>) -> Option<CString> {
    Js::of(js)?.current_url()
}

pub fn set_current_url(js: Option<Page>, url: Option<&CStr>) {
    if let Some(js) = Js::of(js) {
        js.replace_current_url(url.map(CStr::to_owned));
    }
}

pub fn top_url(js: Option<Page>) -> CString {
    Js::of(js).map(scopes::top_url).unwrap_or_default()
}

pub fn set_top_url(js: Option<Page>, url: Option<&CStr>) {
    if let Some(js) = Js::of(js) {
        scopes::set_top_url(js, url);
    }
}

pub fn document_origin(js: Option<Page>) -> Option<CString> {
    Js::of(js)?.st().document_origin.borrow().clone()
}

pub fn set_document(js: Option<Page>, doc: Option<Node<'_>>, origin: Option<&CStr>) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let doc = Node::ptr_or_null(doc).cast_mut();
    js.set_current_doc(doc);
    js.set_ce_main_doc(doc);
    let state = js.st();
    state.autofocus_processed.set(false);
    state.active_modal.set(ptr::null());
    state.document_origin.replace(origin.map(CStr::to_owned));
}

pub fn realm_url(js: Option<Page>, realm: ContextHandle) -> Option<CString> {
    scopes::realm_url(Js::of(js)?, realm)
}

pub fn realm_document_url(js: Option<Page>, realm: ContextHandle) -> Option<CString> {
    let js = Js::of(js)?;
    if !realm.is_null() && realm != js.main_realm() {
        let url = record::frame_url(js, scopes::frame_of_realm(js, realm));
        if url.is_some() {
            return url;
        }
    }
    js.current_url()
}

pub fn node_frame_url(node: Node<'_>) -> Option<CString> {
    record::node_frame_url(node).map(CStr::to_owned)
}

pub fn node_doc_base(js: Option<Page>, node: Option<Node<'_>>) -> Option<CString> {
    node.and_then(node_frame_url).or_else(|| page_url(js))
}

pub fn node_in_page(js: Option<Page>, node: Option<Node<'_>>) -> bool {
    let (Some(js), Some(node)) = (Js::of(js), node) else {
        return false;
    };
    let top = scopes::top_document(js.current_doc());
    !top.is_null() && scopes::top_document(node.as_mut_ptr()) == top
}

pub fn url_parses(js: Option<Page>, url: &CStr) -> bool {
    record::url_parses(js, url)
}

pub fn csp_allows_worker(js: Option<Page>, url: &CStr) -> bool {
    record::csp_allows_worker(js, url)
}

pub fn csp_form_action_allowed(js: Option<Page>, url: Option<&CStr>) -> bool {
    record::csp_form_action_allowed(js, url)
}

pub fn csp_allows_connect(js: Option<Page>, url: &CStr, page: Option<&CStr>) -> bool {
    record::csp_allows_connect(js, url, page)
}

pub fn inline_handlers_allowed(js: Option<Page>) -> bool {
    record::inline_handlers_allowed(js)
}

pub fn with_page_csp<R>(js: Option<Page>, f: impl FnOnce(Option<&southstar_csp::Csp>) -> R) -> R {
    record::with_page_csp(js, f)
}

pub fn cookie_value(js: Option<Page>) -> Option<CString> {
    record::cookie_value(js)
}

pub fn set_cookie_value(js: Option<Page>, value: Option<&CStr>) {
    record::set_cookie_value(js, value);
}

pub fn cookie_version(js: Option<Page>) -> u64 {
    record::cookie_version(js)
}

pub fn set_cookie_version(js: Option<Page>, version: u64) {
    record::set_cookie_version(js, version);
}

pub fn partition_key(js: Option<Page>) -> Option<CString> {
    record::partition_key(js)
}

pub fn time_origin_us(js: Option<Page>) -> i64 {
    record::time_origin_us(js)
}

pub fn time_origin_real_ms(js: Option<Page>) -> f64 {
    record::time_origin_real_ms(js)
}

pub fn navigation_timing(js: Option<Page>) -> Option<NavigationTiming> {
    record::navigation_timing(js)
}

pub fn set_navigation_timing(js: Option<Page>, timing: NavigationTiming) {
    record::set_navigation_timing(js, timing);
}

pub fn layout_root(js: Option<Page>) -> Option<BoxRef<'static>> {
    record::layout_root(js)
}

pub fn style_table(js: Option<Page>) -> *const StyleMap {
    state(js, ptr::null(), |state| {
        state
            .style_table
            .borrow()
            .as_ref()
            .map_or(ptr::null(), Rc::as_ptr)
    })
}

pub fn image_cache(js: Option<Page>) -> *mut c_void {
    state(js, ptr::null_mut(), |state| state.image_cache.get())
}

pub fn glib_context(js: Option<Page>) -> *mut c_void {
    state(js, ptr::null_mut(), |state| state.main_context.get().cast())
}

pub fn ready_state(js: Option<Page>) -> i32 {
    state(js, 0, |state| state.ready_state.get())
}

pub fn set_ready_state(js: Option<Page>, ready_state: i32) {
    state(js, (), |state| state.ready_state.set(ready_state));
}

pub fn doc_ready_state(js: Option<Page>, doc: Node<'_>) -> i32 {
    state(js, -1, |state| {
        let states = state.doc_ready_states.borrow();
        states.get(&doc.downgrade()).copied().unwrap_or(-1)
    })
}

pub fn set_doc_ready_state(js: Option<Page>, doc: Node<'_>, ready_state: i32) {
    state(js, (), |state| {
        state
            .doc_ready_states
            .borrow_mut()
            .insert(doc.downgrade(), ready_state);
    });
}

pub fn clear_doc_ready_states(js: Option<Page>) {
    state(js, (), |state| state.doc_ready_states.borrow_mut().clear());
}

pub fn eval_depth(js: Option<Page>) -> i32 {
    state(js, 0, |state| state.eval_depth.get())
}

pub fn eval_depth_add(js: Option<Page>, delta: i32) -> i32 {
    add(js, |state| &state.eval_depth, delta)
}

pub fn callback_depth(js: Option<Page>) -> i32 {
    state(js, 0, |state| state.callback_depth.get())
}

pub fn callback_depth_add(js: Option<Page>, delta: i32) {
    add(js, |state| &state.callback_depth, delta);
}

pub fn dispatch_depth(js: Option<Page>) -> i32 {
    state(js, 0, |state| state.dispatch_depth.get())
}

pub fn dispatch_depth_add(js: Option<Page>, delta: i32) {
    add(js, |state| &state.dispatch_depth, delta);
}

pub fn ignore_destructive_writes(js: Option<Page>) -> i32 {
    state(js, 0, |state| state.ignore_destructive_writes.get())
}

pub fn ignore_destructive_writes_add(js: Option<Page>, delta: i32) {
    add(js, |state| &state.ignore_destructive_writes, delta);
}

pub fn iframe_load_depth(js: Option<Page>) -> i32 {
    state(js, 0, |state| state.iframe_load_depth.get())
}

pub fn in_frame_load(js: Option<Page>) -> bool {
    iframe_load_depth(js) > 0
}

pub fn mutated(js: Option<Page>) -> bool {
    state(js, false, |state| state.mutated.get())
}

pub fn mark_mutated(js: Option<Page>) {
    state(js, (), |state| state.mutated.set(true));
}

pub fn consume_mutated(js: Option<Page>) -> bool {
    state(js, false, |state| state.mutated.replace(false))
}

pub fn autofocus_processed(js: Option<Page>) -> bool {
    state(js, false, |state| state.autofocus_processed.get())
}

pub fn set_autofocus_processed(js: Option<Page>) {
    state(js, (), |state| state.autofocus_processed.set(true));
}

pub fn active_modal(js: Option<Page>) -> Option<Node<'static>> {
    record::node(state(js, ptr::null(), |state| state.active_modal.get()))
}

pub fn set_active_modal(js: Option<Page>, modal: Option<Node<'_>>) {
    let modal = Node::ptr_or_null(modal);
    state(js, (), |state| {
        state.active_modal.set(modal);
        glue::set_active_modal(modal);
    });
}

pub fn note_user_activation(js: Option<Page>) {
    state(js, (), |state| {
        state.user_activation_us.set(ffi::monotonic_us());
        state.user_ever_activated.set(true);
    });
}

pub fn has_transient_activation(js: Option<Page>) -> bool {
    state(js, false, |state| {
        let at = state.user_activation_us.get();
        at != 0 && ffi::monotonic_us() - at <= TRANSIENT_ACTIVATION_US
    })
}

pub fn user_activation_state(js: Option<Page>) -> (bool, bool) {
    let ever = state(js, false, |state| state.user_ever_activated.get());
    (has_transient_activation(js), ever)
}

pub fn consume_user_activation(js: Option<Page>) {
    state(js, (), |state| state.user_activation_us.set(0));
}

pub fn events_suspended(js: Option<Page>) -> bool {
    Js::of(js).is_some_and(|js| js.halted() || js.st().in_pump.get())
}

pub fn window_events_blocked(js: Option<Page>) -> bool {
    Js::of(js).is_none_or(|js| js.current_doc().is_null() || js.halted() || js.st().in_pump.get())
}

pub fn wpt_hooks_enabled(js: Option<Page>) -> bool {
    state(js, false, |state| state.early_inject_src.borrow().is_some())
}

pub fn early_inject_src(js: Option<Page>) -> Option<CString> {
    state(js, None, |state| state.early_inject_src.borrow().clone())
}

pub fn message_tasks_pending(js: Option<Page>) -> bool {
    state(js, false, |state| !state.message_tasks.borrow().is_empty())
}

pub fn observer_tick_pending(js: Option<Page>) -> bool {
    state(js, false, |state| state.observer_tick_source.get() != 0)
}

pub fn eval_budget_us() -> i64 {
    budget::eval_budget_us()
}

pub fn wrapper_pinned(js: Option<Page>, node: Node<'_>) -> bool {
    record::wrapper_pinned(js, node)
}

pub fn node_unpinned(js: Option<Page>, node: Node<'_>) -> bool {
    record::node_unpinned(js, node)
}

pub fn context_frame(js: Option<Page>, ctx: ContextHandle) -> Option<Node<'static>> {
    let js = Js::of(js)?;
    if ctx.is_null() || ctx == js.main_realm() {
        return None;
    }
    let frame = scopes::frame_of_realm(js, ctx);
    record::node(if frame.is_null() { js.frame() } else { frame })
}

pub fn forget_frame_context(js: Option<Page>, frame: Node<'_>) {
    if let Some(js) = Js::of(js)
        && js.frame().cast_const() == frame.as_ptr()
    {
        js.set_frame(ptr::null_mut());
    }
}

pub fn image_load_realm(js: Option<Page>) -> ContextHandle {
    record::image_load_realm(js)
}

pub fn frame_context_count(js: Option<Page>) -> u32 {
    record::frame_context_count(js)
}

pub fn free_frame_contexts(js: Option<Page>) {
    record::free_frame_contexts(js);
}

pub fn new_frame_context(js: Option<Page>) -> ContextHandle {
    record::new_frame_context(js)
}

pub fn platform_global_names(js: Option<Page>) -> Vec<Vec<u8>> {
    record::platform_global_names(js)
}

pub fn snapshot_platform_globals(js: Option<Page>) {
    record::snapshot_platform_globals(js);
}

pub struct WorkerOptions<'a> {
    pub host: *mut c_void,
    pub context: *mut c_void,
    pub url: Option<&'a CStr>,
    pub base_url: Option<&'a CStr>,
    pub origin: Option<&'a CStr>,
    pub name: Option<&'a CStr>,
    pub service_worker: bool,
}

pub fn new_worker(options: &WorkerOptions<'_>) -> Option<Page> {
    ffi::new_worker(&ffi::WorkerSetup {
        host: options.host.cast(),
        context: options.context.cast(),
        url: options.url,
        base_url: options.base_url,
        origin: options.origin,
        name: options.name,
        service_worker: options.service_worker,
    })
}

pub fn worker_eval(
    js: Option<Page>,
    source: &CStr,
    url: &CStr,
    module: bool,
) -> Result<(), (bool, Value)> {
    let Some(js) = Js::of(js) else {
        return Err((false, Value::undefined()));
    };
    let len = source.to_bytes().len();
    match js.scope(|scope| worker::eval(scope, source, len, url, module)) {
        None => Err((false, Value::undefined())),
        Some(worker::EvalStatus::Done) => Ok(()),
        Some(worker::EvalStatus::ParseError(thrown)) => Err((true, thrown)),
        Some(worker::EvalStatus::Thrown(thrown)) => Err((false, thrown)),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Page(NonNull<NsJs>);

impl Page {
    pub(crate) fn from_record(record: NonNull<NsJs>) -> Page {
        Page(record)
    }

    pub fn from_ptr(js: *const NsJs) -> Option<Page> {
        record::live_page(js.addr())
    }

    pub fn from_addr(addr: usize) -> Option<Page> {
        record::live_page(addr)
    }

    pub fn of_scope(scope: &Scope<'_>) -> Option<Page> {
        Page::from_ptr(scope.context_opaque().cast())
    }

    pub fn current(scope: &Scope<'_>) -> Option<Page> {
        NonNull::new(scope.context_opaque().cast()).map(Page)
    }

    pub fn of_context(ctx: ContextHandle) -> Option<Page> {
        NonNull::new(ctx.opaque().cast()).map(Page)
    }

    pub fn is_live(self) -> bool {
        record::live_page(self.addr()).is_some()
    }

    pub fn addr(self) -> usize {
        self.0.as_ptr().addr()
    }

    pub fn from_raw(data: *const c_void) -> Option<Page> {
        Page::from_ptr(data.cast())
    }

    pub fn as_ptr(self) -> *const NsJs {
        self.0.as_ptr().cast_const()
    }

    pub fn as_raw(self) -> *mut c_void {
        self.0.as_ptr().cast()
    }
}

pub fn new_page(callbacks: ffi::Callbacks, timing: Option<NavigationTiming>) -> Option<Page> {
    ffi::new_page(callbacks, timing)
}

pub fn free(js: Page) {
    ffi::free(js);
}

pub fn set_style_table(js: Option<Page>, styles: Option<Rc<StyleMap>>) {
    state(js, (), |state| drop(state.style_table.replace(styles)));
}

pub fn set_image_cache(js: Option<Page>, cache: *mut c_void) {
    if let Some(js) = Js::of(js) {
        js.st().image_cache.set(cache);
        glue::blob_registry(js, !cache.is_null());
    }
}

pub fn anim(js: Option<Page>) -> Option<AnimHandle> {
    state(js, None, |state| state.anim.get())
}

pub fn set_anim(js: Option<Page>, anim: Option<AnimHandle>) {
    if let Some(js) = Js::of(js) {
        js.st().anim.set(anim);
        glue::raf_set_host_driven(js);
    }
}

pub fn set_early_inject_src(js: Option<Page>, src: Option<&CStr>) {
    if let Some(js) = Js::of(js) {
        js.st().early_inject_src.replace(src.map(CStr::to_owned));
    }
}
