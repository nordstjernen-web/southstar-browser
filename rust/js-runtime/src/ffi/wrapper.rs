//! Southstar — node wrappers: the Element host class whose instances point straight at their DOM node, wrapper creation and caching, invalidation when a node is freed, and the form element's named and indexed controls.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::{CStr, c_void};
use core::ptr;

use southstar_dom::{Kind, Node, NsNode, WeakNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{
    Attributes, ClassHooks, ContextHandle, HostClass, OwnProperty, PropertyDefinition, PropertyKey,
    Scope, Value,
};

use super::{Js, NsJs, imports};

const FLAG_FRAGMENT: u32 = 1 << 2;

unsafe extern "C" {
    fn ns_realm_cloners_made(js: *const NsJs) -> GBoolean;
    fn ns_js_node_realm_context(js: *const c_void, node: *const NsNode) -> ContextHandle;
    fn ns_mut_scrub_node(js: *const NsJs, node: *const NsNode);
    fn ns_attr_detach_owner(js: *mut NsJs, owner: *mut NsNode);
    fn ns_attribute_map_release_owner(js: *mut NsJs, owner: *const NsNode);
    fn ns_js_unorphan_node(js: *mut NsJs, n: *mut NsNode);
    fn ns_focus_forget_node(js: *const NsJs, n: *const NsNode);
    fn ns_js_forget_pending_change_for(js: *mut NsJs, el: *const NsNode);
    fn ns_parser_hold_forget(js: *mut NsJs, n: *const NsNode);
    fn ns_js_frames_forget_node(js: *mut NsJs, node: *const NsNode);
    fn ns_top_layer_forget_node(js: *const NsJs, n: *const NsNode);
    fn ns_dispatch_forget_node(js: *mut NsJs, node: *const NsNode);
}

pub(crate) static ELEMENT_CLASS: HostClass = HostClass::new(
    c"Element",
    ClassHooks {
        get_own_property: Some(named_get_own),
        delete: Some(named_delete),
        define_own_property: Some(named_define),
        ..ClassHooks::NONE
    },
);

pub(crate) struct ElementData {
    node: Cell<Option<WeakNode>>,
}

impl ElementData {
    fn new(node: Option<Node<'_>>) -> ElementData {
        ElementData {
            node: Cell::new(node.map(Node::downgrade)),
        }
    }

    pub(crate) fn node(&self) -> Option<Node<'static>> {
        self.node.get().and_then(WeakNode::get)
    }

    pub(crate) fn unbind(&self) {
        self.node.set(None);
    }
}

impl Drop for ElementData {
    fn drop(&mut self) {
        if let Some(n) = self.node() {
            n.set_js_wrapper(ptr::null_mut());
            n.set_js_invalidate(None);
        }
    }
}

fn node<'a>(p: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(p) }
}

pub(crate) fn register_element_class(scope: &mut Scope<'_>, proto: Value) {
    scope.register_class(&ELEMENT_CLASS);
    scope.set_class_prototype(&ELEMENT_CLASS, proto);
}

pub(crate) fn unwrap(value: &Value) -> Option<Node<'static>> {
    value
        .with_instance(&ELEMENT_CLASS, ElementData::node)
        .flatten()
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
    if is_array_index(&name) {
        let index = core::str::from_utf8(&name)
            .ok()
            .and_then(|digits| digits.parse::<u64>().ok())
            .unwrap_or(0) as u32;
        let control = imports::nsr_form_indexed_control(scope, obj, index)?;
        if control.is_undefined() {
            return Ok(None);
        }
        return Ok(Some(OwnProperty::data(control, false, true, true)));
    }
    let control = imports::nsr_form_named_control(scope, obj, &name)?;
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

fn page_document_object(js: Js) -> Result<Value, Value> {
    js.main_or_current()
        .with(|scope| {
            let global = scope.global();
            scope.get(&global, "document")
        })
        .unwrap_or_else(|| Ok(Value::undefined()))
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
    let proto = unsafe { super::record::kind_proto_value(js_ptr, n.as_ptr()) };
    let proto = proto.unwrap_or_else(Value::undefined);
    let node_realm = match js {
        Some(js) if unsafe { ns_realm_cloners_made(js.ptr()) } != 0 => {
            let realm = unsafe { ns_js_node_realm_context(js.ptr().cast(), n.as_ptr()) };
            if realm.is_null() {
                let root = core::iter::successors(Some(n), |n| n.parent())
                    .last()
                    .unwrap_or(n);
                if !matches!(root.kind(), Kind::Document) || root.flags() & FLAG_FRAGMENT != 0 {
                    scope.handle()
                } else {
                    ContextHandle::NULL
                }
            } else {
                realm
            }
        }
        _ => ContextHandle::NULL,
    };
    if !node_realm.is_null() && proto.is_object() {
        return unsafe { imports::nsr_realm_proto_for(js_ptr, node_realm, &proto) };
    }
    proto
}

pub(crate) fn make(scope: &mut Scope<'_>, n: Node<'static>) -> Result<Value, Value> {
    let js = Js::of_scope(scope);
    if let Some(js) = js
        && ptr::eq(n.as_ptr(), unsafe { *js.st().slots.ce_main_doc })
        && n.parent().is_none()
    {
        let document = page_document_object(js)?;
        if !document.is_undefined() && !document.is_null() {
            return Ok(document);
        }
    }
    let wrapper = n.js_wrapper();
    if !wrapper.is_null() {
        return Ok(scope.object_from_identity(wrapper));
    }
    let obj = scope.new_instance_with(&ELEMENT_CLASS, None, ElementData::new(Some(n)))?;
    n.set_js_wrapper(obj.object_identity());
    n.set_js_invalidate(Some(invalidate_wrapper));
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
        imports::nsr_install_body_reflected_handlers(scope, &obj);
    }
    if let Some(js) = js {
        let pinned = unsafe { *js.st().slots.pinned_wrappers_set };
        if !pinned.is_null() {
            scope.retain_identity(&obj);
            unsafe { glib::g_hash_table_add(pinned, n.as_mut_ptr().cast()) };
        }
    }
    Ok(obj)
}

pub(crate) fn make_element(scope: &mut Scope<'_>, cnode: *const NsNode) -> Result<Value, Value> {
    match node(cnode) {
        Some(n) => make(scope, n),
        None => Ok(Value::null()),
    }
}

fn invalidate_wrapper(target: Node<'static>) {
    let n = target.as_mut_ptr();
    let js = super::record::active();
    unsafe {
        ns_mut_scrub_node(js, n);
        ns_attr_detach_owner(js, n);
        ns_attribute_map_release_owner(js, n);
    }
    let wrapper = target.js_wrapper();
    if !wrapper.is_null() {
        target.set_js_wrapper(ptr::null_mut());
        if let Some(page) = Js::of(js) {
            let ctx = page.ctx();
            let pinned = unsafe { *page.st().slots.pinned_wrappers_set };
            if !ctx.is_null()
                && !pinned.is_null()
                && unsafe { glib::g_hash_table_remove(pinned, n.cast()) } != 0
            {
                ctx.with(|scope| unsafe { scope.release_identity(wrapper) });
            }
        }
    }
    if !js.is_null() {
        unsafe {
            ns_js_unorphan_node(js, n);
            (crate::api::images_hooks().forget_node)(js, target);
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

pub(crate) fn arm_js_invalidate(target: Node<'_>) {
    if target.js_invalidate().is_none() {
        target.set_js_invalidate(Some(invalidate_wrapper));
    }
}

pub(crate) fn new_document_object(page: Js, scope: &mut Scope<'_>) -> Result<Value, Value> {
    let current_doc = unsafe { *page.st().slots.current_doc };
    scope.new_instance_with(&ELEMENT_CLASS, None, ElementData::new(node(current_doc)))
}
