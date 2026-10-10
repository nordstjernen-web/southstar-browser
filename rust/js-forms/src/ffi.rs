//! Southstar — the C ABI of the forms bindings as declared in src/js_internal.h, and the js.c and DOM calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean, GError, GPtrArray};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{Scope, Value};

use crate::{Element, JsResult};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy)]
pub(crate) struct Page(*mut NsJs);

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_make_element(ctx: *mut JSContext, node: *const NsNode) -> JSValue;
    fn ns_throw_dom_exception(
        ctx: *mut JSContext,
        name: *const c_char,
        code: c_int,
        message: *const c_char,
    ) -> JSValue;
    fn ns_js_value_is_form_data(ctx: *mut JSContext, v: JSValue) -> GBoolean;
    fn ns_form_data_construct(ctx: *mut JSContext, new_target: JSValue) -> JSValue;
    fn ns_blob_bytes_as_string(
        ctx: *mut JSContext,
        blob: JSValue,
        out_len: *mut usize,
    ) -> *mut c_char;
    fn ns_multipart_boundary() -> *mut c_char;
    fn ns_js_pattern_context() -> *mut JSContext;
    fn ns_js_current_document(js: *const NsJs) -> *const NsNode;
    fn ns_js_main_context(js: *const NsJs) -> *mut JSContext;
    fn ns_js_events_suspended(js: *const NsJs) -> GBoolean;
    fn ns_js_node_in_page(js: *mut NsJs, node: *const NsNode) -> GBoolean;
    fn ns_node_sandbox_blocks_forms(node: *const NsNode) -> GBoolean;
    fn ns_js_log_line(js: *mut NsJs, line: *const c_char);
    fn ns_js_submit_form(js: *mut NsJs, form: *const NsNode, submitter: *const NsNode);
    fn ns_js_dialog_close(js: *mut NsJs, dialog: *mut NsNode, return_value: *const c_char);
    fn ns_js_mark_mutated(js: *mut NsJs);
    fn ns_js_clear_children(js: *mut NsJs, node: *mut NsNode);
    fn ns_js_dispatch_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_make_event(ctx: *mut JSContext, kind: *const c_char, target: *const NsNode) -> JSValue;
    fn ns_js_dispatch_built_event(
        js: *mut NsJs,
        target: *const NsNode,
        kind: *const c_char,
        event: JSValue,
        default_prevented: *mut GBoolean,
    ) -> GBoolean;
    fn ns_event_ctor(
        ctx: *mut JSContext,
        this_val: JSValue,
        argc: c_int,
        argv: *mut JSValue,
    ) -> JSValue;
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_element_remove_attr(el: *mut NsNode, name: *const c_char);
    fn ns_node_new_text(text: *mut c_char) -> *mut NsNode;
    fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode);
    fn ns_url_is_valid_absolute(url: *const c_char) -> GBoolean;
    fn g_regex_new(
        pattern: *const c_char,
        compile_options: c_uint,
        match_options: c_uint,
        error: *mut *mut GError,
    ) -> *mut c_void;
    fn g_regex_match(
        regex: *const c_void,
        string: *const c_char,
        match_options: c_uint,
        match_info: *mut *mut c_void,
    ) -> GBoolean;
    fn g_regex_unref(regex: *mut c_void);
}

pub(crate) fn cstring(bytes: &[u8]) -> CString {
    CString::new(crate::until_nul(bytes.to_vec())).unwrap_or_default()
}

fn node(ptr: *const NsNode) -> Option<Element> {
    unsafe { Node::from_ptr(ptr) }
}

fn mut_ptr(node: Element) -> *mut NsNode {
    node.as_ptr().cast_mut()
}

pub(crate) fn element(value: &Value) -> Option<Element> {
    node(unsafe { ns_unwrap_element(quickjs::raw(value)) })
}

pub(crate) fn wrap(scope: &mut Scope<'_>, node: Option<Element>) -> Value {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_make_element(ctx, Node::ptr_or_null(node)) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn not_found(scope: &mut Scope<'_>, message: &str) -> Value {
    let ctx = quickjs::raw_context(scope);
    let message = cstring(message.as_bytes());
    unsafe { ns_throw_dom_exception(ctx, c"NotFoundError".as_ptr(), 8, message.as_ptr()) };
    quickjs::take_exception(scope)
}

pub(crate) fn is_form_data(scope: &mut Scope<'_>, value: &Value) -> bool {
    let ctx = quickjs::raw_context(scope);
    unsafe { ns_js_value_is_form_data(ctx, quickjs::raw(value)) != 0 }
}

pub(crate) fn construct_form_data(scope: &mut Scope<'_>, new_target: &Value) -> JsResult {
    let ctx = quickjs::raw_context(scope);
    let raw = unsafe { ns_form_data_construct(ctx, quickjs::raw(new_target)) };
    quickjs::checked(scope, unsafe { quickjs::take_value(scope, raw) })
}

pub(crate) fn blob_bytes(scope: &mut Scope<'_>, blob: &Value) -> Vec<u8> {
    let ctx = quickjs::raw_context(scope);
    let mut len = 0usize;
    let data = unsafe { ns_blob_bytes_as_string(ctx, quickjs::raw(blob), &mut len) };
    if data.is_null() {
        return Vec::new();
    }
    let bytes = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), len) }.to_vec();
    unsafe { glib::g_free(data.cast()) };
    bytes
}

pub(crate) fn multipart_boundary() -> Vec<u8> {
    let raw = unsafe { ns_multipart_boundary() };
    let bytes = unsafe { glib::bytes(raw) }.unwrap_or_default().to_vec();
    unsafe { glib::g_free(raw.cast()) };
    bytes
}

pub(crate) fn with_pattern_scope<R>(f: impl FnOnce(&mut Scope<'_>) -> Option<R>) -> Option<R> {
    let ctx = unsafe { ns_js_pattern_context() };
    if ctx.is_null() {
        return None;
    }
    unsafe { quickjs::with_context(ctx, f) }
}

pub(crate) fn regex_matches(pattern: &[u8], value: &[u8]) -> Option<bool> {
    let pattern = cstring(pattern);
    let value = cstring(value);
    let mut error = ptr::null_mut();
    let regex = unsafe { g_regex_new(pattern.as_ptr(), 0, 0, &mut error) };
    if regex.is_null() {
        if !error.is_null() {
            unsafe { glib::g_error_free(error) };
        }
        return None;
    }
    let matched = unsafe { g_regex_match(regex, value.as_ptr(), 0, ptr::null_mut()) != 0 };
    unsafe { g_regex_unref(regex) };
    Some(matched)
}

pub(crate) fn url_is_valid_absolute(url: &[u8]) -> bool {
    let url = cstring(url);
    unsafe { ns_url_is_valid_absolute(url.as_ptr()) != 0 }
}

pub(crate) fn sandbox_blocks_forms(node: Element) -> bool {
    unsafe { ns_node_sandbox_blocks_forms(node.as_ptr()) != 0 }
}

pub(crate) fn set_attr(node: Element, name: &CStr, value: &[u8]) {
    let value = cstring(value);
    unsafe { ns_element_set_attr(mut_ptr(node), name.as_ptr(), value.as_ptr()) };
}

pub(crate) fn remove_attr(node: Element, name: &CStr) {
    unsafe { ns_element_remove_attr(mut_ptr(node), name.as_ptr()) };
}

pub(crate) fn append_text(parent: Element, text: &[u8]) {
    let text = cstring(text);
    let child = unsafe { ns_node_new_text(glib::g_strdup(text.as_ptr())) };
    unsafe { ns_node_append_child(mut_ptr(parent), child) };
}

pub(crate) fn event_ctor(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    quickjs::call_c_function(scope, ns_event_ctor, this, args)
}

impl Page {
    pub(crate) fn of(scope: &Scope<'_>) -> Option<Page> {
        let js = quickjs::context_opaque(scope).cast::<NsJs>();
        (!js.is_null()).then_some(Page(js))
    }

    fn raw(page: Option<Page>) -> *mut NsJs {
        page.map_or(ptr::null_mut(), |p| p.0)
    }

    pub(crate) fn current_document(self) -> Option<Element> {
        node(unsafe { ns_js_current_document(self.0) })
    }

    pub(crate) fn node_in_page(self, node: Element) -> bool {
        unsafe { ns_js_node_in_page(self.0, node.as_ptr()) != 0 }
    }

    pub(crate) fn log_line(self, line: &CStr) {
        unsafe { ns_js_log_line(self.0, line.as_ptr()) };
    }

    pub(crate) fn submit_form(self, form: Element, submitter: Option<Element>) {
        unsafe { ns_js_submit_form(self.0, form.as_ptr(), Node::ptr_or_null(submitter)) };
    }

    pub(crate) fn close_dialog(page: Option<Page>, dialog: Element, return_value: Option<&CStr>) {
        let value = return_value.map_or(ptr::null(), CStr::as_ptr);
        unsafe { ns_js_dialog_close(Page::raw(page), mut_ptr(dialog), value) };
    }

    pub(crate) fn mark_mutated(page: Option<Page>) {
        unsafe { ns_js_mark_mutated(Page::raw(page)) };
    }

    pub(crate) fn clear_children(page: Option<Page>, node: Element) {
        unsafe { ns_js_clear_children(Page::raw(page), mut_ptr(node)) };
    }

    pub(crate) fn dispatch_event(self, target: Element, kind: &CStr) -> bool {
        let mut prevented: GBoolean = 0;
        unsafe { ns_js_dispatch_event(self.0, target.as_ptr(), kind.as_ptr(), &mut prevented) };
        prevented != 0
    }

    pub(crate) fn dispatch_submit(self, form: Element, submitter: Option<Element>) -> (bool, bool) {
        if unsafe { ns_js_events_suspended(self.0) } != 0 {
            return (false, false);
        }
        let ctx = unsafe { ns_js_main_context(self.0) };
        let event = unsafe {
            quickjs::with_context(ctx, |scope| {
                let raw = ns_make_event(ctx, c"submit".as_ptr(), form.as_ptr());
                let event = quickjs::take_value(scope, raw);
                let submitter = wrap(scope, submitter);
                let _ = scope.set(&event, "submitter", submitter);
                quickjs::into_raw(event)
            })
        };
        let mut prevented: GBoolean = 0;
        let dispatched = unsafe {
            ns_js_dispatch_built_event(
                self.0,
                form.as_ptr(),
                c"submit".as_ptr(),
                event,
                &mut prevented,
            )
        };
        (dispatched != 0, prevented != 0)
    }
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

unsafe fn getter(
    ctx: *mut JSContext,
    this_val: JSValue,
    f: fn(&mut Scope<'_>, &Value, &[Value]) -> JsResult,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), f) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_window_form_data_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::form_data::construct) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_install_form_data(ctx: *mut JSContext, global: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = quickjs::borrow_value(scope, global);
            crate::form_data::install(scope, &global);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_form_data_serialize(
    ctx: *mut JSContext,
    fd: JSValue,
    out_len: *mut usize,
    out_content_type: *mut *mut c_char,
) -> *mut c_char {
    let (body, content_type) = unsafe {
        quickjs::with_context(ctx, |scope| {
            let fd = quickjs::borrow_value(scope, fd);
            crate::form_data::serialize(scope, &fd)
        })
    };
    if !out_content_type.is_null() {
        unsafe { *out_content_type = glib::strdup(&content_type) };
    }
    if !out_len.is_null() {
        unsafe { *out_len = body.len() };
    }
    let out = unsafe { glib::g_malloc(body.len() + 1) }.cast::<u8>();
    unsafe {
        ptr::copy_nonoverlapping(body.as_ptr(), out, body.len());
        *out.add(body.len()) = 0;
    }
    out.cast()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_listed_controls(
    form: *const NsNode,
    include_image: GBoolean,
    out: *mut GPtrArray,
) {
    let (Some(form), false) = (node(form), out.is_null()) else {
        return;
    };
    for control in crate::listed_controls(form, include_image != 0) {
        unsafe { glib::g_ptr_array_add(out, mut_ptr(control).cast()) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_validity_get_valid(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe { getter(ctx, this_val, crate::validity::valid) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_validity(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, crate::validity::validity) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_validation_message(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, crate::validity::validation_message) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_will_validate(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    unsafe { getter(ctx, this_val, crate::validity::will_validate_getter) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_check_validity(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::validity::check_validity) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_setCustomValidity(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe {
        native(
            ctx,
            this_val,
            argc,
            argv,
            crate::validity::set_custom_validity,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_submit_trigger(el: *const NsNode) -> GBoolean {
    glib::boolean(node(el).is_some_and(crate::submit::is_submit_trigger))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_reset_trigger(el: *const NsNode) -> GBoolean {
    glib::boolean(node(el).is_some_and(crate::submit::is_reset_trigger))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_request_submit_form(
    ctx: *mut JSContext,
    form: *const NsNode,
    submitter: *const NsNode,
) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            crate::submit::request_submit(scope, node(form), node(submitter));
            quickjs::into_raw(Value::undefined())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_reset_form(ctx: *mut JSContext, form: *mut NsNode) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            crate::submit::reset_form(scope, node(form));
            quickjs::into_raw(Value::undefined())
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_dispatch_submit_event(
    js: *mut NsJs,
    form: *const NsNode,
    submitter: *const NsNode,
    default_prevented: *mut GBoolean,
) -> GBoolean {
    if !default_prevented.is_null() {
        unsafe { *default_prevented = 0 };
    }
    let (false, Some(form)) = (js.is_null(), node(form)) else {
        return glib::FALSE;
    };
    let (dispatched, prevented) = Page(js).dispatch_submit(form, node(submitter));
    if !default_prevented.is_null() {
        unsafe { *default_prevented = glib::boolean(prevented) };
    }
    glib::boolean(dispatched)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_form_requestSubmit(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe {
        native(
            ctx,
            this_val,
            argc,
            argv,
            crate::submit::request_submit_method,
        )
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_form_submit(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::submit::submit_method) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_form_reset(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::submit::reset_method) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_submit_event_ctor(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
) -> JSValue {
    unsafe { native(ctx, this_val, argc, argv, crate::submit::submit_event_ctor) }
}
