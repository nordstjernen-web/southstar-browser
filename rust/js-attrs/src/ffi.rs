//! Southstar — the attribute bindings' calls into the page record and the other binding crates, and the entry points those call back.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod imports;

use core::ffi::CStr;
use std::cell::RefCell;
use std::rc::Rc;

use southstar_dom::{AttrView, Node};
use southstar_js_engine::{Scope, Value};
use southstar_js_runtime::api;
use southstar_js_runtime::api::Page;

use crate::attr::{self, AttrState};
use crate::{Element, c_string};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Js(Option<Page>);

impl Js {
    pub fn is_null(self) -> bool {
        self.0.is_none()
    }
}

const LIVE_ATTRIBUTES: i32 = 12;

pub(crate) fn js_of(scope: &Scope<'_>) -> Js {
    Js(Page::current(scope))
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

pub(crate) use southstar_js_mutation::QualifiedName;

pub(crate) fn set_attr(js: Js, node: Element, name: &CStr, value: &CStr) {
    southstar_js_mutation::set_attr_recorded(js.0, node, name, value.to_bytes());
}

pub(crate) fn set_attr_ns(js: Js, node: Element, qname: &QualifiedName<'_>, value: &CStr) {
    southstar_js_mutation::set_attr_ns_recorded(js.0, node, qname, value);
}

pub(crate) fn remove_attr(js: Js, node: Element, name: &CStr) {
    southstar_js_mutation::remove_attr_recorded(js.0, node, name);
}

pub(crate) fn remove_attr_ns(js: Js, node: Element, namespace_uri: Option<&CStr>, local: &CStr) {
    southstar_js_mutation::remove_attr_ns_recorded(js.0, node, namespace_uri, local);
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
    imports::nsr_attribute_map_lookup(js.0.filter(|page| page.is_live())?, owner)
        .filter(|map| !map.is_undefined())
}

pub(crate) fn attribute_map_remember(_scope: &mut Scope<'_>, js: Js, owner: Element, map: &Value) {
    if let Some(page) = js.0.filter(|page| page.is_live()) {
        imports::nsr_attribute_map_remember(page, owner, map);
    }
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
    imports::nsr_live_owner_node(value)
}

pub(crate) fn doc_base_url(js: Js) -> Option<Vec<u8>> {
    southstar_js_document::doc_base_url(js.0)
}

pub(crate) fn new_text(js: Js, text: &[u8]) -> Element {
    let node = southstar_dom::node::new_text(southstar_dom::Kind::Text, text);
    southstar_js_mutation::track_orphan(js.0, node);
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

pub(crate) fn attr_owner_node(value: &Value) -> Option<Element> {
    state_of(value).and_then(|state| state.borrow().owner)
}

macro_rules! rust_methods {
    ($($name:ident => $f:path),* $(,)?) => {
        $(
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

pub fn nsr_make_token_list(
    scope: &mut Scope<'_>,
    element: &Value,
    attr: &'static CStr,
) -> Result<Value, Value> {
    crate::element::token_list(scope, element, attr)
}

pub fn nsr_attr_owner(value: &Value) -> Option<Element> {
    attr_owner_node(value)
}

pub fn nsr_attr_to_js(
    scope: &mut Scope<'_>,
    owner: &Value,
    attr: AttrView<'_>,
    include_base: bool,
) -> Result<Value, Value> {
    attr::to_js(scope, owner, attr, include_base)
}

pub fn detach_all(js: Option<Page>) {
    attr::detach_all(Js(js));
}

fn class_id() -> u32 {
    crate::class::ATTR_CLASS.id()
}

fn detach_owner(js: Option<Page>, owner: Node<'_>) {
    if let Some(owner) = owner.downgrade().get() {
        attr::detach_owner(Js(js), owner);
    }
}

fn detach_page(js: Option<Page>) {
    detach_all(js);
}

pub fn install_runtime_hooks() {
    api::set_attrs_hooks(api::AttrsHooks {
        class_id,
        detach_owner,
        detach_all: |js| detach_page(Some(js)),
        detach_matching,
    });
    southstar_js_dom_factory::links::set_js_attrs_links(
        southstar_js_dom_factory::links::JsAttrsLinks {
            attr_to_js: crate::ffi::nsr_attr_to_js,
        },
    );
    southstar_js_reflect::links::set_js_attrs_links(southstar_js_reflect::links::JsAttrsLinks {
        make_token_list: crate::ffi::nsr_make_token_list,
    });
    southstar_js_runtime::links::set_js_attrs_links(southstar_js_runtime::links::JsAttrsLinks {
        attr_to_js: crate::ffi::nsr_attr_to_js,
    });
    southstar_js_forms::links::set_js_attrs_links(southstar_js_forms::links::JsAttrsLinks {
        namedmap_item: crate::ffi::nsr_namedmap_item,
    });
}

fn detach_matching(
    js: Option<Page>,
    owner: Node<'_>,
    namespace_uri: Option<&CStr>,
    local_name: &CStr,
) {
    if let Some(owner) = owner.downgrade().get() {
        attr::detach_matching(Js(js), owner, namespace_uri, local_name);
    }
}
