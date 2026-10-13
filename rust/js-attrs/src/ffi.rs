//! Southstar — the C ABI of the attribute bindings as declared in src/js_internal.h, and the js.c calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
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
    fn ns_namedmap_owner(obj: JSValue) -> *mut NsNode;
    fn ns_attr_apply_proto(ctx: *mut JSContext, obj: JSValue);
    fn ns_valid_element_local_name(name: *const c_char) -> GBoolean;
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
    fn ns_element_reflect_str_get(
        ctx: *mut JSContext,
        this_val: JSValue,
        attr: *const c_char,
        null_if_absent: GBoolean,
    ) -> JSValue;
    fn ns_attribute_map_lookup(js: *mut NsJs, owner: *const NsNode) -> JSValue;
    fn ns_attribute_map_remember(
        ctx: *mut JSContext,
        js: *mut NsJs,
        owner: *const NsNode,
        map: JSValue,
    );
    fn ns_make_live(
        ctx: *mut JSContext,
        owner: JSValue,
        kind: c_int,
        param: *const c_char,
    ) -> JSValue;
}

const LIVE_ATTRIBUTES: c_int = 12;

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
    crate::class::node_of(value)
}

pub(crate) fn is_token_list(value: &Value) -> bool {
    crate::class::is_token_list(value)
}

pub(crate) fn new_token_list(
    scope: &mut Scope<'_>,
    element: &Value,
    attr: &'static CStr,
) -> Result<Value, Value> {
    crate::class::new_token_list(scope, element, attr)
}

pub(crate) fn reflect_str_get(
    scope: &mut Scope<'_>,
    this: &Value,
    attr: &CStr,
) -> Result<Value, Value> {
    let raw = unsafe {
        ns_element_reflect_str_get(
            quickjs::raw_context(scope),
            quickjs::raw(this),
            attr.as_ptr(),
            glib::FALSE,
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
}

pub(crate) fn attribute_map_lookup(scope: &mut Scope<'_>, js: Js, owner: Element) -> Option<Value> {
    let raw = unsafe { ns_attribute_map_lookup(js.0, owner.as_ptr()) };
    let map = unsafe { quickjs::take_value(scope, raw) };
    (!map.is_undefined()).then_some(map)
}

pub(crate) fn attribute_map_remember(scope: &mut Scope<'_>, js: Js, owner: Element, map: &Value) {
    unsafe {
        ns_attribute_map_remember(
            quickjs::raw_context(scope),
            js.0,
            owner.as_ptr(),
            quickjs::raw(map),
        )
    };
}

pub(crate) fn make_attributes_live(scope: &mut Scope<'_>, owner: &Value) -> Result<Value, Value> {
    let raw = unsafe {
        ns_make_live(
            quickjs::raw_context(scope),
            quickjs::raw(owner),
            LIVE_ATTRIBUTES,
            ptr::null(),
        )
    };
    let value = unsafe { quickjs::take_value(scope, raw) };
    quickjs::checked(scope, value)
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
    crate::class::new_attr(scope, state)
}

pub(crate) fn state_of(value: &Value) -> Option<Shared> {
    crate::class::attr_state(value)
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_make_token_list(
    ctx: *mut JSContext,
    element: JSValue,
    attr: *const c_char,
) -> JSValue {
    let Some(attr) = c_str(attr) else {
        return quickjs::into_raw(Value::null());
    };
    unsafe {
        with_value(ctx, element, |scope, element| {
            let result = crate::element::token_list(scope, element, attr);
            quickjs::result_raw(scope, result)
        })
    }
}

macro_rules! getter {
    ($name:ident, $f:path) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
            unsafe { getter(ctx, this_val, $f) }
        }
    };
}

macro_rules! setter {
    ($name:ident, $f:path) => {
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(
            ctx: *mut JSContext,
            this_val: JSValue,
            val: JSValue,
        ) -> JSValue {
            unsafe { setter(ctx, this_val, val, $f) }
        }
    };
}

getter!(ns_element_get_classList, crate::element::class_list);
getter!(ns_element_get_relList, crate::element::rel_list);
getter!(ns_element_get_sandbox_list, crate::element::sandbox_list);
getter!(ns_element_get_sizes_list, crate::element::sizes_list);
getter!(ns_element_get_attributes, crate::element::attributes);
setter!(ns_element_set_style, crate::element::set_style);
setter!(ns_element_set_classList, crate::element::set_class_list);
setter!(ns_element_set_relList, crate::element::set_rel_list);
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
pub unsafe extern "C" fn ns_token_list_register_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            crate::class::register(scope, proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_token_list_class_proto(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(crate::class::prototype(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_namedmap_get_length(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe { getter(ctx, this_val, crate::named_map::get_length) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_get_dataset(ctx: *mut JSContext, this_val: JSValue) -> JSValue {
    unsafe {
        with_value(ctx, this_val, |scope, this| {
            let dataset = crate::class::element_dataset(scope, this);
            quickjs::result_raw(scope, dataset)
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dataset_register_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            crate::class::register_dataset(scope, proto);
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_dataset_class_proto(ctx: *mut JSContext) -> JSValue {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            quickjs::into_raw(crate::class::dataset_prototype(scope))
        })
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_attr_register_class(ctx: *mut JSContext, proto: JSValue) {
    unsafe {
        quickjs::with_context(ctx, |scope| {
            let proto = quickjs::take_value(scope, proto);
            crate::class::register_attr(scope, proto);
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_attr_class_id() -> u32 {
    quickjs::class_id(&crate::class::ATTR_CLASS)
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
    let state = unsafe { quickjs::with_raw(value, crate::class::attr_state) };
    state
        .and_then(|state| state.borrow().owner)
        .map_or(ptr::null_mut(), Node::as_mut_ptr)
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

#[unsafe(no_mangle)]
pub unsafe fn nsr_attr_to_js(
    scope: &mut southstar_js_engine::Scope<'_>,
    owner: &southstar_js_engine::Value,
    attr: *const NsAttr,
    include_base: bool,
) -> Result<southstar_js_engine::Value, southstar_js_engine::Value> {
    match unsafe { Attr::from_ptr(attr) } {
        Some(attr) => attr::to_js(scope, owner, attr, include_base),
        None => Ok(southstar_js_engine::Value::undefined()),
    }
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_contains(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::token_list::contains(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_add(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    crate::token_list::add(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_remove(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::token_list::remove(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_toggle(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::token_list::toggle(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_replace(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::token_list::replace(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
    crate::token_list::item(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_supports(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::token_list::supports(scope, this, args)
}

#[unsafe(no_mangle)]
#[allow(non_snake_case)]
pub fn nsr_tlist_toString(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> Result<Value, Value> {
    crate::token_list::get_value(scope, this, args)
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_get_length(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
    crate::token_list::get_length(scope, this, &[])
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_get_value(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
    crate::token_list::get_value(scope, this, &[])
}

#[unsafe(no_mangle)]
pub fn nsr_tlist_set_value(
    scope: &mut Scope<'_>,
    this: &Value,
    value: &Value,
) -> Result<(), Value> {
    crate::token_list::set_value(scope, this, core::slice::from_ref(value)).map(drop)
}
