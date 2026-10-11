//! Southstar — the few natives the DOM function tables point at directly: getNumberOfChars, accessKeyLabel, the type setter, hasFeature and the document's forms, images, all and links collections.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};

use southstar_dom::{Node, NsNode};
use southstar_glib as glib;
use southstar_js_engine::Value;
use southstar_js_engine::quickjs::{self, JSContext, JSValue};

const LIVE_DOC_TAG: c_int = 3;
const LIVE_LINKS: c_int = 10;

unsafe extern "C" {
    fn ns_unwrap_element(v: JSValue) -> *const NsNode;
    fn ns_node_collect_text(root: *const NsNode) -> *mut c_char;
    fn ns_element_reflect_str_set(
        ctx: *mut JSContext,
        this_val: JSValue,
        value: JSValue,
        attr: *const c_char,
    ) -> JSValue;
    fn ns_input_resanitize_value(el: *mut NsNode);
    fn ns_make_live(
        ctx: *mut JSContext,
        owner: JSValue,
        kind: c_int,
        param: *const c_char,
    ) -> JSValue;
    fn JS_SetIsHTMLDDA(ctx: *mut JSContext, obj: JSValue);
    fn JS_NewStringLen(ctx: *mut JSContext, str1: *const c_char, len1: usize) -> JSValue;
}

fn int(value: i32) -> JSValue {
    quickjs::into_raw(Value::int(value))
}

fn utf8_char_count(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| b & 0xc0 != 0x80).count()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_getNumberOfChars(
    _ctx: *mut JSContext,
    this_val: JSValue,
    _argc: c_int,
    _argv: *mut JSValue,
) -> JSValue {
    let node = unsafe { ns_unwrap_element(this_val) };
    if node.is_null() {
        return int(0);
    }
    let text = unsafe { ns_node_collect_text(node) };
    let count = if text.is_null() {
        0
    } else {
        let count = utf8_char_count(unsafe { CStr::from_ptr(text) }.to_bytes());
        unsafe { glib::g_free(text.cast()) };
        count
    };
    int(count as i32)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_accessKeyLabel(
    ctx: *mut JSContext,
    this_val: JSValue,
) -> JSValue {
    let key = unsafe { Node::from_ptr(ns_unwrap_element(this_val)) }
        .and_then(|node| node.attr(c"accesskey"))
        .map(CStr::to_bytes)
        .filter(|key| core::str::from_utf8(key).is_ok_and(|key| key.chars().count() == 1))
        .unwrap_or_default();
    unsafe { JS_NewStringLen(ctx, key.as_ptr().cast(), key.len()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_set_type(
    ctx: *mut JSContext,
    this_val: JSValue,
    value: JSValue,
) -> JSValue {
    let result = unsafe { ns_element_reflect_str_set(ctx, this_val, value, c"type".as_ptr()) };
    let node = unsafe { ns_unwrap_element(this_val) };
    if !node.is_null() {
        unsafe { ns_input_resanitize_value(node.cast_mut()) };
    }
    result
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_true(
    _ctx: *mut JSContext,
    _this_val: JSValue,
    _argc: c_int,
    _argv: *mut JSValue,
) -> JSValue {
    quickjs::into_raw(Value::boolean(true))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_event_false(
    _ctx: *mut JSContext,
    _this_val: JSValue,
    _argc: c_int,
    _argv: *mut JSValue,
) -> JSValue {
    quickjs::into_raw(Value::boolean(false))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_get_forms(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe { ns_make_live(ctx, this_val, LIVE_DOC_TAG, c"form".as_ptr()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_get_images(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe { ns_make_live(ctx, this_val, LIVE_DOC_TAG, c"img".as_ptr()) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_get_all(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    let all = unsafe { ns_make_live(ctx, this_val, LIVE_DOC_TAG, c"*".as_ptr()) };
    unsafe { JS_SetIsHTMLDDA(ctx, all) };
    all
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_document_get_links(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe { ns_make_live(ctx, this_val, LIVE_LINKS, core::ptr::null()) }
}
