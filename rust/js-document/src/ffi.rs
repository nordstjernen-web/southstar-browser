//! Southstar — the C ABI of the document bindings as declared in src/js_internal.h, and the js.c, net and GLib calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::Element;

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }
}

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_document_is_realm_document(ctx: *mut JSContext, doc: JSValue) -> GBoolean;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_current_url(js: *const NsJs) -> *const c_char;
    fn ns_js_cookie_value(js: *const NsJs) -> *const c_char;
    fn ns_js_set_cookie_value(js: *mut NsJs, value: *const c_char);
    fn ns_js_partition_key(js: *const NsJs) -> *const c_char;
    fn ns_js_referrer(js: *const NsJs) -> *const c_char;
    fn ns_js_frame_referrer_for(js: *const NsJs, frame: *const NsNode) -> *const c_char;
    fn ns_js_ready_state(js: *const NsJs) -> c_int;
    fn ns_js_doc_ready_state(js: *const NsJs, doc: *const NsNode) -> c_int;
    fn ns_js_current_script(js: *const NsJs) -> *const NsNode;
    fn ns_js_focused_node(js: *const NsJs) -> *const NsNode;
    fn ns_js_focused_doc(js: *const NsJs) -> *const NsNode;
    fn ns_js_clear_focused_node(js: *mut NsJs);
    fn ns_js_unorphan_node(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_orphan_node(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_record_child_change(
        js: *mut NsJs,
        parent: *mut NsNode,
        added: *mut NsNode,
        removed: *mut NsNode,
        previous_sibling: *mut NsNode,
        next_sibling: *mut NsNode,
    );
    fn ns_node_iters_pre_remove(js: *mut NsJs, removed: *mut NsNode);
    fn ns_ce_disconnect_subtree(js: *mut NsJs, root: *mut NsNode);
    fn ns_element_insert_before_single(
        js: *mut NsJs,
        parent: *mut NsNode,
        node: *mut NsNode,
        reference: *mut NsNode,
    );
    fn ns_element_replace_all_recorded(js: *mut NsJs, node: *mut NsNode, added: *mut NsNode);
    fn ns_js_set_attr_recorded_len(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
        len: isize,
    );
    fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode);
    fn ns_node_remove(child: *mut NsNode);
    fn ns_net_cookies_for_js(url: *const c_char) -> *mut c_char;
    fn ns_net_cookie_store_from_js(url: *const c_char, cookie: *const c_char);
    fn ns_net_http_date(date: *const c_char) -> i64;
    fn g_date_time_new_now_local() -> *mut c_void;
    fn g_date_time_format(datetime: *mut c_void, format: *const c_char) -> *mut c_char;
    fn g_date_time_unref(datetime: *mut c_void);
}

fn node_of(raw: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(raw) }
}

fn raw(node: Option<Element>) -> *mut NsNode {
    node.map_or(ptr::null_mut(), Node::as_mut_ptr)
}

fn borrowed_text(text: *const c_char) -> Option<Vec<u8>> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) }.to_bytes().to_vec())
}

fn take_glib_string(text: *mut c_char) -> Option<Vec<u8>> {
    let bytes = borrowed_text(text)?;
    unsafe { glib::g_free(text.cast()) };
    Some(bytes)
}

fn c_bytes(bytes: &[u8]) -> CString {
    CString::new(crate::until_nul(bytes)).unwrap_or_default()
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope).cast())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    node_of(unsafe { ns_unwrap_element(quickjs::raw(value)) })
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), raw(node)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    let name = CString::new(name).unwrap_or_default();
    let message = CString::new(message).unwrap_or_default();
    unsafe {
        ns_throw_dom_exception(
            quickjs::raw_context(scope),
            name.as_ptr(),
            code,
            message.as_ptr(),
        )
    };
    quickjs::take_exception(scope)
}

pub(crate) fn is_realm_document(scope: &Scope<'_>, doc: &Value) -> bool {
    unsafe { ns_document_is_realm_document(quickjs::raw_context(scope), quickjs::raw(doc)) != 0 }
}

pub(crate) fn current_document(js: Js) -> Option<Element> {
    if js.is_null() {
        return None;
    }
    node_of(unsafe { ns_js_current_document(js.0) })
}

pub(crate) fn current_url(js: Js) -> Option<Vec<u8>> {
    borrowed_text(unsafe { ns_js_current_url(js.0) }).filter(|url| !url.is_empty())
}

pub(crate) fn cookie_value(js: Js) -> Option<Vec<u8>> {
    borrowed_text(unsafe { ns_js_cookie_value(js.0) })
}

pub(crate) fn set_cookie_value(js: Js, value: &[u8]) {
    let value = c_bytes(value);
    unsafe { ns_js_set_cookie_value(js.0, value.as_ptr()) };
}

pub(crate) fn partition_key(js: Js) -> Option<Vec<u8>> {
    borrowed_text(unsafe { ns_js_partition_key(js.0) })
}

pub(crate) fn referrer(js: Js) -> Option<Vec<u8>> {
    borrowed_text(unsafe { ns_js_referrer(js.0) })
}

pub(crate) fn frame_referrer(js: Js, frame: Element) -> Option<Vec<u8>> {
    borrowed_text(unsafe { ns_js_frame_referrer_for(js.0, frame.as_ptr()) })
}

pub(crate) fn ready_state(js: Js, doc: Option<Element>) -> i32 {
    if let Some(doc) = doc {
        let state = unsafe { ns_js_doc_ready_state(js.0, doc.as_ptr()) };
        if state >= 0 {
            return state;
        }
    }
    unsafe { ns_js_ready_state(js.0) }
}

pub(crate) fn current_script(js: Js) -> Option<Element> {
    node_of(unsafe { ns_js_current_script(js.0) })
}

pub(crate) fn focused_node(js: Js) -> Option<Element> {
    node_of(unsafe { ns_js_focused_node(js.0) })
}

pub(crate) fn focused_doc(js: Js) -> Option<Element> {
    node_of(unsafe { ns_js_focused_doc(js.0) })
}

pub(crate) fn clear_focused_node(js: Js) {
    unsafe { ns_js_clear_focused_node(js.0) };
}

pub(crate) fn unorphan_node(js: Js, node: Element) {
    unsafe { ns_js_unorphan_node(js.0, node.as_mut_ptr()) };
}

pub(crate) fn orphan_node(js: Js, node: Element) {
    unsafe { ns_js_orphan_node(js.0, node.as_mut_ptr()) };
}

pub(crate) fn mark_mutated(js: Js) {
    unsafe { ns_js_mark_mutated(js.0) };
}

pub(crate) fn record_child_change(
    js: Js,
    parent: Element,
    added: Option<Element>,
    removed: Option<Element>,
    previous: Option<Element>,
    next: Option<Element>,
) {
    unsafe {
        ns_js_record_child_change(
            js.0,
            parent.as_mut_ptr(),
            raw(added),
            raw(removed),
            raw(previous),
            raw(next),
        )
    };
}

pub(crate) fn iters_pre_remove(js: Js, node: Element) {
    unsafe { ns_node_iters_pre_remove(js.0, node.as_mut_ptr()) };
}

pub(crate) fn ce_disconnect_subtree(js: Js, node: Element) {
    unsafe { ns_ce_disconnect_subtree(js.0, node.as_mut_ptr()) };
}

pub(crate) fn insert_before_single(js: Js, parent: Element, node: Element, reference: Element) {
    unsafe {
        ns_element_insert_before_single(
            js.0,
            parent.as_mut_ptr(),
            node.as_mut_ptr(),
            reference.as_mut_ptr(),
        )
    };
}

pub(crate) fn replace_all_recorded(js: Js, node: Element, added: Option<Element>) {
    unsafe { ns_element_replace_all_recorded(js.0, node.as_mut_ptr(), raw(added)) };
}

pub(crate) fn set_attr_recorded(js: Js, node: Element, name: &CStr, value: &[u8]) {
    let mut terminated = value.to_vec();
    terminated.push(0);
    unsafe {
        ns_js_set_attr_recorded_len(
            js.0,
            node.as_mut_ptr(),
            name.as_ptr(),
            terminated.as_ptr().cast(),
            value.len() as isize,
        )
    };
}

pub(crate) fn append_child(parent: Element, child: Element) {
    unsafe { ns_node_append_child(parent.as_mut_ptr(), child.as_mut_ptr()) };
}

pub(crate) fn remove_node(node: Element) {
    unsafe { ns_node_remove(node.as_mut_ptr()) };
}

pub(crate) fn new_element(name: &[u8]) -> Element {
    southstar_dom::node::new_element(glib::strdup(name))
}

pub(crate) fn new_text(text: &[u8]) -> Element {
    southstar_dom::node::new_text(
        southstar_dom::node::KIND_TEXT,
        glib::strdup(text),
        text.len() as u32,
    )
}

pub(crate) fn attr_bytes(node: Element, name: &CStr) -> Option<&'static [u8]> {
    let attr = southstar_dom::attrs::find(node, name)?;
    let value = attr.value_ptr();
    if value.is_null() {
        return None;
    }
    Some(unsafe { core::slice::from_raw_parts(value.cast(), attr.value_len() as usize) })
}

pub(crate) fn cookies_for_js(url: &[u8]) -> Option<Vec<u8>> {
    let url = c_bytes(url);
    take_glib_string(unsafe { ns_net_cookies_for_js(url.as_ptr()) })
}

pub(crate) fn cookie_store_from_js(url: &[u8], cookie: &[u8]) {
    let url = c_bytes(url);
    let cookie = c_bytes(cookie);
    unsafe { ns_net_cookie_store_from_js(url.as_ptr(), cookie.as_ptr()) };
}

pub(crate) fn http_date(date: &[u8]) -> i64 {
    let date = c_bytes(date);
    unsafe { ns_net_http_date(date.as_ptr()) }
}

pub(crate) fn local_now_formatted(format: &CStr) -> Option<Vec<u8>> {
    let now = unsafe { g_date_time_new_now_local() };
    if now.is_null() {
        return None;
    }
    let text = take_glib_string(unsafe { g_date_time_format(now, format.as_ptr()) });
    unsafe { g_date_time_unref(now) };
    Some(text.unwrap_or_default())
}

unsafe fn getter(ctx: *mut JSContext, this_val: JSValue, f: NativeFn) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), f) }
}

unsafe fn setter(ctx: *mut JSContext, this_val: JSValue, val: JSValue, f: NativeFn) -> JSValue {
    let mut argv = [val];
    unsafe { quickjs::call_native(ctx, this_val, 1, argv.as_mut_ptr(), f) }
}

macro_rules! getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
                unsafe { getter(ctx, this_val, $f) }
            }
        )*
    };
}

macro_rules! setters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            pub unsafe extern "C" fn $name(
                ctx: *mut JSContext,
                this_val: JSValue,
                val: JSValue,
            ) -> JSValue {
                unsafe { setter(ctx, this_val, val, $f) }
            }
        )*
    };
}

getters! {
    ns_document_get_documentElement => crate::elements::get_document_element,
    ns_document_get_body => crate::elements::get_body,
    ns_document_get_head => crate::elements::get_head,
    ns_document_get_scrollingElement => crate::elements::get_scrolling_element,
    ns_document_get_activeElement => crate::elements::get_active_element,
    ns_document_get_currentScript => crate::elements::get_current_script,
    ns_document_get_scripts => crate::elements::get_scripts,
    ns_document_get_anchors => crate::elements::get_anchors,
    ns_document_get_embeds => crate::elements::empty_list,
    ns_document_get_plugins => crate::elements::empty_list,
    ns_document_get_applets => crate::elements::empty_list,
    ns_document_get_title => crate::props::get_title,
    ns_document_get_dir => crate::props::get_dir,
    ns_document_get_cookie => crate::cookie::get_cookie,
    ns_document_get_referrer => crate::props::get_referrer,
    ns_document_get_readyState => crate::props::get_ready_state,
    ns_document_get_designMode => crate::props::get_design_mode,
    ns_document_get_lastModified => crate::props::get_last_modified,
    ns_document_get_xmlVersion => crate::props::get_xml_version,
    ns_document_get_hidden => crate::props::get_hidden,
    ns_document_get_visibilityState => crate::props::get_visibility_state,
    ns_document_get_compatMode => crate::props::get_compat_mode,
}

setters! {
    ns_document_set_body => crate::elements::set_body,
    ns_document_set_title => crate::props::set_title,
    ns_document_set_dir => crate::props::set_dir,
    ns_document_set_cookie => crate::cookie::set_cookie,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_get_color(
    ctx: *mut JSContext,
    this_val: JSValue,
    magic: c_int,
) -> JSValue {
    unsafe {
        quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), |scope, _, _| {
            crate::props::get_color(scope, magic)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_set_color(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
    magic: c_int,
) -> JSValue {
    let mut argv = [val];
    unsafe {
        quickjs::call_native(ctx, this_val, 1, argv.as_mut_ptr(), |scope, _, args| {
            crate::props::set_color(scope, &args[0], magic)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_seed_cookies_from_jar(js: *mut NsJs) {
    if !js.is_null() {
        crate::cookie::seed_from_jar(Js(js));
    }
}
