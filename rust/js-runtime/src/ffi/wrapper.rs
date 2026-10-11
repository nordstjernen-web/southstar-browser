//! Southstar — node wrappers: the Element host class whose instances point straight at their DOM node, wrapper creation and caching, invalidation when a node is freed, and the form element's named and indexed controls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Kind, Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{
    Attributes, ClassHooks, HostClass, OwnProperty, PropertyDefinition, PropertyKey, Scope, Value,
};

use super::page_object::{ns_active_js, ns_js_kind_proto};
use super::{Js, NsJs};

const FLAG_FRAGMENT: u32 = 1 << 2;

unsafe extern "C" {
    fn ns_realm_cloners_made(js: *const NsJs) -> GBoolean;
    fn ns_js_node_realm_context(js: *const c_void, node: *const NsNode) -> *mut JSContext;
    fn ns_realm_proto_for(js: *mut NsJs, realm: *mut JSContext, proto: JSValue) -> JSValue;
    fn ns_install_body_reflected_handlers(ctx: *mut JSContext, wrapper: JSValue);
    fn ns_mut_scrub_node(js: *const NsJs, node: *const NsNode);
    fn ns_attr_detach_owner(js: *mut NsJs, owner: *mut NsNode);
    fn ns_attribute_map_release_owner(js: *mut NsJs, owner: *const NsNode);
    fn ns_js_unorphan_node(js: *mut NsJs, n: *mut NsNode);
    fn ns_js_images_forget_node(js: *mut NsJs, node: *const NsNode);
    fn ns_focus_forget_node(js: *const NsJs, n: *const NsNode);
    fn ns_js_forget_pending_change_for(js: *mut NsJs, el: *const NsNode);
    fn ns_parser_hold_forget(js: *mut NsJs, n: *const NsNode);
    fn ns_js_frames_forget_node(js: *mut NsJs, node: *const NsNode);
    fn ns_top_layer_forget_node(js: *const NsJs, n: *const NsNode);
    fn ns_dispatch_forget_node(js: *mut NsJs, node: *const NsNode);
    fn ns_form_indexed_control(ctx: *mut JSContext, form: JSValue, index: u32) -> JSValue;
    fn ns_form_named_control(ctx: *mut JSContext, form: JSValue, name: *const c_char) -> JSValue;
    fn JS_FreeValue(ctx: *mut JSContext, v: JSValue);
}

pub(crate) static ELEMENT_CLASS: HostClass = HostClass::new_raw(
    c"Element",
    ClassHooks {
        get_own_property: Some(named_get_own),
        delete: Some(named_delete),
        define_own_property: Some(named_define),
        ..ClassHooks::NONE
    },
    finalize_element,
);

fn node<'a>(p: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(p) }
}

fn finalize_element(data: *mut c_void) {
    if let Some(n) = node(data.cast()) {
        n.set_js_wrapper(ptr::null_mut());
        n.set_js_invalidate(None);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_register_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            scope.register_class(&ELEMENT_CLASS);
            scope.set_class_prototype(&ELEMENT_CLASS, proto);
        })
    }
}

pub(crate) fn unwrap(value: &Value) -> Option<Node<'static>> {
    node(value.raw_instance(&ELEMENT_CLASS).cast())
}

fn form_of(value: &Value) -> bool {
    unwrap(value).is_some_and(|n| {
        matches!(n.kind(), Kind::Element) && n.name().is_some_and(|name| name.to_bytes() == b"form")
    })
}

fn is_array_index(name: &[u8]) -> bool {
    if name.is_empty() || !name.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if name[0] == b'0' && name.len() > 1 {
        return false;
    }
    core::str::from_utf8(name)
        .ok()
        .and_then(|digits| digits.parse::<u64>().ok())
        .is_some_and(|index| index <= 4_294_967_294)
}

fn reserved(name: &[u8]) -> bool {
    name.is_empty() || name.starts_with(b"_ns_") || name.starts_with(b"__")
}

fn named_get_own(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
) -> Result<Option<OwnProperty>, Value> {
    if !form_of(obj) || !key.to_value(scope).is_string() {
        return Ok(None);
    }
    let Some(name) = key.to_c_bytes(scope) else {
        return Ok(None);
    };
    if reserved(&name) {
        return Ok(None);
    }
    let ctx = quickjs::raw_context(scope);
    if is_array_index(&name) {
        let index = core::str::from_utf8(&name)
            .ok()
            .and_then(|digits| digits.parse::<u64>().ok())
            .unwrap_or(0) as u32;
        let raw = unsafe { ns_form_indexed_control(ctx, quickjs::raw(obj), index) };
        let control = quickjs::checked(scope, unsafe { quickjs::take_value(scope, raw) })?;
        if control.is_undefined() {
            return Ok(None);
        }
        return Ok(Some(OwnProperty::data(control, false, true, true)));
    }
    let name = CString::new(name).unwrap_or_default();
    let raw = unsafe { ns_form_named_control(ctx, quickjs::raw(obj), name.as_ptr()) };
    let control = quickjs::checked(scope, unsafe { quickjs::take_value(scope, raw) })?;
    if control.is_null() || control.is_undefined() {
        return Ok(None);
    }
    Ok(Some(OwnProperty::data(control, false, false, true)))
}

fn named_delete(scope: &mut Scope<'_>, obj: &Value, key: &PropertyKey) -> Result<bool, Value> {
    if !form_of(obj) {
        return Ok(true);
    }
    let Some(name) = key.to_c_bytes(scope) else {
        return Ok(true);
    };
    if reserved(&name) {
        return Ok(true);
    }
    Ok(!matches!(named_get_own(scope, obj, key), Ok(Some(_))))
}

fn named_define(
    scope: &mut Scope<'_>,
    obj: &Value,
    key: &PropertyKey,
    definition: &PropertyDefinition,
) -> Result<bool, Value> {
    if form_of(obj)
        && let Some(name) = key.to_c_bytes(scope)
        && !reserved(&name)
        && matches!(named_get_own(scope, obj, key), Ok(Some(_)))
    {
        return Err(scope.type_error("Cannot define property on form"));
    }
    scope.define_ordinary(obj, key, definition)
}

fn page_document_object(js: Js) -> Value {
    let main = js.main_realm();
    let ctx = if main.is_null() { js.ctx() } else { main };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let global = scope.global();
            match scope.get(&global, "document") {
                Ok(document) => document,
                Err(error) => {
                    let raw = quickjs::result_raw(scope, Err(error));
                    quickjs::take_value(scope, raw)
                }
            }
        })
    }
}

fn define_doctype(scope: &mut Scope<'_>, obj: &Value, n: Node<'_>) {
    let mut public_id: &[u8] = b"";
    let mut system_id: &[u8] = b"";
    for attr in n.attrs() {
        let Some(name) = attr.name() else {
            continue;
        };
        let value = attr.value().map_or(&b""[..], CStr::to_bytes);
        match name.to_bytes() {
            b"publicId" => public_id = value,
            b"systemId" => system_id = value,
            _ => {}
        }
    }
    let name = n.name().map_or(&b""[..], CStr::to_bytes);
    for (key, text) in [
        ("name", name),
        ("publicId", public_id),
        ("systemId", system_id),
    ] {
        let text = scope.string_from_bytes(text);
        let _ = scope.define(obj, key, text, Attributes::ENUMERABLE);
    }
}

fn kind_proto(scope: &mut Scope<'_>, js: Option<Js>, n: Node<'_>) -> Value {
    let js_ptr = js.map_or(ptr::null_mut(), Js::ptr);
    let mut proto = unsafe { ns_js_kind_proto(js_ptr, n.as_ptr()) };
    let ctx = quickjs::raw_context(scope);
    let node_realm = match js {
        Some(js) if unsafe { ns_realm_cloners_made(js.ptr()) } != 0 => {
            let realm = unsafe { ns_js_node_realm_context(js.ptr().cast(), n.as_ptr()) };
            if realm.is_null() {
                let root = core::iter::successors(Some(n), |n| n.parent())
                    .last()
                    .unwrap_or(n);
                if !matches!(root.kind(), Kind::Document) || root.flags() & FLAG_FRAGMENT != 0 {
                    ctx
                } else {
                    ptr::null_mut()
                }
            } else {
                realm
            }
        }
        _ => ptr::null_mut(),
    };
    if !node_realm.is_null() && quickjs::raw_is_object(proto) {
        proto = unsafe { ns_realm_proto_for(js_ptr, node_realm, proto) };
    }
    unsafe { quickjs::borrow_value(scope, proto) }
}

fn make(scope: &mut Scope<'_>, n: Node<'static>) -> Result<Value, Value> {
    let ctx = quickjs::raw_context(scope);
    let js = Js::of_context(ctx);
    if let Some(js) = js
        && ptr::eq(n.as_ptr(), unsafe { *js.st().slots.ce_main_doc })
        && n.parent().is_none()
    {
        let document = page_document_object(js);
        if !document.is_undefined() && !document.is_null() {
            return quickjs::checked(scope, document);
        }
    }
    let wrapper = n.js_wrapper();
    if !wrapper.is_null() {
        return Ok(quickjs::object_from_identity(scope, wrapper));
    }
    let obj = unsafe { scope.new_raw_instance(&ELEMENT_CLASS, None, n.as_mut_ptr().cast()) }?;
    n.set_js_wrapper(obj.object_identity());
    n.set_js_invalidate(Some(ns_invalidate_wrapper));
    let proto = kind_proto(scope, js, n);
    if proto.is_object() {
        let _ = scope.set_prototype(&obj, &proto);
    }
    if matches!(n.kind(), Kind::Doctype) {
        define_doctype(scope, &obj, n);
    }
    if matches!(n.kind(), Kind::Element)
        && n.name()
            .is_some_and(|name| matches!(name.to_bytes(), b"body" | b"frameset"))
    {
        unsafe { ns_install_body_reflected_handlers(ctx, quickjs::raw(&obj)) };
    }
    if let Some(js) = js {
        let pinned = unsafe { *js.st().slots.pinned_wrappers_set };
        if !pinned.is_null() {
            core::mem::forget(obj.clone());
            unsafe { glib::g_hash_table_add(pinned, n.as_mut_ptr().cast()) };
        }
    }
    Ok(obj)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_element(ctx: *mut JSContext, cnode: *const NsNode) -> JSValue {
    let Some(n) = node(cnode) else {
        return quickjs::into_raw(Value::null());
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let element = make(scope, n);
            quickjs::result_raw(scope, element)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_unwrap_element(value: JSValue) -> *const NsNode {
    quickjs::raw_instance(value, &ELEMENT_CLASS).cast()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_element_class_id() -> u32 {
    quickjs::class_id(&ELEMENT_CLASS)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_invalidate_wrapper(n: *mut NsNode) {
    let Some(target) = node(n) else {
        return;
    };
    let js = unsafe { ns_active_js() };
    unsafe {
        ns_mut_scrub_node(js, n);
        ns_attr_detach_owner(js, n);
        ns_attribute_map_release_owner(js, n);
    }
    let wrapper = target.js_wrapper();
    if !wrapper.is_null() {
        unsafe { quickjs::set_identity_opaque(wrapper, ptr::null_mut()) };
        target.set_js_wrapper(ptr::null_mut());
        if let Some(page) = Js::of(js) {
            let ctx = page.ctx();
            let pinned = unsafe { *page.st().slots.pinned_wrappers_set };
            if !ctx.is_null()
                && !pinned.is_null()
                && unsafe { glib::g_hash_table_remove(pinned, n.cast()) } != 0
            {
                unsafe { JS_FreeValue(ctx, quickjs::identity_raw(wrapper)) };
            }
        }
    }
    if !js.is_null() {
        unsafe {
            ns_js_unorphan_node(js, n);
            ns_js_images_forget_node(js, n);
            ns_focus_forget_node(js, n);
            ns_js_forget_pending_change_for(js, n);
            ns_parser_hold_forget(js, n);
            ns_js_frames_forget_node(js, n);
            ns_top_layer_forget_node(js, n);
        }
    }
    target.set_js_invalidate(None);
    if !js.is_null() {
        unsafe { ns_dispatch_forget_node(js, n) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_arm_js_invalidate(n: *mut NsNode) {
    if let Some(target) = node(n)
        && target.js_invalidate().is_none()
    {
        target.set_js_invalidate(Some(ns_invalidate_wrapper));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_js_new_document_object(js: *mut NsJs) -> JSValue {
    let Some(page) = Js::of(js) else {
        return quickjs::UNDEFINED;
    };
    let current_doc = unsafe { *page.st().slots.current_doc };
    unsafe {
        quickjs::with_context(page.ctx(), |scope| {
            let document = scope.new_raw_instance(&ELEMENT_CLASS, None, current_doc.cast());
            quickjs::result_raw(scope, document)
        })
    }
}
