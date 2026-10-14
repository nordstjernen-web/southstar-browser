//! Southstar — the attribute bindings' calls into the page record and the other binding crates, and the entry points those call back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod imports;

use core::ffi::{CStr, c_char};
use core::ptr;
use std::cell::RefCell;
use std::rc::Rc;

use southstar_dom::{AttrView, Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;

use crate::attr::{self, AttrState};
use crate::{Element, c_string};

pub use southstar_js_runtime::api::NsJs;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(*mut NsJs);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_null()
    }
}

unsafe extern "C" {
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
    fn ns_valid_element_local_name(name: *const c_char) -> GBoolean;
    fn ns_js_doc_base_url(js: *mut NsJs) -> *mut c_char;
    fn ns_js_track_orphan(js: *mut NsJs, node: *mut NsNode);
}

const LIVE_ATTRIBUTES: i32 = 12;

fn opt_ptr(text: Option<&CStr>) -> *const c_char {
    text.map_or(ptr::null(), CStr::as_ptr)
}

fn c_str<'a>(text: *const c_char) -> Option<&'a CStr> {
    (!text.is_null()).then(|| unsafe { CStr::from_ptr(text) })
}

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(scope.context_opaque().cast())
}

pub(crate) fn unwrap_node(value: &Value) -> Option<Element> {
    api::unwrap_element(value)
}

pub(crate) fn wrap_node(scope: &mut Scope<'_>, node: Element) -> Value {
    api::make_element(scope, node.as_ptr()).unwrap_or_else(|_| Value::null())
}

pub(crate) fn dom_exception(scope: &mut Scope<'_>, name: &str, code: i32, message: &str) -> Value {
    let message = c_string(message.as_bytes());
    imports::nsr_dom_exception(scope, name.as_bytes(), code, message.to_bytes())
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
    imports::nsr_element_reflect_str_get(scope, this, attr, false)
}

pub(crate) fn attribute_map_lookup(
    _scope: &mut Scope<'_>,
    js: Js,
    owner: Element,
) -> Option<Value> {
    unsafe { imports::nsr_attribute_map_lookup(js.0, owner.as_ptr()) }
        .filter(|map| !map.is_undefined())
}

pub(crate) fn attribute_map_remember(_scope: &mut Scope<'_>, js: Js, owner: Element, map: &Value) {
    unsafe { imports::nsr_attribute_map_remember(js.0, owner.as_ptr(), map) };
}

pub(crate) fn make_attributes_live(scope: &mut Scope<'_>, owner: &Value) -> Result<Value, Value> {
    Ok(imports::nsr_make_live(
        scope,
        owner,
        LIVE_ATTRIBUTES,
        None,
        None,
    ))
}

pub(crate) fn named_map_owner(value: &Value) -> Option<Element> {
    unsafe { Node::from_ptr(imports::nsr_live_owner_node(value)) }
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
    let node = southstar_dom::node::new_text(southstar_dom::Kind::Text, text);
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
    api::attr_apply_proto(scope, entry);
}

pub(crate) fn attr_owner_node(value: &Value) -> *mut NsNode {
    state_of(value)
        .and_then(|state| state.borrow().owner)
        .map_or(ptr::null_mut(), Node::as_mut_ptr)
}

macro_rules! rust_methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> Result<Value, Value> {
                $f(scope, this, args)
            }
        )*
    };
}

macro_rules! rust_getters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value) -> Result<Value, Value> {
                $f(scope, this, &[])
            }
        )*
    };
}

macro_rules! rust_setters {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
            #[unsafe(no_mangle)]
            #[allow(non_snake_case)]
            pub fn $name(scope: &mut Scope<'_>, this: &Value, value: &Value) -> Result<(), Value> {
                $f(scope, this, core::slice::from_ref(value)).map(drop)
            }
        )*
    };
}

rust_methods! {
    nsr_tlist_contains => crate::token_list::contains,
    nsr_tlist_add => crate::token_list::add,
    nsr_tlist_remove => crate::token_list::remove,
    nsr_tlist_toggle => crate::token_list::toggle,
    nsr_tlist_replace => crate::token_list::replace,
    nsr_tlist_item => crate::token_list::item,
    nsr_tlist_supports => crate::token_list::supports,
    nsr_tlist_toString => crate::token_list::get_value,
    nsr_namedmap_item => crate::named_map::item,
}

rust_getters! {
    nsr_tlist_get_length => crate::token_list::get_length,
    nsr_tlist_get_value => crate::token_list::get_value,
}

rust_setters! {
    nsr_tlist_set_value => crate::token_list::set_value,
}

#[unsafe(no_mangle)]
pub fn nsr_make_token_list(
    scope: &mut Scope<'_>,
    element: &Value,
    attr: &'static CStr,
) -> Result<Value, Value> {
    crate::element::token_list(scope, element, attr)
}

#[unsafe(no_mangle)]
pub fn nsr_attr_owner(value: &Value) -> *mut NsNode {
    attr_owner_node(value)
}

#[unsafe(no_mangle)]
pub fn nsr_attr_to_js(
    scope: &mut Scope<'_>,
    owner: &Value,
    attr: AttrView<'_>,
    include_base: bool,
) -> Result<Value, Value> {
    attr::to_js(scope, owner, attr, include_base)
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_attr_class_id() -> u32 {
    crate::class::ATTR_CLASS.id()
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
