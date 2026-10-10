//! Southstar — the C ABI of the attribute bindings as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use std::cell::RefCell;
use std::rc::Rc;

use southstar_dom::{Attr, Node, NsAttr, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::quickjs::{self, JSContext, JSValue};
use southstar_js_engine::{NativeFn, Scope, Value};

use crate::attr::{self, AttrState};
use crate::{Element, c_string};

#[repr(C)]
pub(crate) struct NsJs {
    _private: [u8; 0],
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
    fn ns_js_set_attr_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_js_set_attr_ns_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        namespace_uri: *const c_char,
        prefix: *const c_char,
        local_name: *const c_char,
        name: *const c_char,
        value: *const c_char,
    );
    fn ns_js_remove_attr_recorded(js: *mut NsJs, node: *mut NsNode, name: *const c_char);
    fn ns_js_remove_attr_ns_recorded(
        js: *mut NsJs,
        node: *mut NsNode,
        namespace_uri: *const c_char,
        local_name: *const c_char,
    );
    fn ns_token_list_node(obj: JSValue, attr: *mut *const c_char) -> *mut NsNode;
    fn ns_dataset_node(obj: JSValue) -> *mut NsNode;
    fn ns_namedmap_owner(obj: JSValue) -> *mut NsNode;
    fn ns_attr_new_object(ctx: *mut JSContext, state: *mut c_void) -> JSValue;
    fn ns_attr_opaque(obj: JSValue) -> *mut c_void;
    fn ns_attr_apply_proto(ctx: *mut JSContext, obj: JSValue);
    fn ns_valid_element_local_name(name: *const c_char) -> GBoolean;
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
}

fn opt_ptr(text: Option<&CStr>) -> *const c_char {
    text.map_or(ptr::null(), CStr::as_ptr)
}

fn c_str<'a>(text: *const c_char) -> Option<&'a CStr> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) })
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(quickjs::context_opaque(scope).cast())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_unwrap_element(quickjs::raw(value))) }
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    let raw = unsafe { ns_make_element(quickjs::raw_context(scope), node.as_ptr()) };
    unsafe { quickjs::take_value(scope, raw) }
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    let name = c_string(name.as_bytes());
    let message = c_string(message.as_bytes());
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

pub(crate) fn set_attr(js: Js, node: Element, name: &CStr, value: &CStr) {
    unsafe { ns_js_set_attr_recorded(js.0, node.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
}

pub(crate) struct QualifiedName<'a> {
    pub namespace_uri: Option<&'a CStr>,
    pub prefix: Option<&'a CStr>,
    pub local_name: &'a CStr,
    pub name: &'a CStr,
}

pub(crate) fn set_attr_ns(js: Js, node: Element, qname: &QualifiedName<'_>, value: &CStr) {
    unsafe {
        ns_js_set_attr_ns_recorded(
            js.0,
            node.as_mut_ptr(),
            opt_ptr(qname.namespace_uri),
            opt_ptr(qname.prefix),
            qname.local_name.as_ptr(),
            qname.name.as_ptr(),
            value.as_ptr(),
        )
    };
}

pub(crate) fn remove_attr(js: Js, node: Element, name: &CStr) {
    unsafe { ns_js_remove_attr_recorded(js.0, node.as_mut_ptr(), name.as_ptr()) };
}

pub(crate) fn remove_attr_ns(js: Js, node: Element, namespace_uri: Option<&CStr>, local: &CStr) {
    unsafe {
        ns_js_remove_attr_ns_recorded(
            js.0,
            node.as_mut_ptr(),
            opt_ptr(namespace_uri),
            local.as_ptr(),
        )
    };
}

pub(crate) fn token_list_node(value: &Value) -> (Option<Element>, &'static CStr) {
    token_list_node_raw(quickjs::raw(value))
}

fn token_list_node_raw(obj: JSValue) -> (Option<Element>, &'static CStr) {
    let mut attr: *const c_char = ptr::null();
    let node = unsafe { ns_token_list_node(obj, &mut attr) };
    (
        unsafe { Node::from_ptr(node) },
        c_str(attr).unwrap_or(c"class"),
    )
}

fn dataset_node(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_dataset_node(quickjs::raw(value))) }
}

pub(crate) fn named_map_owner(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(ns_namedmap_owner(quickjs::raw(value))) }
}

pub(crate) fn valid_element_local_name(name: &CStr) -> bool {
    unsafe { ns_valid_element_local_name(name.as_ptr()) != 0 }
}

pub(crate) fn doc_base_url(js: Js) -> Option<Vec<u8>> {
    let url = unsafe { ns_js_doc_base_url(js.0) };
    if url.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(url) }.to_bytes().to_vec();
    unsafe { glib::g_free(url.cast()) };
    Some(bytes)
}

pub(crate) fn new_text(js: Js, text: &[u8]) -> Element {
    let node = southstar_dom::node::new_text(
        southstar_dom::node::KIND_TEXT,
        glib::strdup(text),
        text.len() as u32,
    );
    unsafe { ns_js_track_orphan(js.0, node.as_mut_ptr()) };
    node
}

pub(crate) type Shared = Rc<RefCell<AttrState>>;

pub(crate) fn new_attr_object(scope: &mut Scope<'_>, state: Shared) -> Result<Value, Value> {
    let opaque = Rc::into_raw(state).cast_mut().cast::<c_void>();
    let raw = unsafe { ns_attr_new_object(quickjs::raw_context(scope), opaque) };
    let value = unsafe { quickjs::take_value(scope, raw) };
    let checked = quickjs::checked(scope, value);
    if checked.is_err() {
        unsafe { release_state(opaque) };
    }
    checked
}

pub(crate) fn state_of(value: &Value) -> Option<Shared> {
    let opaque = unsafe { ns_attr_opaque(quickjs::raw(value)) }.cast::<RefCell<AttrState>>();
    if opaque.is_null() {
        return None;
    }
    unsafe {
        Rc::increment_strong_count(opaque);
        Some(Rc::from_raw(opaque))
    }
}

unsafe fn release_state(opaque: *mut c_void) {
    if !opaque.is_null() {
        drop(unsafe { Rc::from_raw(opaque.cast::<RefCell<AttrState>>()) });
    }
}

pub(crate) fn apply_attr_proto(scope: &mut Scope<'_>, entry: &Value) {
    unsafe { ns_attr_apply_proto(quickjs::raw_context(scope), quickjs::raw(entry)) };
}

unsafe fn native(
    ctx: *mut JSContext,
    this_val: JSValue,
    argc: c_int,
    argv: *mut JSValue,
    f: NativeFn,
) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, argc, argv, f) }
}

unsafe fn getter(ctx: *mut JSContext, this_val: JSValue, f: NativeFn) -> JSValue {
    unsafe { quickjs::call_native(ctx, this_val, 0, ptr::null_mut(), f) }
}

unsafe fn setter(ctx: *mut JSContext, this_val: JSValue, val: JSValue, f: NativeFn) -> JSValue {
    let mut argv = [val];
    unsafe { quickjs::call_native(ctx, this_val, 1, argv.as_mut_ptr(), f) }
}

unsafe fn with_value<R>(
    ctx: *mut JSContext,
    value: JSValue,
    f: impl FnOnce(&mut Scope<'_>, &Value) -> R,
) -> R {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let value = quickjs::borrow_value(scope, value);
            f(scope, &value)
        })
    }
}

fn strv(names: &[Vec<u8>]) -> *mut *mut c_char {
    let strv = unsafe { glib::g_malloc0((names.len() + 1) * size_of::<*mut c_char>()) }
        .cast::<*mut c_char>();
    for (i, name) in names.iter().enumerate() {
        unsafe { *strv.add(i) = glib::strdup(c_string(name).as_bytes()) };
    }
    strv
}

macro_rules! method {
    ($name:ident, $f:path) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            ctx: *mut JSContext,
            this_val: JSValue,
            argc: c_int,
            argv: *mut JSValue,
        ) -> JSValue {
            unsafe { native(ctx, this_val, argc, argv, $f) }
        }
    };
}

method!(ns_tlist_contains, crate::token_list::contains);
method!(ns_tlist_add, crate::token_list::add);
method!(ns_tlist_remove, crate::token_list::remove);
method!(ns_tlist_toggle, crate::token_list::toggle);
method!(ns_tlist_replace, crate::token_list::replace);
method!(ns_tlist_item, crate::token_list::item);
method!(ns_tlist_supports, crate::token_list::supports);
method!(ns_tlist_toString, crate::token_list::get_value);
method!(ns_namedmap_getNamedItem, crate::named_map::get_named_item);
method!(
    ns_namedmap_getNamedItemNS,
    crate::named_map::get_named_item_ns
);
method!(ns_namedmap_setNamedItem, crate::named_map::set_named_item);
method!(
    ns_namedmap_removeNamedItem,
    crate::named_map::remove_named_item
);
method!(
    ns_namedmap_removeNamedItemNS,
    crate::named_map::remove_named_item_ns
);
method!(ns_namedmap_item, crate::named_map::item);
method!(ns_element_getAttributeNode, crate::attr::get_attribute_node);
method!(
    ns_element_getAttributeNodeNS,
    crate::attr::get_attribute_node_ns
);
method!(ns_element_setAttributeNode, crate::attr::set_attribute_node);
method!(
    ns_element_removeAttributeNode,
    crate::attr::remove_attribute_node
);

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tlist_get_length(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe { getter(ctx, this_val, crate::token_list::get_length) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tlist_get_value(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe { getter(ctx, this_val, crate::token_list::get_value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tlist_set_value(
    ctx: *mut JSContext,
    this_val: JSValue,
    val: JSValue,
) -> JSValue {
    unsafe { setter(ctx, this_val, val, crate::token_list::set_value) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_tlist_named_token(obj: JSValue, name: *const c_char) -> *mut c_char {
    let Some(name) = c_str(name) else {
        return ptr::null_mut();
    };
    let (node, attr) = token_list_node_raw(obj);
    node.and_then(|node| crate::token_list::indexed_token(node, attr, name.to_bytes()))
        .map_or(ptr::null_mut(), |token| glib::strdup(&token))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_namedmap_get_length(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe { getter(ctx, this_val, crate::named_map::get_length) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dataset_named_value(obj: JSValue, name: *const c_char) -> *mut c_char {
    let node = unsafe { Node::from_ptr(ns_dataset_node(obj)) };
    let (Some(node), Some(name)) = (node, c_str(name)) else {
        return ptr::null_mut();
    };
    crate::dataset::named_value(node, name.to_bytes())
        .map_or(ptr::null_mut(), |value| glib::strdup(value.to_bytes()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dataset_names(obj: JSValue) -> *mut *mut c_char {
    let node = unsafe { Node::from_ptr(ns_dataset_node(obj)) };
    strv(&node.map(crate::dataset::names).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dataset_named_set(
    ctx: *mut JSContext,
    obj: JSValue,
    name: *const c_char,
    value: JSValue,
) -> c_int {
    let Some(name) = c_str(name) else {
        return 0;
    };
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let obj = quickjs::borrow_value(scope, obj);
            let Some(node) = dataset_node(&obj) else {
                return 0;
            };
            let value = quickjs::borrow_value(scope, value);
            match crate::dataset::named_set(scope, node, name.to_bytes(), &value) {
                Ok(()) => 1,
                Err(error) => {
                    quickjs::result_raw(scope, Err(error));
                    -1
                }
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dataset_named_delete(
    ctx: *mut JSContext,
    obj: JSValue,
    name: *const c_char,
) {
    let Some(name) = c_str(name) else {
        return;
    };
    unsafe {
        with_value(ctx, obj, |scope, obj| {
            if let Some(node) = dataset_node(obj) {
                crate::dataset::named_delete(scope, node, name.to_bytes());
            }
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_has_dataset(node: *const NsNode) -> GBoolean {
    let node = unsafe { Node::from_ptr(node) };
    glib::boolean(node.is_some_and(crate::dataset::has_dataset))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_to_js(
    ctx: *mut JSContext,
    owner: JSValue,
    attr: *const NsAttr,
    include_base: GBoolean,
) -> JSValue {
    let Some(attr) = (unsafe { Attr::from_ptr(attr) }) else {
        return quickjs::UNDEFINED;
    };
    unsafe {
        with_value(ctx, owner, |scope, owner| {
            let result = attr::to_js(scope, owner, attr, include_base != 0);
            quickjs::result_raw(scope, result)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_owner(value: JSValue) -> *mut NsNode {
    let opaque = unsafe { ns_attr_opaque(value) }.cast::<RefCell<AttrState>>();
    let Some(state) = (unsafe { opaque.as_ref() }) else {
        return ptr::null_mut();
    };
    state
        .borrow()
        .owner
        .map_or(ptr::null_mut(), Node::as_mut_ptr)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_state_release(state: *mut c_void) {
    unsafe { release_state(state) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_detach_matching(
    js: *mut NsJs,
    owner: *mut NsNode,
    namespace_uri: *const c_char,
    local_name: *const c_char,
) {
    let (Some(owner), Some(local)) = (unsafe { Node::from_ptr(owner) }, c_str(local_name)) else {
        return;
    };
    attr::detach_matching(Js(js), owner, c_str(namespace_uri), local);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_detach_owner(js: *mut NsJs, owner: *mut NsNode) {
    if let Some(owner) = unsafe { Node::from_ptr(owner) } {
        attr::detach_owner(Js(js), owner);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_detach_all(js: *mut NsJs) {
    attr::detach_all(Js(js));
}
