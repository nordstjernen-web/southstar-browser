//! Southstar — the C ABI of the window bindings as declared in src/js_internal.h, and the js.c, DOM and URL calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::GBoolean;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::{context, history, location, message, named, navigation, page, set};

type JobFunc =
    unsafe extern "C" fn(ctx: *mut JSContext, argc: c_int, argv: *mut JSValue) -> JSValue;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Realm(usize);

impl Realm {
    pub fn of(scope: &Scope<'_>) -> Realm {
        Realm(quickjs::raw_context(scope) as usize)
    }

    fn from_ptr(ctx: *mut JSContext) -> Option<Realm> {
        (!ctx.is_null()).then_some(Realm(ctx as usize))
    }

    fn ptr(self) -> *mut JSContext {
        self.0 as *mut JSContext
    }

    pub fn enter<R>(self, f: impl FnOnce(&mut Scope<'_>) -> R) -> R {
        unsafe { quickjs::with_context(self.ptr(), f) }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Frame(usize);

pub(crate) struct UrlToken(*mut c_void);

unsafe extern "C" {
    fn ns_js_main_realm_context(js: *const NsJs) -> *mut JSContext;
    fn ns_window_frame_node(js: *mut NsJs, window: JSValue) -> *mut NsNode;
    fn ns_iframe_origin_is_opaque(frame: *mut NsNode) -> GBoolean;
    fn ns_js_document_origin(js: *const NsJs) -> *const c_char;
    fn ns_js_frame_url(js: *const NsJs, frame: *const NsNode) -> *const c_char;
    fn ns_js_frame_context(js: *const NsJs, frame: *const NsNode) -> *mut JSContext;
    fn ns_structured_clone_transfer(
        ctx: *mut JSContext,
        value: JSValue,
        transfer: JSValue,
        seed_from: JSValue,
        seed_to: JSValue,
    ) -> JSValue;
    fn ns_perf_realm_now_ms(ctx: *mut JSContext) -> f64;
    fn JS_GetCallerRealm(ctx: *mut JSContext) -> *mut JSContext;
    fn ns_js_budget_enter(js: *mut NsJs) -> i64;
    fn ns_js_budget_leave(js: *mut NsJs, saved: i64);
    fn ns_js_realm_url_enter(js: *mut NsJs, realm: *mut JSContext) -> *mut c_void;
    fn ns_js_realm_url_leave(js: *mut NsJs, token: *mut c_void);
    fn ns_port_transfer_prepare(
        ctx: *mut JSContext,
        transfer: JSValue,
        source_port: JSValue,
        realm: *mut JSContext,
        old_ports: *mut JSValue,
        new_ports: *mut JSValue,
    ) -> c_int;
    fn ns_port_transfer_commit(ctx: *mut JSContext, old_ports: JSValue, new_ports: JSValue);
    fn ns_iframe_cross_origin_window(ctx: *mut JSContext, target: JSValue) -> JSValue;
    fn ns_js_queue_message_task(
        ctx: *mut JSContext,
        func: JobFunc,
        argc: c_int,
        argv: *mut JSValue,
    );
}

pub(crate) fn main_realm(js: Js) -> Option<Realm> {
    if js.is_null() {
        return None;
    }
    Realm::from_ptr(unsafe { ns_js_main_realm_context(js.ptr()) })
}

pub(crate) fn current_realm(js: Js) -> Option<Realm> {
    if js.is_null() {
        return None;
    }
    Realm::from_ptr(unsafe { ns_js_main_context(js.ptr()) })
}

pub(crate) fn frame_node(js: Js, window: &Value) -> Option<Frame> {
    if js.is_null() || !window.is_object() {
        return None;
    }
    let node = unsafe { ns_window_frame_node(js.ptr(), quickjs::raw(window)) };
    (!node.is_null()).then_some(Frame(node as usize))
}

pub(crate) fn frame_origin_is_opaque(frame: Frame) -> bool {
    unsafe { ns_iframe_origin_is_opaque(frame.0 as *mut NsNode) != 0 }
}

pub(crate) fn document_origin(js: Js) -> Option<Vec<u8>> {
    let origin = unsafe { ns_js_document_origin(js.ptr()) };
    (!origin.is_null()).then(|| unsafe { borrowed(origin) })
}

pub(crate) fn frame_url(js: Js, frame: Frame) -> Option<Vec<u8>> {
    let url = unsafe { ns_js_frame_url(js.ptr(), frame.0 as *const NsNode) };
    (!url.is_null()).then(|| unsafe { borrowed(url) })
}

pub(crate) fn frame_realm(js: Js, frame: Frame) -> Option<Realm> {
    Realm::from_ptr(unsafe { ns_js_frame_context(js.ptr(), frame.0 as *const NsNode) })
}

pub(crate) fn structured_clone_transfer(
    scope: &mut Scope<'_>,
    value: &Value,
    transfer: &Value,
    seed_from: &Value,
    seed_to: &Value,
) -> Result<Value, Value> {
    let raw = unsafe {
        ns_structured_clone_transfer(
            quickjs::raw_context(scope),
            quickjs::raw(value),
            quickjs::into_raw(transfer.clone()),
            quickjs::raw(seed_from),
            quickjs::raw(seed_to),
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) fn realm_now_ms(realm: Realm) -> f64 {
    unsafe { ns_perf_realm_now_ms(realm.ptr()) }
}

pub(crate) fn caller_realm(scope: &Scope<'_>) -> Realm {
    let ctx = quickjs::raw_context(scope);
    Realm::from_ptr(unsafe { JS_GetCallerRealm(ctx) }).unwrap_or(Realm(ctx as usize))
}

pub(crate) fn function_realm(scope: &mut Scope<'_>, function: &Value) -> Realm {
    quickjs::function_realm(scope, function)
        .ok()
        .and_then(Realm::from_ptr)
        .unwrap_or_else(|| Realm::of(scope))
}

pub(crate) fn budget_enter(js: Js) -> i64 {
    unsafe { ns_js_budget_enter(js.ptr()) }
}

pub(crate) fn budget_leave(js: Js, saved: i64) {
    unsafe { ns_js_budget_leave(js.ptr(), saved) };
}

pub(crate) fn realm_url_enter(js: Js, realm: Realm) -> UrlToken {
    UrlToken(unsafe { ns_js_realm_url_enter(js.ptr(), realm.ptr()) })
}

pub(crate) fn realm_url_leave(js: Js, token: UrlToken) {
    unsafe { ns_js_realm_url_leave(js.ptr(), token.0) };
}

pub(crate) fn dispatch_main_window_event(js: Js, kind: &str, event: Value) {
    let kind = c_string(kind.as_bytes());
    unsafe { ns_js_dispatch_main_window_event(js.ptr(), kind.as_ptr(), quickjs::into_raw(event)) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_main_window_event(
    js: *mut NsJs,
    kind: *const c_char,
    event: JSValue,
) {
    let js = Js::of(js);
    let saved_ctx = unsafe { ns_js_main_context(js.ptr()) };
    let saved_doc = current_document(js);
    let main_ctx = main_realm(js).map_or(saved_ctx, Realm::ptr);
    let main_doc = crate::window_events::top_document(saved_doc);
    unsafe {
        ns_js_set_realm(js.ptr(), main_ctx, Node::ptr_or_null(main_doc).cast_mut());
        ns_js_dispatch_window_only_event(
            js.ptr(),
            Node::ptr_or_null(main_doc),
            kind,
            event,
            core::ptr::null_mut(),
        );
        ns_js_set_realm(js.ptr(), saved_ctx, Node::ptr_or_null(saved_doc).cast_mut());
    }
}

pub(crate) fn port_transfer_prepare(
    scope: &mut Scope<'_>,
    transfer: &Value,
    realm: Realm,
) -> Result<(Value, Value), Value> {
    let mut old_ports = quickjs::UNDEFINED;
    let mut new_ports = quickjs::UNDEFINED;
    let status = unsafe {
        ns_port_transfer_prepare(
            quickjs::raw_context(scope),
            quickjs::raw(transfer),
            quickjs::UNDEFINED,
            realm.ptr(),
            &mut old_ports,
            &mut new_ports,
        )
    };
    if status < 0 {
        return Err(quickjs::take_exception(scope));
    }
    unsafe {
        Ok((
            quickjs::take_value(scope, old_ports),
            quickjs::take_value(scope, new_ports),
        ))
    }
}

pub(crate) fn port_transfer_commit(scope: &mut Scope<'_>, old_ports: &Value, new_ports: &Value) {
    unsafe {
        ns_port_transfer_commit(
            quickjs::raw_context(scope),
            quickjs::raw(old_ports),
            quickjs::raw(new_ports),
        )
    };
}

pub(crate) fn cross_origin_window(scope: &mut Scope<'_>, window: &Value) -> Value {
    let raw = unsafe {
        ns_iframe_cross_origin_window(
            quickjs::raw_context(scope),
            quickjs::into_raw(window.clone()),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

unsafe extern "C" fn deliver_job(ctx: *mut JSContext, argc: c_int, argv: *mut JSValue) -> JSValue {
    if argc < 2 || argv.is_null() {
        return quickjs::UNDEFINED;
    }
    let raw = unsafe { core::slice::from_raw_parts(argv, argc as usize) };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let target = quickjs::borrow_value(scope, raw[0]);
            let event = quickjs::borrow_value(scope, raw[1]);
            message::deliver(scope, &target, &event);
        })
    };
    quickjs::UNDEFINED
}

pub(crate) fn queue_delivery(scope: &mut Scope<'_>, target: &Value, event: &Value) {
    let mut raw = [quickjs::raw(target), quickjs::raw(event)];
    unsafe {
        ns_js_queue_message_task(
            quickjs::raw_context(scope),
            deliver_job,
            raw.len() as c_int,
            raw.as_mut_ptr(),
        )
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_bind_post_message(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            message::bind_post_message(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_make_post_message(
    ctx: *mut JSContext,
    window: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let window = quickjs::borrow_value(scope, window);
            quickjs::into_raw(message::make_post_message(scope, &window))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_links_clear(js: *const NsJs, _destroy: GBoolean) {
    message::links_clear(Js::of(js));
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_link_outward(
    js: *const NsJs,
    outward: JSValue,
    realm_window: JSValue,
) {
    let js = Js::of(js);
    let Some(realm) = current_realm(js) else {
        return;
    };
    realm.enter(|scope| {
        let outward = unsafe { quickjs::borrow_value(scope, outward) };
        let realm_window = unsafe { quickjs::borrow_value(scope, realm_window) };
        message::link_outward(js, &outward, &realm_window);
    });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_forward_of(js: *const NsJs, outward: JSValue) -> JSValue {
    let js = Js::of(js);
    let Some(realm) = current_realm(js) else {
        return quickjs::UNDEFINED;
    };
    realm.enter(|scope| {
        let outward = unsafe { quickjs::borrow_value(scope, outward) };
        quickjs::into_raw(message::forward_of(js, &outward))
    })
}

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[repr(C)]
struct NsUrlParts {
    href: *mut c_char,
    protocol: *mut c_char,
    origin: *mut c_char,
    host: *mut c_char,
    hostname: *mut c_char,
    port: *mut c_char,
    pathname: *mut c_char,
    search: *mut c_char,
    hash: *mut c_char,
    username: *mut c_char,
    password: *mut c_char,
}

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
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_set_current_url(js: *mut NsJs, url: *const c_char);
    fn ns_js_soft_navigate(js: *mut NsJs, url: *const c_char, replace: GBoolean);
    fn ns_js_navigate(js: *mut NsJs, url: *const c_char, reload: GBoolean) -> GBoolean;
    fn ns_js_window_events_blocked(js: *const NsJs) -> GBoolean;
    fn ns_js_dispatch_document_window_event(js: *mut NsJs, kind: *const c_char, event: JSValue);
    fn ns_window_structured_clone(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_target_make_event(ctx: *mut JSContext, target: JSValue, kind: *const c_char) -> JSValue;
    fn ns_target_dispatch_with_event(
        ctx: *mut JSContext,
        target: JSValue,
        kind: *const c_char,
        event: JSValue,
    );
    fn ns_bind_event_target_listeners(ctx: *mut JSContext, object: JSValue);
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_url_origin_from(url: *const c_char) -> *mut c_char;
    fn ns_url_parts_new(url: *const c_char) -> *mut NsUrlParts;
    fn ns_url_parts_free(parts: *mut NsUrlParts);
    fn ns_url_set_component_len(
        href: *const c_char,
        component: *const c_char,
        value: *const c_char,
        value_len: usize,
    ) -> *mut c_char;
    fn ns_js_top_url(js: *mut NsJs) -> *const c_char;
    fn ns_js_set_top_url(js: *mut NsJs, url: *const c_char);
    fn ns_js_url_parses(js: *mut NsJs, url: *const c_char) -> GBoolean;
    fn ns_js_in_frame_load(js: *const NsJs) -> GBoolean;
    fn ns_js_can_navigate(js: *const NsJs) -> GBoolean;
    fn ns_js_fragment_navigated(js: *mut NsJs, url: *const c_char);
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_js_has_transient_activation(js: *mut NsJs) -> GBoolean;
    fn ns_js_consume_user_activation(js: *mut NsJs);
    fn ns_js_events_suspended(js: *const NsJs) -> GBoolean;
    fn ns_js_realm_scope_push(js: *mut NsJs, realm: *mut JSContext, buf: *mut c_void);
    fn ns_js_realm_scope_pop(js: *mut NsJs, buf: *mut c_void);
    fn ns_js_set_realm(js: *mut NsJs, ctx: *mut JSContext, doc: *mut NsNode);
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_js_iframe_document_node(js: *const NsJs) -> *const NsNode;
    fn ns_iframe_document_node(iframe: *const NsNode) -> *mut NsNode;
    fn ns_js_ce_main_doc(js: *const NsJs) -> *const NsNode;
    fn g_utf8_casefold(text: *const c_char, len: isize) -> *mut c_char;
}

fn c_string(bytes: &[u8]) -> CString {
    CString::new(bytes).unwrap_or_default()
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
    Js(quickjs::context_opaque(scope) as usize)
}

pub(crate) fn with_main_context<R>(js: Js, f: impl FnOnce(&mut Scope<'_>) -> R) -> Option<R> {
    if js.is_null() {
        return None;
    }
    let ctx = unsafe { ns_js_main_context(js.ptr()) };
    if ctx.is_null() {
        return None;
    }
    Some(unsafe { quickjs::with_context(ctx, f) })
}

pub(crate) fn current_url(js: Js) -> Vec<u8> {
    unsafe { borrowed(ns_js_current_url(js.ptr())) }
}

pub(crate) fn set_current_url(js: Js, url: &[u8]) {
    let url = c_string(url);
    unsafe { ns_js_set_current_url(js.ptr(), url.as_ptr()) };
}

pub(crate) fn soft_navigate(js: Js, url: &[u8], replace: bool) {
    let url = c_string(url);
    unsafe { ns_js_soft_navigate(js.ptr(), url.as_ptr(), southstar_glib::boolean(replace)) };
}

pub(crate) fn navigate(js: Js, url: &[u8], reload: bool) -> bool {
    let url = c_string(url);
    unsafe { ns_js_navigate(js.ptr(), url.as_ptr(), southstar_glib::boolean(reload)) != 0 }
}

pub(crate) fn window_events_blocked(js: Js) -> bool {
    unsafe { ns_js_window_events_blocked(js.ptr()) != 0 }
}

pub(crate) fn dispatch_document_window_event(js: Js, kind: &str, event: Value) {
    let kind = c_string(kind.as_bytes());
    unsafe {
        ns_js_dispatch_document_window_event(js.ptr(), kind.as_ptr(), quickjs::into_raw(event))
    };
}

pub(crate) fn structured_clone(scope: &mut Scope<'_>, value: &Value) -> Result<Value, Value> {
    quickjs::call_c_function(
        scope,
        ns_window_structured_clone,
        &Value::undefined(),
        core::slice::from_ref(value),
    )
}

pub(crate) fn target_make_event(scope: &mut Scope<'_>, target: &Value, kind: &str) -> Value {
    let kind = c_string(kind.as_bytes());
    let raw = unsafe {
        ns_target_make_event(
            quickjs::raw_context(scope),
            quickjs::raw(target),
            kind.as_ptr(),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn target_dispatch(scope: &mut Scope<'_>, target: &Value, kind: &str, event: &Value) {
    let kind = c_string(kind.as_bytes());
    unsafe {
        ns_target_dispatch_with_event(
            quickjs::raw_context(scope),
            quickjs::raw(target),
            kind.as_ptr(),
            quickjs::raw(event),
        )
    };
}

pub(crate) fn bind_event_target_listeners(scope: &mut Scope<'_>, object: &Value) {
    unsafe { ns_bind_event_target_listeners(quickjs::raw_context(scope), quickjs::raw(object)) };
}

pub(crate) fn make_window_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    crate::window_events::make_window_event(scope, kind)
}

pub(crate) fn make_event(scope: &mut Scope<'_>, kind: &str) -> Value {
    let kind = c_string(kind.as_bytes());
    let raw = unsafe {
        ns_make_event(
            quickjs::raw_context(scope),
            kind.as_ptr(),
            core::ptr::null(),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_window_event(ctx: *mut JSContext, kind: *const c_char) -> JSValue {
    let kind = unsafe { borrowed(kind) };
    let kind = String::from_utf8_lossy(&kind);
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(crate::window_events::make_window_event(scope, &kind))
        })
    }
}

pub(crate) fn url_resolve(base: Option<&[u8]>, href: &[u8]) -> Option<Vec<u8>> {
    let base = base.map(c_string);
    let href = c_string(href);
    unsafe {
        taken(ns_url_resolve(
            base.as_ref()
                .map_or(core::ptr::null(), |base| base.as_ptr()),
            href.as_ptr(),
        ))
    }
}

pub(crate) fn url_origin(url: &[u8]) -> Option<Vec<u8>> {
    let url = c_string(url);
    unsafe { taken(ns_url_origin_from(url.as_ptr())) }
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
    let url = c_string(url);
    let parts = unsafe { ns_url_parts_new(url.as_ptr()) };
    let p = unsafe { parts.as_ref() }?;
    let value = match field {
        UrlField::Protocol => p.protocol,
        UrlField::Origin => p.origin,
        UrlField::Host => p.host,
        UrlField::Hostname => p.hostname,
        UrlField::Port => p.port,
        UrlField::Pathname => p.pathname,
        UrlField::Search => p.search,
        UrlField::Hash => p.hash,
    };
    let value = (!value.is_null()).then(|| unsafe { borrowed(value) });
    unsafe { ns_url_parts_free(parts) };
    value
}

pub(crate) fn url_set_component(href: &[u8], component: &str, value: &[u8]) -> Option<Vec<u8>> {
    let href = c_string(href);
    let component = c_string(component.as_bytes());
    unsafe {
        taken(ns_url_set_component_len(
            href.as_ptr(),
            component.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
        ))
    }
}

pub(crate) fn top_url(js: Js) -> Vec<u8> {
    unsafe { borrowed(ns_js_top_url(js.ptr())) }
}

pub(crate) fn set_top_url(js: Js, url: &[u8]) {
    let url = c_string(url);
    unsafe { ns_js_set_top_url(js.ptr(), url.as_ptr()) };
}

pub(crate) fn url_parses(js: Js, url: &[u8]) -> bool {
    let url = c_string(url);
    unsafe { ns_js_url_parses(js.ptr(), url.as_ptr()) != 0 }
}

pub(crate) fn in_frame_load(js: Js) -> bool {
    unsafe { ns_js_in_frame_load(js.ptr()) != 0 }
}

pub(crate) fn can_navigate(js: Js) -> bool {
    unsafe { ns_js_can_navigate(js.ptr()) != 0 }
}

pub(crate) fn fragment_navigated(js: Js, url: &[u8]) {
    let url = c_string(url);
    unsafe { ns_js_fragment_navigated(js.ptr(), url.as_ptr()) };
}

pub(crate) fn dispatch_hashchange(js: Js, old_url: &[u8], new_url: &[u8]) {
    let old_url = c_string(old_url);
    let new_url = c_string(new_url);
    crate::window_events::dispatch_hashchange(js, old_url.as_bytes(), new_url.as_bytes());
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_hashchange(
    js: *mut NsJs,
    old_url: *const c_char,
    new_url: *const c_char,
) {
    let old_url = unsafe { borrowed(old_url) };
    let new_url = unsafe { borrowed(new_url) };
    crate::window_events::dispatch_hashchange(Js::of(js), &old_url, &new_url);
}

#[repr(C, align(16))]
struct ScopeBuffer([u8; 128]);

pub(crate) fn has_main_context(js: Js) -> bool {
    !unsafe { ns_js_main_context(js.ptr()) }.is_null()
}

pub(crate) fn events_suspended(js: Js) -> bool {
    unsafe { ns_js_events_suspended(js.ptr()) != 0 }
}

pub(crate) fn in_main_realm_scope(js: Js, f: impl FnOnce()) {
    let realm = unsafe { ns_js_main_realm_context(js.ptr()) };
    let mut buffer = ScopeBuffer([0; 128]);
    let buffer = buffer.0.as_mut_ptr().cast::<c_void>();
    unsafe { ns_js_realm_scope_push(js.ptr(), realm, buffer) };
    f();
    unsafe { ns_js_realm_scope_pop(js.ptr(), buffer) };
}

pub(crate) fn unwrap(value: &Value) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn iframe_document(js: Js) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_js_iframe_document_node(js.ptr())) }
}

pub(crate) fn frame_document(frame: Option<Node<'static>>) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_iframe_document_node(Node::ptr_or_null(frame))) }
}

pub(crate) fn ce_main_document(js: Js) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_js_ce_main_doc(js.ptr())) }
}

pub(crate) fn forward_of(scope: &mut Scope<'_>, js: Js, window: &Value) -> Value {
    let raw = unsafe { ns_window_forward_of(js.ptr(), quickjs::raw(window)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn casefold(text: &[u8]) -> Vec<u8> {
    let text = c_string(text);
    let folded = unsafe { g_utf8_casefold(text.as_ptr(), -1) };
    unsafe { taken(folded) }.unwrap_or_default()
}

pub(crate) fn log_line(js: Js, line: &[u8]) {
    let line = c_string(line);
    unsafe { ns_js_log_line(js.ptr(), line.as_ptr()) };
}

pub(crate) fn has_transient_activation(js: Js) -> bool {
    unsafe { ns_js_has_transient_activation(js.ptr()) != 0 }
}

pub(crate) fn consume_user_activation(js: Js) {
    unsafe { ns_js_consume_user_activation(js.ptr()) };
}

pub(crate) fn url_parts(url: &[u8]) -> Option<UrlOrigin> {
    let url = c_string(url);
    let parts = unsafe { ns_url_parts_new(url.as_ptr()) };
    let p = unsafe { parts.as_ref() }?;
    let origin = unsafe {
        UrlOrigin {
            protocol: borrowed(p.protocol),
            hostname: borrowed(p.hostname),
            port: borrowed(p.port),
        }
    };
    unsafe { ns_url_parts_free(parts) };
    Some(origin)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_install_history(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            let history = history::make_history(scope);
            set(scope, &global, "history", history);
            let navigation = navigation::make_navigation(scope);
            set(scope, &global, "navigation", navigation);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_make_location(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(location::make_location(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_open_method(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, location::open) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_history_teardown(js: *const NsJs) {
    crate::teardown(Js::of(js));
}

unsafe extern "C" {
    fn ns_window_child_frame_window(
        ctx: *mut JSContext,
        doc: *mut NsNode,
        index: u32,
        name: *const c_char,
        raw: GBoolean,
    ) -> JSValue;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_js_window_action(js: *mut NsJs, action: *const c_char);
    fn ns_css_viewport_w() -> f64;
    fn ns_css_viewport_h() -> f64;
    fn ns_css_device_pixel_ratio() -> f64;
    fn ns_services_screen_metrics(
        width: *mut c_int,
        height: *mut c_int,
        avail_width: *mut c_int,
        avail_height: *mut c_int,
        avail_left: *mut c_int,
        avail_top: *mut c_int,
    );
}

pub(crate) fn current_document_for(scope: &mut Scope<'_>, window: &Value) -> Option<Node<'static>> {
    crate::documents::current_document_for(scope, window)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_current_document_for(
    ctx: *mut JSContext,
    window: JSValue,
) -> *mut NsNode {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let window = quickjs::borrow_value(scope, window);
            Node::ptr_or_null(crate::documents::current_document_for(scope, &window)).cast_mut()
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_document_for(
    ctx: *mut JSContext,
    window: JSValue,
) -> *mut NsNode {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let window = quickjs::borrow_value(scope, window);
            Node::ptr_or_null(crate::documents::document_for(scope, &window)).cast_mut()
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_find(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, crate::documents::find) }
}

pub(crate) fn child_frame_window(
    scope: &mut Scope<'_>,
    doc: Node<'_>,
    index: u32,
    name: Option<&CStr>,
) -> Value {
    let raw = unsafe {
        ns_window_child_frame_window(
            quickjs::raw_context(scope),
            doc.as_mut_ptr(),
            index,
            name.map_or(core::ptr::null(), CStr::as_ptr),
            southstar_glib::FALSE,
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value).unwrap_or_else(|_| Value::undefined())
}

pub(crate) fn wrap(scope: &mut Scope<'_>, node: Node<'_>) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), node.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn window_action(js: Js, action: &str) {
    let action = c_string(action.as_bytes());
    unsafe { ns_js_window_action(js.ptr(), action.as_ptr()) };
}

pub(crate) fn viewport_width() -> f64 {
    unsafe { ns_css_viewport_w() }
}

pub(crate) fn viewport_height() -> f64 {
    unsafe { ns_css_viewport_h() }
}

pub(crate) fn device_pixel_ratio() -> f64 {
    unsafe { ns_css_device_pixel_ratio() }
}

pub(crate) fn screen_size() -> (i32, i32) {
    let (mut width, mut height) = (0, 0);
    unsafe {
        ns_services_screen_metrics(
            &mut width,
            &mut height,
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
            core::ptr::null_mut(),
        )
    };
    (width, height)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_install_browsing_context(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            context::install_browsing_context(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_install_state(
    js: *const NsJs,
    ctx: *mut JSContext,
    global: JSValue,
) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            context::install_state(scope, Js::of(js), &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_install_actions(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            context::install_actions(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_sync_window_metrics(js: *const NsJs) {
    with_main_context(Js::of(js), context::sync_metrics);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_named_property(
    ctx: *mut JSContext,
    window: JSValue,
    key: JSValue,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let window = quickjs::borrow_value(scope, window);
            let key = quickjs::borrow_value(scope, key);
            quickjs::into_raw(named::named_property(scope, &window, &key))
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_window_named_suspend() {
    named::suspend();
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_window_named_resume() {
    named::resume();
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
pub unsafe extern "C" fn ns_document_expose_legacy_named(
    ctx: *mut JSContext,
    root: *const NsNode,
    document: JSValue,
) {
    let Some(root) = (unsafe { Node::from_ptr(root) }) else {
        return;
    };
    if ctx.is_null() {
        return;
    }
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let document = quickjs::borrow_value(scope, document);
            named::expose_legacy_named(scope, root, &document);
        })
    }
}

unsafe extern "C" {
    fn ns_js_halted(js: *const NsJs) -> GBoolean;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_page_url(js: *const NsJs) -> *const c_char;
    fn ns_window_scroll_prop(ctx: *mut JSContext, prop: *const c_char) -> f64;
    fn ns_js_queue_scrollend_document(js: *mut NsJs);
    fn ns_box_set_hit_viewport(scroll_x: f64, scroll_y: f64);
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_js_dispatch_window_only_event(
        js: *mut NsJs,
        target_doc: *const NsNode,
        kind: *const c_char,
        event: JSValue,
        default_prevented: *mut GBoolean,
    );
    fn ns_make_event(ctx: *mut JSContext, kind: *const c_char, target: *const NsNode) -> JSValue;
    fn ns_observer_schedule_tick(js: *mut NsJs);
    fn ns_services_reeval_media_queries(js: *mut NsJs);
    fn ns_url_is_http_or_https(url: *const c_char) -> GBoolean;
    fn ns_url_same_origin(a: *const c_char, b: *const c_char) -> GBoolean;
    fn ns_cors_allows(
        doc_url: *const c_char,
        resp_url: *const c_char,
        cors_header: *const c_char,
    ) -> GBoolean;
    fn ns_js_frame_of_realm(js: *const NsJs, realm: *mut JSContext) -> *mut NsNode;
}

pub(crate) fn halted(js: Js) -> bool {
    unsafe { ns_js_halted(js.ptr()) != 0 }
}

pub(crate) fn current_document(js: Js) -> Option<Node<'static>> {
    unsafe { Node::from_ptr(ns_js_current_document(js.ptr())) }
}

pub(crate) fn window_scroll_prop(scope: &mut Scope<'_>, prop: &CStr) -> f64 {
    unsafe { ns_window_scroll_prop(quickjs::raw_context(scope), prop.as_ptr()) }
}

pub(crate) fn queue_scrollend_document(js: Js) {
    unsafe { ns_js_queue_scrollend_document(js.ptr()) };
}

pub(crate) fn set_hit_viewport(x: f64, y: f64) {
    unsafe { ns_box_set_hit_viewport(x, y) };
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
    unsafe {
        ns_js_dispatch_window_only_event(
            js.ptr(),
            Node::ptr_or_null(doc),
            kind.as_ptr(),
            quickjs::into_raw(event),
            core::ptr::null_mut(),
        )
    };
}

pub(crate) fn make_node_event(
    scope: &mut Scope<'_>,
    kind: &CStr,
    target: Option<Node<'_>>,
) -> Value {
    let raw = unsafe {
        ns_make_event(
            quickjs::raw_context(scope),
            kind.as_ptr(),
            Node::ptr_or_null(target),
        )
    };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn schedule_observer_tick(js: Js) {
    unsafe { ns_observer_schedule_tick(js.ptr()) };
}

pub(crate) fn reeval_media_queries(js: Js) {
    unsafe { ns_services_reeval_media_queries(js.ptr()) };
}

pub(crate) fn url_is_http_or_https(url: &[u8]) -> bool {
    let url = c_string(url);
    unsafe { ns_url_is_http_or_https(url.as_ptr()) != 0 }
}

pub(crate) fn url_same_origin(a: &[u8], b: &[u8]) -> bool {
    let a = c_string(a);
    let b = c_string(b);
    unsafe { ns_url_same_origin(a.as_ptr(), b.as_ptr()) != 0 }
}

pub(crate) fn cors_allows(doc_url: Option<&[u8]>, url: &[u8], allow: &[u8]) -> bool {
    let doc_url = doc_url.map(c_string);
    let url = c_string(url);
    let allow = c_string(allow);
    let doc_ptr = doc_url.as_ref().map_or(core::ptr::null(), |d| d.as_ptr());
    unsafe { ns_cors_allows(doc_ptr, url.as_ptr(), allow.as_ptr()) != 0 }
}

fn caller_document_url(js: Js, ctx: *mut JSContext) -> Option<Vec<u8>> {
    let frame = unsafe { Node::from_ptr(ns_js_frame_of_realm(js.ptr(), JS_GetCallerRealm(ctx))) };
    if let Some(url) = frame.and_then(|frame| frame.attr(c"data-nd-frame-url"))
        && !url.is_empty()
    {
        return Some(url.to_bytes().to_vec());
    }
    let origin = unsafe { ns_js_document_origin(js.ptr()) };
    let url = if origin.is_null() {
        unsafe { ns_js_page_url(js.ptr()) }
    } else {
        origin
    };
    (!url.is_null()).then(|| unsafe { borrowed(url) })
}

fn opt_bytes<'a>(p: *const c_char) -> Option<&'a [u8]> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_anchor_fragment_navigate(
    js: *mut NsJs,
    abs_url: *const c_char,
) -> GBoolean {
    let Some(abs_url) = opt_bytes(abs_url) else {
        return 0;
    };
    southstar_glib::boolean(page::anchor_fragment_navigate(Js::of(js), abs_url))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_note_viewport_scroll(js: *mut NsJs, x: f64, y: f64) {
    page::note_viewport_scroll(Js::of(js), x, y);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_resize(js: *mut NsJs) {
    if !js.is_null() {
        page::dispatch_resize(Js::of(js));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_reeval_media_queries(js: *mut NsJs) {
    reeval_media_queries(Js::of(js));
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
    ctx: *mut JSContext,
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
