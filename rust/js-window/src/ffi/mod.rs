//! Southstar — the window bindings' calls into the realm record, the DOM, the URL helpers and the other binding crates, and the entry points they export.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::GBoolean;
use southstar_js_engine::{ContextHandle, Scope, Value};
use southstar_js_runtime::api;

use crate::{context, history, location, message, named, navigation, page, set};

mod imports;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Realm(ContextHandle);

impl Realm {
    pub fn of(scope: &Scope<'_>) -> Realm {
        Realm(scope.handle())
    }

    fn from_handle(handle: ContextHandle) -> Option<Realm> {
        handle.non_null().map(Realm)
    }

    pub fn enter<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
        self.0.with(f)
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Frame(Node<'static>);

pub(crate) struct UrlToken(Option<api::UrlScope>);

pub(crate) fn main_realm(js: Js) -> Option<Realm> {
    if js.is_null() {
        return None;
    }
    Realm::from_handle(api::main_realm(js.ptr()))
}

pub(crate) fn current_realm(js: Js) -> Option<Realm> {
    if js.is_null() {
        return None;
    }
    Realm::from_handle(api::main_context(js.ptr()))
}

pub(crate) fn frame_node(js: Js, window: &Value) -> Option<Frame> {
    southstar_js_frames::frame_of_window(js.ptr(), window).map(Frame)
}

pub(crate) fn frame_origin_is_opaque(frame: Frame) -> bool {
    southstar_js_frames::iframe_origin_is_opaque(frame.0)
}

pub(crate) fn document_origin(js: Js) -> Option<Vec<u8>> {
    api::document_origin(js.ptr()).map(CString::into_bytes)
}

pub(crate) fn frame_url(js: Js, frame: Frame) -> Option<Vec<u8>> {
    southstar_js_frames::frame_url(js.ptr(), frame.0)
}

pub(crate) fn frame_realm(js: Js, frame: Frame) -> Option<Realm> {
    Realm::from_handle(southstar_js_frames::frame_context(js.ptr(), frame.0))
}

pub(crate) fn structured_clone_transfer(
    scope: &mut Scope<'_>,
    value: &Value,
    transfer: &Value,
    seed_from: &Value,
    seed_to: &Value,
) -> Result<Value, Value> {
    imports::nsr_structured_clone_transfer(scope, value, transfer, seed_from, seed_to)
}

pub(crate) fn realm_now_ms(realm: Realm) -> f64 {
    southstar_js_perf::context_now_ms(realm.0)
}

pub(crate) fn caller_realm(scope: &mut Scope<'_>) -> Realm {
    Realm::from_handle(scope.calling_frame_handle()).unwrap_or_else(|| Realm::of(scope))
}

pub(crate) fn function_realm(scope: &mut Scope<'_>, function: &Value) -> Realm {
    scope
        .function_handle(function)
        .ok()
        .and_then(Realm::from_handle)
        .unwrap_or_else(|| Realm::of(scope))
}

pub(crate) fn budget_enter(js: Js) -> i64 {
    api::budget_enter(js.ptr())
}

pub(crate) fn budget_leave(js: Js, saved: i64) {
    api::budget_leave(js.ptr(), saved);
}

pub(crate) fn realm_url_enter(js: Js, realm: Realm) -> UrlToken {
    UrlToken(api::enter_realm_url(js.ptr(), realm.0))
}

pub(crate) fn realm_url_leave(token: UrlToken) {
    if let Some(scope) = token.0 {
        scope.leave();
    }
}

pub(crate) fn dispatch_main_window_event(js: Js, kind: &str, event: Value) {
    let saved_ctx = api::main_context(js.ptr());
    let saved_doc = current_document(js);
    let main_ctx = main_realm(js).map_or(saved_ctx, |realm| realm.0);
    let main_doc = crate::window_events::top_document(saved_doc);
    api::set_realm(js.ptr(), main_ctx, main_doc);
    unsafe {
        imports::nsr_js_dispatch_window_only_event(
            js.ptr(),
            Node::ptr_or_null(main_doc),
            kind,
            event,
        )
    };
    api::set_realm(js.ptr(), saved_ctx, saved_doc);
}

pub(crate) fn port_transfer_prepare(
    scope: &mut Scope<'_>,
    transfer: &Value,
    realm: Realm,
) -> Result<(Value, Value), Value> {
    imports::nsr_port_transfer_prepare(scope, transfer, &Value::undefined(), realm.0)
}

pub(crate) fn port_transfer_commit(scope: &mut Scope<'_>, old_ports: &Value, new_ports: &Value) {
    imports::nsr_port_transfer_commit(scope, old_ports, new_ports);
}

pub(crate) fn cross_origin_window(scope: &mut Scope<'_>, window: &Value) -> Value {
    southstar_js_frames::iframe_cross_origin_window(scope, window)
}

fn deliver_task(scope: &mut Scope<'_>, args: &[Value]) -> Result<Value, Value> {
    if let [target, event, ..] = args {
        message::deliver(scope, target, event);
    }
    Ok(Value::undefined())
}

pub(crate) fn queue_delivery(scope: &mut Scope<'_>, target: &Value, event: &Value) {
    let _ = api::queue_message_task(scope, deliver_task, &[target.clone(), event.clone()]);
}

#[unsafe(no_mangle)]
pub fn nsr_window_bind_post_message(scope: &mut Scope<'_>, global: &Value) {
    message::bind_post_message(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_window_make_post_message(scope: &mut Scope<'_>, window: &Value) -> Value {
    message::make_post_message(scope, window)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_links_clear(js: *const NsJs, _destroy: GBoolean) {
    clear_window_links(js);
}

pub fn clear_window_links(js: *const NsJs) {
    message::links_clear(Js::of(js));
}

pub fn with_named_access_suspended<R>(f: impl FnOnce() -> R) -> R {
    named::suspend();
    let result = f();
    named::resume();
    result
}

pub fn anchor_fragment_navigate(js: *const NsJs, abs_url: &[u8]) -> bool {
    page::anchor_fragment_navigate(Js::of(js), abs_url)
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_window_link_outward(js: *const NsJs, outward: &Value, realm_window: &Value) {
    message::link_outward(Js::of(js), outward, realm_window);
}

pub(crate) use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Js(usize);

impl Js {
    fn of(js: *const NsJs) -> Js {
        Js(js as usize)
    }

    pub fn is_null(self) -> bool {
        self.0 == 0
    }

    fn ptr(self) -> *mut NsJs {
        self.0 as *mut NsJs
    }
}

pub(crate) struct UrlOrigin {
    pub protocol: Vec<u8>,
    pub hostname: Vec<u8>,
    pub port: Vec<u8>,
}

unsafe extern "C" {
    fn g_utf8_casefold(text: *const c_char, len: isize) -> *mut c_char;
}

fn c_string(bytes: &[u8]) -> CString {
    CString::new(bytes).unwrap_or_default()
}

fn c_text(bytes: &[u8]) -> &[u8] {
    if bytes.contains(&0) { &[] } else { bytes }
}

unsafe fn borrowed(p: *const c_char) -> Vec<u8> {
    if p.is_null() {
        Vec::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_bytes().to_vec()
    }
}

unsafe fn taken(p: *mut c_char) -> Option<Vec<u8>> {
    if p.is_null() {
        return None;
    }
    let bytes = unsafe { borrowed(p) };
    unsafe { southstar_glib::g_free(p.cast()) };
    Some(bytes)
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque() as usize)
}

pub(crate) fn with_main_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    api::main_context(js.ptr()).with(f)
}

pub(crate) fn current_url(js: Js) -> Vec<u8> {
    api::current_url(js.ptr()).into_bytes()
}

pub(crate) fn set_current_url(js: Js, url: &[u8]) {
    api::set_current_url(js.ptr(), Some(&c_string(url)));
}

pub(crate) fn soft_navigate(js: Js, url: &[u8], replace: bool) {
    api::soft_navigate(js.ptr(), &c_string(url), replace);
}

pub(crate) fn navigate(js: Js, url: &[u8], reload: bool) -> bool {
    api::navigate(js.ptr(), &c_string(url), reload)
}

pub(crate) fn window_events_blocked(js: Js) -> bool {
    api::window_events_blocked(js.ptr())
}

pub(crate) fn dispatch_document_window_event(js: Js, kind: &str, event: Value) {
    api::dispatch_document_window_event(js.ptr(), kind, event);
}

pub(crate) fn structured_clone(scope: &mut Scope<'_>, value: &Value) -> Result<Value, Value> {
    imports::nsr_window_structured_clone(scope, &Value::undefined(), core::slice::from_ref(value))
}

pub(crate) fn target_make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    imports::nsr_target_make_event(scope, target, kind)
}

pub(crate) fn target_dispatch(scope: &mut Scope<'_>, target: &Value, kind: &str, event: &Value) {
    imports::nsr_target_dispatch_with_event(scope, target, kind, event);
}

pub(crate) fn bind_event_target_listeners(scope: &mut Scope<'_>, object: &Value) {
    api::bind_event_target_listeners(scope, object);
}

pub(crate) fn make_window_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    crate::window_events::make_window_event(scope, kind)
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    unsafe { imports::nsr_make_event(scope, kind, core::ptr::null()) }
}

#[unsafe(no_mangle)]
pub fn nsr_make_window_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    crate::window_events::make_window_event(scope, kind)
}

pub(crate) fn url_resolve(base: Option<&[u8]>, href: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::resolve(base.map(c_text), c_text(href))
}

pub(crate) fn url_origin(url: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::origin_from(c_text(url))
}

#[derive(Clone, Copy)]
pub(crate) enum UrlField {
    Protocol,
    Origin,
    Host,
    Hostname,
    Port,
    Pathname,
    Search,
    Hash,
}

pub(crate) fn url_field(url: &[u8], field: UrlField) -> Option<Vec<u8>> {
    let p = southstar_url::bytes::parts(c_text(url))?;
    Some(match field {
        UrlField::Protocol => p.protocol,
        UrlField::Origin => p.origin,
        UrlField::Host => p.host,
        UrlField::Hostname => p.hostname,
        UrlField::Port => p.port,
        UrlField::Pathname => p.pathname,
        UrlField::Search => p.search,
        UrlField::Hash => p.hash,
    })
}

pub(crate) fn url_set_component(href: &[u8], component: &str, value: &[u8]) -> Option<Vec<u8>> {
    southstar_url::bytes::set_component(c_text(href), c_text(component.as_bytes()), value)
}

pub(crate) fn top_url(js: Js) -> Vec<u8> {
    api::top_url(js.ptr()).into_bytes()
}

pub(crate) fn set_top_url(js: Js, url: &[u8]) {
    api::set_top_url(js.ptr(), Some(&c_string(url)));
}

pub(crate) fn url_parses(js: Js, url: &[u8]) -> bool {
    api::url_parses(js.ptr(), &c_string(url))
}

pub(crate) fn in_frame_load(js: Js) -> bool {
    api::in_frame_load(js.ptr())
}

pub(crate) fn can_navigate(js: Js) -> bool {
    api::can_navigate(js.ptr())
}

pub(crate) fn fragment_navigated(js: Js, url: &[u8]) {
    api::fragment_navigated(js.ptr(), &c_string(url));
}

pub(crate) fn dispatch_hashchange(js: Js, old_url: &[u8], new_url: &[u8]) {
    let old_url = c_string(old_url);
    let new_url = c_string(new_url);
    crate::window_events::dispatch_hashchange(js, old_url.as_bytes(), new_url.as_bytes());
}

pub(crate) fn has_main_context(js: Js) -> bool {
    !api::main_context(js.ptr()).is_null()
}

pub(crate) fn events_suspended(js: Js) -> bool {
    api::events_suspended(js.ptr())
}

pub(crate) fn in_main_realm_scope(js: Js, f: impl FnOnce()) {
    api::in_realm(js.ptr(), api::main_realm(js.ptr()), f);
}

pub(crate) fn unwrap(value: &Value) -> Option<Node<'static>> {
    api::unwrap_element(value)
}

pub(crate) fn iframe_document(js: Js) -> Option<Node<'static>> {
    api::iframe_document_node(js.ptr())
}

pub(crate) fn frame_document(frame: Option<Node<'static>>) -> Option<Node<'static>> {
    frame.and_then(southstar_js_frames::iframe_document_node)
}

pub(crate) fn ce_main_document(js: Js) -> Option<Node<'static>> {
    api::ce_main_doc(js.ptr())
}

pub(crate) fn casefold(text: &[u8]) -> Vec<u8> {
    let text = c_string(text);
    let folded = unsafe { g_utf8_casefold(text.as_ptr(), -1) };
    unsafe { taken(folded) }.unwrap_or_default()
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    api::log_line(js.ptr(), &c_string(line));
}

pub(crate) fn has_transient_activation(js: Js) -> bool {
    api::has_transient_activation(js.ptr())
}

pub(crate) fn consume_user_activation(js: Js) {
    api::consume_user_activation(js.ptr());
}

pub(crate) fn url_parts(url: &[u8]) -> Option<UrlOrigin> {
    let p = southstar_url::bytes::parts(c_text(url))?;
    Some(UrlOrigin {
        protocol: p.protocol,
        hostname: p.hostname,
        port: p.port,
    })
}

#[unsafe(no_mangle)]
pub fn nsr_window_install_history(scope: &mut Scope<'_>, global: &Value) {
    let history = history::make_history(scope);
    set(scope, global, "history", history);
    let navigation = navigation::make_navigation(scope);
    set(scope, global, "navigation", navigation);
}

#[unsafe(no_mangle)]
pub fn nsr_window_make_location(scope: &mut Scope<'_>) -> Value {
    location::make_location(scope)
}

#[unsafe(no_mangle)]
pub fn nsr_window_open_method(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    location::open(scope, this, args)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_history_teardown(js: *const NsJs) {
    crate::teardown(Js::of(js));
}

pub(crate) fn current_document_for(scope: &mut Scope<'_>, window: &Value) -> Option<Node<'static>> {
    crate::documents::current_document_for(scope, window)
}

#[unsafe(no_mangle)]
pub fn nsr_window_current_document_for(scope: &mut Scope<'_>, window: &Value) -> *mut NsNode {
    Node::ptr_or_null(crate::documents::current_document_for(scope, window)).cast_mut()
}

#[unsafe(no_mangle)]
pub fn nsr_window_document_for(scope: &mut Scope<'_>, window: &Value) -> *mut NsNode {
    Node::ptr_or_null(crate::documents::document_for(scope, window)).cast_mut()
}

#[unsafe(no_mangle)]
pub fn nsr_window_find(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::documents::find(scope, this, args)
}

pub(crate) fn child_frame_window(
    scope: &mut Scope<'_>,
    doc: Node<'_>,
    index: u32,
    name: Option<&CStr>,
) -> Value {
    southstar_js_frames::child_frame_window(scope, Some(doc), index, name, false)
        .unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn wrap(scope: &mut Scope<'_>, node: Node<'_>) -> Value {
    api::make_element(scope, node.as_ptr()).unwrap_or_else(|_| Value::null())
}

pub(crate) fn window_action(js: Js, action: &str) {
    api::window_action(js.ptr(), &c_string(action.as_bytes()));
}

pub(crate) fn viewport_width() -> f64 {
    southstar_css::viewport().0
}

pub(crate) fn viewport_height() -> f64 {
    southstar_css::viewport().1
}

pub(crate) fn device_pixel_ratio() -> f64 {
    southstar_css_media::device_pixel_ratio()
}

pub(crate) fn screen_size() -> (i32, i32) {
    southstar_js_services::screen_size()
}

#[unsafe(no_mangle)]
pub fn nsr_window_install_browsing_context(scope: &mut Scope<'_>, global: &Value) {
    context::install_browsing_context(scope, global);
}

#[unsafe(no_mangle)]
pub fn nsr_window_install_actions(scope: &mut Scope<'_>, global: &Value) {
    context::install_actions(scope, global);
}

fn sync_metrics(js: Js) {
    if js.is_null() {
        return;
    }
    in_main_realm_scope(js, || {
        with_main_context(js, context::sync_metrics);
    });
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn sync_window_metrics(js: *mut c_void) {
    sync_metrics(Js::of(js.cast()));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_sync_window_metrics(js: *const NsJs) {
    sync_metrics(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_child_frame(
    doc: *const NsNode,
    index: u32,
    name: *const c_char,
) -> *const NsNode {
    let Some(doc) = (unsafe { Node::from_ptr(doc) }) else {
        return core::ptr::null();
    };
    let name = (!name.is_null()).then(|| unsafe { CStr::from_ptr(name) });
    Node::ptr_or_null(named::child_frame(doc, index, name))
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_document_expose_legacy_named(
    scope: &mut Scope<'_>,
    root: *const NsNode,
    document: &Value,
) {
    if let Some(root) = unsafe { Node::from_ptr(root) } {
        named::expose_legacy_named(scope, root, document);
    }
}

unsafe extern "C" {
    fn ns_window_scroll_prop(ctx: ContextHandle, prop: *const c_char) -> f64;
    fn ns_js_queue_scrollend_document(js: *mut NsJs);
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
}

pub(crate) fn halted(js: Js) -> bool {
    api::halted(js.ptr())
}

pub(crate) fn current_document(js: Js) -> Option<Node<'static>> {
    api::current_document(js.ptr())
}

pub(crate) fn window_scroll_prop(scope: &mut Scope<'_>, prop: &CStr) -> f64 {
    unsafe { ns_window_scroll_prop(scope.handle(), prop.as_ptr()) }
}

pub(crate) fn queue_scrollend_document(js: Js) {
    unsafe { ns_js_queue_scrollend_document(js.ptr()) };
}

pub(crate) fn set_hit_viewport(x: f64, y: f64) {
    southstar_layout_hit::set_hit_viewport(x, y);
}

pub(crate) fn dispatch_node_event(js: Js, target: Node<'_>, kind: &CStr) {
    unsafe {
        ns_js_dispatch_event(
            js.ptr(),
            target.as_ptr(),
            kind.as_ptr(),
            core::ptr::null_mut(),
        )
    };
}

pub(crate) fn dispatch_window_only_event(js: Js, doc: Option<Node<'_>>, kind: &CStr, event: Value) {
    let kind = kind.to_string_lossy();
    unsafe {
        imports::nsr_js_dispatch_window_only_event(js.ptr(), Node::ptr_or_null(doc), &kind, event)
    };
}

pub(crate) fn make_node_event(
    scope: &mut Scope<'_>,
    kind: &CStr,
    target: Option<Node<'_>>,
) -> Value {
    let kind = kind.to_string_lossy();
    unsafe { imports::nsr_make_event(scope, &kind, Node::ptr_or_null(target)) }
}

pub(crate) fn schedule_observer_tick(js: Js) {
    api::observer_schedule_tick(js.ptr());
}

pub(crate) fn reeval_services_media_queries(js: Js) {
    southstar_js_services::reeval_media_queries(js.ptr());
}

pub(crate) fn url_is_http_or_https(url: &[u8]) -> bool {
    southstar_url::bytes::is_http_or_https(c_text(url))
}

pub(crate) fn url_same_origin(a: &[u8], b: &[u8]) -> bool {
    southstar_url::bytes::same_origin(c_text(a), c_text(b))
}

pub(crate) fn cors_allows(doc_url: Option<&[u8]>, url: &[u8], allow: &[u8]) -> bool {
    southstar_js_net::cors_allows(doc_url, Some(url), Some(allow))
}

fn caller_document_url(js: Js, ctx: ContextHandle) -> Option<Vec<u8>> {
    let caller = ctx
        .with(|scope| scope.caller_handle())
        .unwrap_or(ContextHandle::NULL);
    let frame = southstar_js_frames::frame_of_realm(js.ptr(), caller);
    if let Some(url) = frame.and_then(|frame| frame.attr(c"data-nd-frame-url"))
        && !url.is_empty()
    {
        return Some(url.to_bytes().to_vec());
    }
    api::document_origin(js.ptr())
        .or_else(|| api::page_url(js.ptr()))
        .map(CString::into_bytes)
}

fn opt_bytes<'a>(p: *const c_char) -> Option<&'a [u8]> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_bytes())
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn note_viewport_scroll(js: *mut c_void, x: f64, y: f64) {
    page::note_viewport_scroll(Js::of(js.cast()), x, y);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_note_viewport_scroll(js: *mut NsJs, x: f64, y: f64) {
    page::note_viewport_scroll(Js::of(js), x, y);
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn dispatch_resize(js: *mut c_void) {
    if !js.is_null() {
        page::dispatch_resize(Js::of(js.cast()));
    }
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn reeval_media_queries(js: *mut c_void) {
    reeval_services_media_queries(Js::of(js.cast()));
}

/// # Safety
///
/// `js` is null or a live script engine.
pub unsafe fn fire_page_transition(js: *mut c_void, kind: &CStr, persisted: bool) {
    if !js.is_null() {
        page::fire_page_transition(Js::of(js.cast()), kind, persisted);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_fire_page_transition(
    js: *mut NsJs,
    kind: *const c_char,
    persisted: GBoolean,
) {
    if js.is_null() || kind.is_null() {
        return;
    }
    let kind = unsafe { CStr::from_ptr(kind) };
    page::fire_page_transition(Js::of(js), kind, persisted != 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_resource_origin_clean(
    js: *mut NsJs,
    ctx: ContextHandle,
    url: *const c_char,
    cors_allow_origin: *const c_char,
) -> GBoolean {
    let Some(url) = opt_bytes(url) else {
        return 0;
    };
    if js.is_null() {
        return 0;
    }
    let js = Js::of(js);
    let caller = if url.starts_with(b"data:") || url.starts_with(b"blob:") {
        None
    } else {
        caller_document_url(js, ctx)
    };
    southstar_glib::boolean(page::resource_origin_clean(
        caller,
        url,
        opt_bytes(cors_allow_origin),
    ))
}

#[unsafe(no_mangle)]
pub fn nsr_js_new_window_named(scope: &mut Scope<'_>) -> Result<Value, Value> {
    crate::class::new_window_named(scope)
}

#[unsafe(no_mangle)]
pub unsafe fn nsr_window_install_state(js: *const NsJs, scope: &mut Scope<'_>, global: &Value) {
    context::install_state(scope, Js::of(js), global);
}

#[unsafe(no_mangle)]
pub fn nsr_is_window_named(value: &Value) -> bool {
    value.is_instance_of_class(&crate::class::WINDOW_NAMED_CLASS)
}
