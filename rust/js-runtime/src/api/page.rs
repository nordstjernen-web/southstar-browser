//! Southstar — the page record's state for binding crates: its documents, URLs and realms, the scheduling depths and flags, user activation and the page's CSP, cookies and timing.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::Node;
use southstar_glib::{self as glib, GHashTable};
use southstar_js_engine::ContextHandle;

use crate::budget;
use crate::ffi::state::PageState;
use crate::ffi::{self, Js, NsJs, glue, record, scopes, wrapper};
use crate::scheduling;

const TRANSIENT_ACTIVATION_US: i64 = 5_000_000;

fn owned(text: *const c_char) -> Option<CString> {
    glue::text(text).map(CStr::to_owned)
}

fn state<R>(js: *const NsJs, fallback: R, f: impl FnOnce(&PageState) -> R) -> R {
    Js::of(js).map_or(fallback, |js| f(js.st()))
}

fn add(js: *const NsJs, depth: impl FnOnce(&PageState) -> &Cell<c_int>, delta: i32) -> i32 {
    state(js, 0, |state| scheduling::add_depth(depth(state), delta))
}

pub fn active() -> *mut NsJs {
    record::active()
}

pub fn halted(js: *const NsJs) -> bool {
    Js::of(js).is_some_and(Js::halted)
}

pub fn halt(js: *const NsJs) {
    if let Some(js) = Js::of(js) {
        js.halt();
    }
}

pub fn in_pump(js: *const NsJs) -> bool {
    state(js, false, |state| state.in_pump.get())
}

pub fn is_worker(js: *const NsJs) -> bool {
    state(js, false, |state| !state.worker_host.get().is_null())
}

pub fn worker_host(js: *const NsJs) -> *mut c_void {
    state(js, ptr::null_mut(), |state| state.worker_host.get().cast())
}

pub fn log_enabled(js: *const NsJs) -> bool {
    state(js, false, |state| state.log.is_set())
}

pub fn log_line(js: *const NsJs, line: &CStr) {
    if let Some(js) = Js::of(js) {
        glue::log_line(js, line.as_ptr());
    }
}

pub fn main_context(js: *const NsJs) -> ContextHandle {
    Js::of(js).map_or(ContextHandle::NULL, Js::ctx)
}

pub fn main_realm(js: *const NsJs) -> ContextHandle {
    Js::of(js).map_or(ContextHandle::NULL, Js::main_realm)
}

pub fn pattern_context() -> ContextHandle {
    glue::active_js().map_or(ContextHandle::NULL, Js::main_or_current)
}

pub fn module_context(js: *const NsJs) -> ContextHandle {
    Js::of(js).map_or(ContextHandle::NULL, |js| {
        js.st().module_ctx.get().or(js.ctx())
    })
}

pub fn swap_module_context(js: *const NsJs, ctx: ContextHandle) -> ContextHandle {
    state(js, ContextHandle::NULL, |state| {
        state.module_ctx.replace(ctx)
    })
}

pub fn set_realm(js: *const NsJs, ctx: ContextHandle, doc: Option<Node<'_>>) {
    if let Some(js) = Js::of(js) {
        js.set_ctx(ctx);
        js.set_current_doc(Node::ptr_or_null(doc).cast_mut());
    }
}

pub fn current_document(js: *const NsJs) -> Option<Node<'static>> {
    record::node(Js::of(js)?.current_doc())
}

pub fn swap_current_document(js: *const NsJs, doc: Option<Node<'_>>) -> Option<Node<'static>> {
    let js = Js::of(js)?;
    let previous = js.current_doc();
    js.set_current_doc(Node::ptr_or_null(doc).cast_mut());
    record::node(previous)
}

pub fn current_script(js: *const NsJs) -> Option<Node<'static>> {
    record::node(Js::of(js)?.st().current_script.get())
}

pub fn swap_current_script(js: *const NsJs, script: Option<Node<'_>>) -> Option<Node<'static>> {
    let previous = Js::of(js)?
        .st()
        .current_script
        .replace(Node::ptr_or_null(script).cast_mut());
    record::node(previous)
}

pub fn ce_main_doc(js: *const NsJs) -> Option<Node<'static>> {
    record::ce_main_doc(js)
}

pub fn iframe_document_node(js: *const NsJs) -> Option<Node<'static>> {
    Js::of(js)?
        .st()
        .iframe_doc
        .borrow()
        .as_ref()
        .and_then(wrapper::unwrap)
}

pub fn current_url(js: *const NsJs) -> CString {
    Js::of(js)
        .and_then(|js| owned(js.current_url_raw()))
        .unwrap_or_default()
}

pub fn page_url(js: *const NsJs) -> Option<CString> {
    owned(page_url_ptr(js))
}

pub fn page_url_ptr(js: *const NsJs) -> *const c_char {
    Js::of(js).map_or(ptr::null(), |js| js.current_url_raw().cast_const())
}

pub fn set_current_url(js: *const NsJs, url: Option<&CStr>) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let copy = url.map_or(ptr::null_mut(), |url| glib::strdup(url.to_bytes()));
    let previous = js.current_url_raw();
    js.set_current_url_raw(copy);
    // SAFETY: the page's URL slot holds null or a string the page allocated with g_malloc, and nothing else frees it.
    unsafe { glib::g_free(previous.cast()) };
}

pub fn top_url(js: *const NsJs) -> CString {
    Js::of(js)
        .and_then(|js| owned(scopes::top_url(js)))
        .unwrap_or_default()
}

pub fn set_top_url(js: *const NsJs, url: Option<&CStr>) {
    if let Some(js) = Js::of(js) {
        scopes::set_top_url(js, url.map_or(ptr::null(), CStr::as_ptr));
    }
}

pub fn document_origin(js: *const NsJs) -> Option<CString> {
    owned(Js::of(js)?.st().document_origin.get())
}

pub fn set_document(js: *const NsJs, doc: Option<Node<'_>>, origin: Option<&CStr>) {
    let Some(js) = Js::of(js) else {
        return;
    };
    let doc = Node::ptr_or_null(doc).cast_mut();
    js.set_current_doc(doc);
    js.set_ce_main_doc(doc);
    let state = js.st();
    state.autofocus_processed.set(false);
    state.active_modal.set(ptr::null());
    let origin = origin.map_or(ptr::null_mut(), |origin| glib::strdup(origin.to_bytes()));
    PageState::replace_owned(&state.document_origin, origin);
}

pub fn realm_url(js: *const NsJs, realm: ContextHandle) -> Option<CString> {
    owned(scopes::realm_url(Js::of(js)?, realm))
}

pub fn realm_document_url_ptr(js: *const NsJs, realm: ContextHandle) -> *const c_char {
    let Some(js) = Js::of(js) else {
        return ptr::null();
    };
    if !realm.is_null() && realm != js.main_realm() {
        let url = record::frame_url(js, scopes::frame_of_realm(js, realm));
        if !url.is_null() {
            return url;
        }
    }
    js.current_url_raw()
}

pub fn node_frame_url(node: Node<'_>) -> Option<CString> {
    record::node_frame_url(node).map(CStr::to_owned)
}

pub fn node_doc_base(js: *const NsJs, node: Option<Node<'_>>) -> Option<CString> {
    node.and_then(node_frame_url).or_else(|| page_url(js))
}

pub fn node_in_page(js: *const NsJs, node: Option<Node<'_>>) -> bool {
    let (Some(js), Some(node)) = (Js::of(js), node) else {
        return false;
    };
    let top = scopes::top_document(js.current_doc());
    !top.is_null() && scopes::top_document(node.as_mut_ptr()) == top
}

pub fn url_parses(js: *const NsJs, url: &CStr) -> bool {
    record::url_parses(js, url)
}

pub fn csp_allows_worker(js: *const NsJs, url: &CStr) -> bool {
    record::csp_allows_worker(js, url)
}

pub fn csp_form_action_allowed(js: *const NsJs, url: Option<&CStr>) -> bool {
    record::csp_form_action_allowed(js, url)
}

pub fn csp_allows_connect(js: *const NsJs, url: &CStr, page: Option<&CStr>) -> bool {
    record::csp_allows_connect(js, url, page)
}

pub fn inline_handlers_allowed(js: *const NsJs) -> bool {
    record::inline_handlers_allowed(js)
}

pub fn page_csp(js: *const NsJs) -> *const c_void {
    record::page_csp(js).cast()
}

pub fn cookie_value(js: *const NsJs) -> Option<CString> {
    record::cookie_value(js)
}

pub fn set_cookie_value(js: *const NsJs, value: Option<&CStr>) {
    record::set_cookie_value(js, value);
}

pub fn partition_key(js: *const NsJs) -> Option<CString> {
    record::partition_key(js)
}

pub fn time_origin_us(js: *const NsJs) -> i64 {
    record::time_origin_us(js)
}

pub fn time_origin_real_ms(js: *const NsJs) -> f64 {
    record::time_origin_real_ms(js)
}

pub fn layout_root(js: *const NsJs) -> *const c_void {
    record::layout_root(js)
}

pub fn style_table(js: *const NsJs) -> *mut GHashTable {
    state(js, ptr::null_mut(), |state| state.style_table.get())
}

pub fn image_cache(js: *const NsJs) -> *mut c_void {
    state(js, ptr::null_mut(), |state| state.image_cache.get())
}

pub fn glib_context(js: *const NsJs) -> *mut c_void {
    state(js, ptr::null_mut(), |state| state.main_context.get().cast())
}

pub fn ready_state(js: *const NsJs) -> i32 {
    state(js, 0, |state| state.ready_state.get())
}

pub fn set_ready_state(js: *const NsJs, ready_state: i32) {
    state(js, (), |state| state.ready_state.set(ready_state));
}

pub fn doc_ready_state(js: *const NsJs, doc: Node<'_>) -> i32 {
    state(js, -1, |state| {
        let states = state.doc_ready_states.borrow();
        states.get(&(doc.as_ptr() as usize)).copied().unwrap_or(-1)
    })
}

pub fn set_doc_ready_state(js: *const NsJs, doc: Node<'_>, ready_state: i32) {
    state(js, (), |state| {
        state
            .doc_ready_states
            .borrow_mut()
            .insert(doc.as_ptr() as usize, ready_state);
    });
}

pub fn clear_doc_ready_states(js: *const NsJs) {
    state(js, (), |state| state.doc_ready_states.borrow_mut().clear());
}

pub fn eval_depth(js: *const NsJs) -> i32 {
    state(js, 0, |state| state.eval_depth.get())
}

pub fn eval_depth_add(js: *const NsJs, delta: i32) -> i32 {
    add(js, |state| &state.eval_depth, delta)
}

pub fn callback_depth(js: *const NsJs) -> i32 {
    state(js, 0, |state| state.callback_depth.get())
}

pub fn callback_depth_add(js: *const NsJs, delta: i32) {
    add(js, |state| &state.callback_depth, delta);
}

pub fn dispatch_depth(js: *const NsJs) -> i32 {
    state(js, 0, |state| state.dispatch_depth.get())
}

pub fn dispatch_depth_add(js: *const NsJs, delta: i32) {
    add(js, |state| &state.dispatch_depth, delta);
}

pub fn ignore_destructive_writes(js: *const NsJs) -> i32 {
    state(js, 0, |state| state.ignore_destructive_writes.get())
}

pub fn ignore_destructive_writes_add(js: *const NsJs, delta: i32) {
    add(js, |state| &state.ignore_destructive_writes, delta);
}

pub fn iframe_load_depth(js: *const NsJs) -> i32 {
    state(js, 0, |state| state.iframe_load_depth.get())
}

pub fn in_frame_load(js: *const NsJs) -> bool {
    iframe_load_depth(js) > 0
}

pub fn mutated(js: *const NsJs) -> bool {
    state(js, false, |state| state.mutated.get())
}

pub fn mark_mutated(js: *const NsJs) {
    state(js, (), |state| state.mutated.set(true));
}

pub fn consume_mutated(js: *const NsJs) -> bool {
    state(js, false, |state| state.mutated.replace(false))
}

pub fn autofocus_processed(js: *const NsJs) -> bool {
    state(js, false, |state| state.autofocus_processed.get())
}

pub fn set_autofocus_processed(js: *const NsJs) {
    state(js, (), |state| state.autofocus_processed.set(true));
}

pub fn active_modal(js: *const NsJs) -> Option<Node<'static>> {
    record::node(state(js, ptr::null(), |state| state.active_modal.get()))
}

pub fn set_active_modal(js: *const NsJs, modal: Option<Node<'_>>) {
    let modal = Node::ptr_or_null(modal);
    state(js, (), |state| {
        state.active_modal.set(modal);
        glue::set_active_modal(modal);
    });
}

pub fn note_user_activation(js: *const NsJs) {
    state(js, (), |state| {
        state.user_activation_us.set(ffi::monotonic_us());
        state.user_ever_activated.set(true);
    });
}

pub fn has_transient_activation(js: *const NsJs) -> bool {
    state(js, false, |state| {
        let at = state.user_activation_us.get();
        at != 0 && ffi::monotonic_us() - at <= TRANSIENT_ACTIVATION_US
    })
}

pub fn user_activation_state(js: *const NsJs) -> (bool, bool) {
    let ever = state(js, false, |state| state.user_ever_activated.get());
    (has_transient_activation(js), ever)
}

pub fn consume_user_activation(js: *const NsJs) {
    state(js, (), |state| state.user_activation_us.set(0));
}

pub fn events_suspended(js: *const NsJs) -> bool {
    Js::of(js).is_some_and(|js| js.halted() || js.st().in_pump.get())
}

pub fn window_events_blocked(js: *const NsJs) -> bool {
    Js::of(js).is_none_or(|js| js.current_doc().is_null() || js.halted() || js.st().in_pump.get())
}

pub fn wpt_hooks_enabled(js: *const NsJs) -> bool {
    state(js, false, |state| !state.early_inject_src.get().is_null())
}

pub fn early_inject_src(js: *const NsJs) -> Option<CString> {
    owned(state(js, ptr::null_mut(), |state| {
        state.early_inject_src.get()
    }))
}

pub fn message_tasks_pending(js: *const NsJs) -> bool {
    state(js, false, |state| !state.message_tasks.borrow().is_empty())
}

pub fn observer_tick_pending(js: *const NsJs) -> bool {
    state(js, false, |state| state.observer_tick_source.get() != 0)
}

pub fn eval_budget_us() -> i64 {
    budget::eval_budget_us()
}

pub fn wrapper_pinned(js: *const NsJs, node: Node<'_>) -> bool {
    record::wrapper_pinned(js, node)
}

pub fn context_frame(js: *const NsJs, ctx: ContextHandle) -> Option<Node<'static>> {
    let js = Js::of(js)?;
    if ctx.is_null() || ctx == js.main_realm() {
        return None;
    }
    let frame = scopes::frame_of_realm(js, ctx);
    record::node(if frame.is_null() { js.frame() } else { frame })
}

pub fn forget_frame_context(js: *const NsJs, frame: Node<'_>) {
    if let Some(js) = Js::of(js)
        && js.frame().cast_const() == frame.as_ptr()
    {
        js.set_frame(ptr::null_mut());
    }
}

pub fn image_load_realm(js: *const NsJs) -> ContextHandle {
    record::image_load_realm(js)
}

pub fn frame_context_count(js: *const NsJs) -> u32 {
    record::frame_context_count(js)
}

pub fn free_frame_contexts(js: *const NsJs) {
    record::free_frame_contexts(js);
}

pub fn new_frame_context(js: *const NsJs) -> ContextHandle {
    record::new_frame_context(js)
}

pub fn platform_global_names(js: *const NsJs) -> Vec<Vec<u8>> {
    record::platform_global_names(js)
}

pub fn snapshot_platform_globals(js: *const NsJs) {
    record::snapshot_platform_globals(js);
}
