//! Southstar — Attr nodes: one wrapper per owned attribute, its live value accessors, and get/set/removeAttributeNode.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::cell::RefCell;
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::{Attr, FLAG_FOREIGN_NS, FLAG_SVG_NS, attrs};
use southstar_js_engine::{Attributes, Scope, Value};

use crate::ffi::{self, Js, QualifiedName, Shared};
use crate::{
    Element, INUSE_ATTRIBUTE_ERR, JsResult, NOT_FOUND_ERR, optional_string_arg, same_node,
    string_arg,
};

const NODE_FIELD: Attributes = Attributes {
    writable: false,
    enumerable: true,
    configurable: true,
};

pub(crate) struct AttrState {
    js: Js,
    pub(crate) owner: Option<Element>,
    namespace_uri: Option<CString>,
    prefix: Option<CString>,
    local_name: CString,
    name: CString,
    value: CString,
    pinned: Option<Value>,
}

struct OwnedName {
    namespace_uri: Option<CString>,
    prefix: Option<CString>,
    local_name: CString,
    name: CString,
}

impl OwnedName {
    fn qualified(&self) -> QualifiedName<'_> {
        QualifiedName {
            namespace_uri: self.namespace_uri.as_deref(),
            prefix: self.prefix.as_deref(),
            local_name: &self.local_name,
            name: &self.name,
        }
    }
}

impl AttrState {
    fn owned_name(&self) -> OwnedName {
        OwnedName {
            namespace_uri: self.namespace_uri.clone(),
            prefix: self.prefix.clone(),
            local_name: self.local_name.clone(),
            name: self.name.clone(),
        }
    }
}

thread_local! {
    static OWNED: RefCell<Vec<Shared>> = const { RefCell::new(Vec::new()) };
}

fn not_internal(attr: &Attr<'_>) -> bool {
    attr.name()
        .is_none_or(|name| !attrs::is_internal(name.to_bytes()))
}

pub(crate) fn page_attr(
    node: Element,
    namespace_uri: Option<&CStr>,
    local: &CStr,
) -> Option<Attr<'static>> {
    attrs::find_ns(node, namespace_uri, local).filter(not_internal)
}

fn dom_attr(state: &AttrState) -> Option<Attr<'static>> {
    page_attr(
        state.owner?,
        state.namespace_uri.as_deref(),
        &state.local_name,
    )
}

fn find(js: Js, owner: Element, namespace_uri: Option<&CStr>, local: &CStr) -> Option<Shared> {
    if js.is_null() {
        return None;
    }
    let namespace_uri = namespace_uri.filter(|ns| !ns.is_empty());
    OWNED.with_borrow(|owned| {
        owned
            .iter()
            .find(|shared| {
                let state = shared.borrow();
                state.owner.is_some_and(|o| same_node(o, owner))
                    && state.namespace_uri.as_deref() == namespace_uri
                    && state.local_name.as_c_str() == local
            })
            .cloned()
    })
}

fn detach(shared: &Shared) {
    let pinned = {
        let mut state = shared.borrow_mut();
        if state.owner.is_none() {
            return;
        }
        if let Some(attr) = dom_attr(&state) {
            state.value = attr.value().unwrap_or(c"").to_owned();
        }
        state.owner = None;
        state.pinned.take()
    };
    OWNED.with_borrow_mut(|owned| owned.retain(|other| !Rc::ptr_eq(other, shared)));
    drop(pinned);
}

fn attach(scope: &Scope<'_>, entry: &Value, shared: &Shared, owner: Element) {
    let current = shared.borrow().owner;
    if current.is_some_and(|o| same_node(o, owner)) {
        return;
    }
    if current.is_some() {
        detach(shared);
    }
    let js = ffi::js_of(scope);
    {
        let mut state = shared.borrow_mut();
        state.js = js;
        state.owner = Some(owner);
        state.pinned = Some(entry.clone());
    }
    if !js.is_null() {
        OWNED.with_borrow_mut(|owned| owned.push(shared.clone()));
    }
}

fn states_where(js: Js, keep: impl Fn(&AttrState) -> bool) -> Vec<Shared> {
    OWNED.with_borrow(|owned| {
        owned
            .iter()
            .rev()
            .filter(|shared| {
                let state = shared.borrow();
                state.js == js && keep(&state)
            })
            .cloned()
            .collect()
    })
}

pub(crate) fn detach_matching(js: Js, owner: Element, namespace_uri: Option<&CStr>, local: &CStr) {
    if let Some(shared) = find(js, owner, namespace_uri, local) {
        detach(&shared);
    }
}

pub(crate) fn detach_owner(js: Js, owner: Element) {
    if js.is_null() {
        return;
    }
    let matching = states_where(js, |state| state.owner.is_some_and(|o| same_node(o, owner)));
    for shared in matching {
        detach(&shared);
    }
}

pub(crate) fn detach_all(js: Js) {
    if js.is_null() {
        return;
    }
    for shared in states_where(js, |_| true) {
        detach(&shared);
    }
}

fn get_value(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let Some(shared) = ffi::state_of(this) else {
        return Ok(scope.string(""));
    };
    let value = {
        let state = shared.borrow();
        match dom_attr(&state).and_then(|attr| attr.value()) {
            Some(value) => value.to_bytes().to_vec(),
            None => state.value.to_bytes().to_vec(),
        }
    };
    Ok(scope.string_from_bytes(&value))
}

fn set_value(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let value = string_arg(scope, args.first().unwrap_or(&Value::undefined()))?;
    let Some(shared) = ffi::state_of(this) else {
        return Ok(Value::undefined());
    };
    let target = {
        let mut state = shared.borrow_mut();
        state.value = value.clone();
        match state.owner {
            Some(owner) if !attrs::is_internal(state.name.to_bytes()) => {
                Some((state.js, owner, state.owned_name()))
            }
            _ => None,
        }
    };
    if let Some((js, owner, name)) = target {
        ffi::set_attr_ns(js, owner, &name.qualified(), &value);
    }
    Ok(Value::undefined())
}

fn get_owner(scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let owner = ffi::state_of(this).and_then(|shared| shared.borrow().owner);
    Ok(owner.map_or_else(Value::null, |owner| ffi::wrap_node(scope, owner)))
}

fn optional_string(scope: &mut Scope<'_>, text: Option<&CStr>) -> Value {
    text.map_or_else(Value::null, |text| scope.string_from_bytes(text.to_bytes()))
}

fn define_node_fields(scope: &mut Scope<'_>, entry: &Value, state: &AttrState) -> JsResult<()> {
    for key in ["name", "nodeName"] {
        let name = scope.string_from_bytes(state.name.to_bytes());
        scope.define(entry, key, name, NODE_FIELD)?;
    }
    let local = scope.string_from_bytes(state.local_name.to_bytes());
    scope.define(entry, "localName", local, NODE_FIELD)?;
    for key in ["value", "nodeValue", "textContent"] {
        let getter = scope.function("get", 0, get_value);
        let setter = scope.function("set", 1, set_value);
        scope.define_accessor(
            entry,
            key,
            Some(&getter),
            Some(&setter),
            Attributes::CONFIGURABLE,
        )?;
    }
    let owner_getter = scope.function("get", 0, get_owner);
    scope.define_accessor(
        entry,
        "ownerElement",
        Some(&owner_getter),
        None,
        Attributes::CONFIGURABLE,
    )?;
    scope.define(entry, "nodeType", Value::int(2), NODE_FIELD)?;
    let namespace_uri = optional_string(scope, state.namespace_uri.as_deref());
    scope.define(entry, "namespaceURI", namespace_uri, NODE_FIELD)?;
    let prefix = optional_string(scope, state.prefix.as_deref());
    scope.define(entry, "prefix", prefix, NODE_FIELD)?;
    scope.define(entry, "specified", Value::boolean(true), NODE_FIELD)
}

fn define_tree_fields(scope: &mut Scope<'_>, entry: &Value, value: &[u8]) -> JsResult<()> {
    let js = ffi::js_of(scope);
    let base = ffi::doc_base_url(js).filter(|base| !base.is_empty());
    let base = scope.string_from_bytes(base.as_deref().unwrap_or(b"about:blank"));
    scope.set(entry, "baseURI", base)?;
    let kids = scope.new_array();
    if value.is_empty() {
        scope.set(entry, "firstChild", Value::null())?;
        scope.set(entry, "lastChild", Value::null())?;
    } else {
        let text = ffi::new_text(js, value);
        let wrapper = ffi::wrap_node(scope, text);
        scope.set_index(&kids, 0, wrapper.clone())?;
        scope.set(entry, "firstChild", wrapper.clone())?;
        scope.set(entry, "lastChild", wrapper)?;
    }
    scope.set(entry, "childNodes", kids)
}

pub(crate) fn to_js(
    scope: &mut Scope<'_>,
    owner: &Value,
    attr: Attr<'_>,
    include_base: bool,
) -> JsResult {
    let local = attrs::local_name(attr);
    let owner_node = ffi::unwrap_node(owner);
    let js = ffi::js_of(scope);
    if let Some(owner_node) = owner_node
        && let Some(cached) = find(js, owner_node, attr.namespace_uri(), local)
        && let Some(pinned) = cached.borrow().pinned.clone()
    {
        return Ok(pinned);
    }
    let shared: Shared = Rc::new(RefCell::new(AttrState {
        js,
        owner: None,
        namespace_uri: attr.namespace_uri().map(CStr::to_owned),
        prefix: attr.prefix().map(CStr::to_owned),
        local_name: local.to_owned(),
        name: attr.name().unwrap_or(c"").to_owned(),
        value: attr.value().unwrap_or(c"").to_owned(),
        pinned: None,
    }));
    let entry = ffi::new_attr_object(scope, shared.clone())?;
    ffi::apply_attr_proto(scope, &entry);
    define_node_fields(scope, &entry, &shared.borrow())?;
    if let Some(owner_node) = owner_node {
        attach(scope, &entry, &shared, owner_node);
    }
    if include_base {
        let value = shared.borrow().value.to_bytes().to_vec();
        define_tree_fields(scope, &entry, &value)?;
    }
    Ok(entry)
}

fn element_of(this: &Value) -> Option<Element> {
    ffi::unwrap_node(this).filter(|n| n.is_element())
}

pub(crate) fn get_attribute_node(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (Some(node), Some(arg)) = (element_of(this), args.first()) else {
        return Ok(Value::null());
    };
    let wanted = string_arg(scope, arg)?;
    let lowered = if node.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0 {
        wanted.to_bytes().to_ascii_lowercase()
    } else {
        wanted.to_bytes().to_vec()
    };
    let found = node
        .attrs()
        .find(|a| a.name().is_some_and(|name| name.to_bytes() == lowered));
    match found {
        Some(found) if not_internal(&found) => to_js(scope, this, found, true),
        _ => Ok(Value::null()),
    }
}

pub(crate) fn get_attribute_node_ns(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(node) = element_of(this).filter(|_| args.len() >= 2) else {
        return Ok(Value::null());
    };
    let namespace_uri = optional_string_arg(scope, &args[0])?.filter(|ns| !ns.is_empty());
    let local = string_arg(scope, &args[1])?;
    match page_attr(node, namespace_uri.as_deref(), &local) {
        Some(found) => to_js(scope, this, found, true),
        None => Ok(Value::null()),
    }
}

fn attr_argument(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult<(Element, Shared)> {
    let (Some(node), Some(arg)) = (element_of(this), args.first()) else {
        return Err(scope.type_error("1 Attr argument required"));
    };
    let Some(shared) = ffi::state_of(arg) else {
        return Err(scope.type_error("argument 1 is not an Attr"));
    };
    Ok((node, shared))
}

pub(crate) fn remove_attribute_node(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let (node, shared) = attr_argument(scope, this, args)?;
    let (namespace_uri, local) = {
        let state = shared.borrow();
        let owned = state.owner.is_some_and(|o| same_node(o, node)) && dom_attr(&state).is_some();
        if !owned {
            drop(state);
            return Err(ffi::dom_exception(
                scope,
                "NotFoundError",
                NOT_FOUND_ERR,
                "the attribute is not owned by this element",
            ));
        }
        (state.namespace_uri.clone(), state.local_name.clone())
    };
    ffi::remove_attr_ns(ffi::js_of(scope), node, namespace_uri.as_deref(), &local);
    Ok(args[0].clone())
}

pub(crate) fn set_attribute_node(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let (node, shared) = attr_argument(scope, this, args)?;
    let (owner, name, value) = {
        let state = shared.borrow();
        (state.owner, state.owned_name(), state.value.clone())
    };
    if owner.is_some_and(|o| !same_node(o, node)) {
        return Err(ffi::dom_exception(
            scope,
            "InUseAttributeError",
            INUSE_ATTRIBUTE_ERR,
            "the attribute is in use by another element",
        ));
    }
    if attrs::is_internal(name.name.to_bytes()) || attrs::is_internal(name.local_name.to_bytes()) {
        return Ok(Value::null());
    }
    let namespace_uri = name.namespace_uri.as_deref();
    let previous = page_attr(node, namespace_uri, &name.local_name);
    let previous_state = find(ffi::js_of(scope), node, namespace_uri, &name.local_name);
    if owner.is_some() && previous_state.is_some_and(|p| Rc::ptr_eq(&p, &shared)) {
        return Ok(args[0].clone());
    }
    let mut old = Value::null();
    if let Some(previous) = previous {
        old = to_js(scope, this, previous, true)?;
        if let Some(previous_state) = ffi::state_of(&old) {
            detach(&previous_state);
        }
    }
    ffi::set_attr_ns(ffi::js_of(scope), node, &name.qualified(), &value);
    attach(scope, &args[0], &shared, node);
    Ok(old)
}
