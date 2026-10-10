//! Southstar — attachShadow, the ShadowRoot wrapper's accessors, element.shadowRoot and getRootNode.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, Kind, MAX_DEPTH};
use southstar_glib as glib;
use southstar_js_engine::{Attributes, NativeFn, Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, JsResult, SHADOW_ATTR};

const DECLARATIVE: &CStr = c"data-nd-shadow-declarative";
const DELEGATES: &CStr = c"data-nd-shadow-delegates";
const SERIALIZABLE: &CStr = c"data-nd-shadow-serializable";
const CLONABLE: &CStr = c"data-nd-shadow-clonable";

const ALLOWED_HOSTS: [&[u8]; 18] = [
    b"article",
    b"aside",
    b"blockquote",
    b"body",
    b"div",
    b"footer",
    b"h1",
    b"h2",
    b"h3",
    b"h4",
    b"h5",
    b"h6",
    b"header",
    b"main",
    b"nav",
    b"p",
    b"section",
    b"span",
];

#[derive(Clone, Copy)]
struct Flags {
    delegates: bool,
    serializable: bool,
    clonable: bool,
}

fn host_name_allowed(host: Element) -> bool {
    let Some(name) = host.name().map(CStr::to_bytes) else {
        return false;
    };
    name.contains(&b'-')
        || ALLOWED_HOSTS
            .iter()
            .any(|allowed| name.eq_ignore_ascii_case(allowed))
}

fn disabled_for_custom_element(scope: &mut Scope<'_>, host: Element) -> bool {
    let Some(js) = ffi::js_of(scope) else {
        return false;
    };
    let class = ffi::custom_element_class(scope, js, host);
    if !class.is_object() {
        return false;
    }
    let Ok(features) = scope.get(&class, "disabledFeatures") else {
        return false;
    };
    if !features.is_array() {
        return false;
    }
    let length = scope
        .get(&features, "length")
        .and_then(|len| scope.to_number(&len))
        .unwrap_or(0.0) as u32;
    (0..length).any(|i| {
        scope.get_index(&features, i).is_ok_and(|item| {
            item.is_string() && scope.to_bytes(&item).is_ok_and(|s| s == b"shadow")
        })
    })
}

fn read_flag(scope: &mut Scope<'_>, init: &Value, key: &str) -> JsResult<bool> {
    let value = scope.get(init, key)?;
    Ok(scope.to_bool(&value))
}

fn write_flags(root: Element, flags: Flags) {
    for (on, attr) in [
        (flags.delegates, DELEGATES),
        (flags.serializable, SERIALIZABLE),
        (flags.clonable, CLONABLE),
    ] {
        if on {
            southstar_dom::attrs::set_len(root, attr, Some(b"1"), 1);
        }
    }
}

pub(crate) fn attach_shadow(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(host) = ffi::unwrap_node(this).filter(|n| n.kind() == Kind::Element) else {
        return Err(scope.type_error("attachShadow requires an Element host"));
    };
    let Some(init) = args.first().filter(|v| v.is_object()) else {
        return Err(scope.type_error("attachShadow: argument 1 is not a dictionary"));
    };
    if host.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) != 0 {
        return Err(ffi::not_supported(
            scope,
            "attachShadow: host is not in the HTML namespace",
        ));
    }
    let mode_value = scope.get(init, "mode")?;
    let mode: &CStr = match scope.to_bytes(&mode_value)?.as_slice() {
        b"open" => c"open",
        b"closed" => c"closed",
        _ => return Err(scope.type_error("attachShadow: mode must be 'open' or 'closed'")),
    };
    if !host_name_allowed(host) {
        return Err(ffi::not_supported(
            scope,
            "attachShadow: element cannot host a shadow root",
        ));
    }
    if disabled_for_custom_element(scope, host) {
        return Err(ffi::not_supported(
            scope,
            "attachShadow: custom element disables shadow roots",
        ));
    }
    let flags = Flags {
        delegates: read_flag(scope, init, "delegatesFocus")?,
        serializable: read_flag(scope, init, "serializable")?,
        clonable: read_flag(scope, init, "clonable")?,
    };
    if let Some(existing) = crate::shadow_child(host) {
        let declarative = existing.attr(DECLARATIVE).is_some();
        if !declarative || existing.attr(SHADOW_ATTR) != Some(mode) {
            return Err(ffi::not_supported(
                scope,
                "attachShadow: the element already hosts a shadow root",
            ));
        }
        ffi::clear_children(ffi::js_of(scope), existing);
        for attr in [DECLARATIVE, DELEGATES, SERIALIZABLE, CLONABLE] {
            southstar_dom::attrs::remove(existing, attr);
        }
        write_flags(existing, flags);
        return wrap_shadow_root(scope, existing);
    }
    let root = southstar_dom::node::new_element(glib::strdup(b"div"));
    southstar_dom::attrs::set_len(
        root,
        SHADOW_ATTR,
        Some(mode.to_bytes()),
        mode.to_bytes().len(),
    );
    write_flags(root, flags);
    ffi::append_child(host, root);
    ffi::arm_js_invalidate(root);
    wrap_shadow_root(scope, root)
}

fn get_host(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    match ffi::unwrap_node(this).and_then(|root| root.parent()) {
        Some(host) => Ok(ffi::wrap_node(scope, host)),
        None => Ok(Value::undefined()),
    }
}

fn get_mode(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let Some(root) = ffi::unwrap_node(this) else {
        return Ok(Value::undefined());
    };
    let mode = root.attr(SHADOW_ATTR).map_or(&b"open"[..], CStr::to_bytes);
    Ok(scope.string_from_bytes(mode))
}

fn flag(this: &Value, attr: &CStr) -> JsResult {
    Ok(Value::boolean(
        ffi::unwrap_node(this).is_some_and(|root| root.attr(attr).is_some()),
    ))
}

fn get_delegates_focus(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    flag(this, DELEGATES)
}

fn get_serializable(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    flag(this, SERIALIZABLE)
}

fn get_clonable(_: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    flag(this, CLONABLE)
}

fn within(node: Element, ancestor: Element) -> bool {
    node == ancestor
        || southstar_dom::ancestors(node)
            .take(MAX_DEPTH as usize)
            .any(|p| p == ancestor)
}

fn active_in(js: Js, root: Element) -> Option<Element> {
    let doc = ffi::current_document(js)?;
    let focused = ffi::focused_node(js)?;
    (within(root, doc) && within(focused, root)).then_some(focused)
}

fn get_active_element(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    let active = ffi::js_of(scope)
        .zip(ffi::unwrap_node(this))
        .and_then(|(js, root)| active_in(js, root));
    Ok(active.map_or_else(Value::null, |node| ffi::wrap_node(scope, node)))
}

pub(crate) fn wrap_shadow_root(scope: &mut Scope<'_>, root: Element) -> JsResult {
    let wrapper = ffi::wrap_node(scope, root);
    define_props(scope, &wrapper)?;
    Ok(wrapper)
}

fn define_getter(
    scope: &mut Scope<'_>,
    wrapper: &Value,
    key: &str,
    name: &str,
    f: NativeFn,
) -> JsResult<()> {
    let getter = scope.function(name, 0, f);
    scope.define_accessor(wrapper, key, Some(&getter), None, Attributes::CONFIGURABLE)
}

fn define_props(scope: &mut Scope<'_>, wrapper: &Value) -> JsResult<()> {
    let global = scope.global();
    let ctor = scope.get(&global, "ShadowRoot")?;
    let proto = scope.get(&ctor, "prototype")?;
    if proto.is_object() {
        scope.set_prototype(wrapper, &proto)?;
    }
    let getters: [(&str, &str, NativeFn); 6] = [
        ("delegatesFocus", "delegatesFocus", get_delegates_focus),
        ("serializable", "serializable", get_serializable),
        ("clonable", "clonable", get_clonable),
        ("host", "get host", get_host),
        ("mode", "get mode", get_mode),
        ("activeElement", "get activeElement", get_active_element),
    ];
    for (key, name, f) in getters {
        define_getter(scope, wrapper, key, name, f)?;
    }
    for (key, function) in ffi::element_from_point_fns(scope) {
        scope.set(wrapper, key, function)?;
    }
    Ok(())
}

pub(crate) fn get_shadow_root(scope: &mut Scope<'_>, this: &Value, _: &[Value]) -> JsResult {
    match ffi::unwrap_node(this).and_then(crate::shadow_child) {
        Some(root) if !crate::is_closed(root) => wrap_shadow_root(scope, root),
        _ => Ok(Value::null()),
    }
}

fn composed_option(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<bool> {
    match args.first().filter(|v| v.is_object()) {
        Some(options) => {
            let composed = scope.get(options, "composed")?;
            Ok(scope.to_bool(&composed))
        }
        None => Ok(false),
    }
}

fn root_of_non_node(scope: &mut Scope<'_>, this: &Value) -> JsResult {
    let host = scope.get(this, "host")?;
    if !host.is_undefined() && !host.is_null() {
        return Ok(this.clone());
    }
    let node_type = scope.get(this, "nodeType")?;
    if !node_type.is_undefined() && !node_type.is_null() {
        return Ok(this.clone());
    }
    Ok(Value::null())
}

pub(crate) fn get_root_node(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return root_of_non_node(scope, this);
    };
    if !composed_option(scope, args)?
        && let Some(root) =
            southstar_dom::ancestors_and_self(node).find(|&a| crate::is_shadow_root(Some(a)))
    {
        return wrap_shadow_root(scope, root);
    }
    let top = southstar_dom::ancestors_and_self(node)
        .last()
        .unwrap_or(node);
    Ok(ffi::wrap_node(scope, top))
}
