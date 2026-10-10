//! Southstar — locate a namespace and locate a namespace prefix: lookupNamespaceURI, lookupPrefix and isDefaultNamespace on nodes and Attr objects.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, Kind, MAX_DEPTH, children};
use southstar_js_engine::{Scope, Value};

use crate::names::{HTML_NS, SVG_NS, XML_NS, XMLNS_NS};
use crate::{Element, FLAG_FRAGMENT, JsResult, arg, c_text, ffi, is_nullish};

fn element_namespace(node: Element) -> Option<&'static [u8]> {
    if node.kind() != Kind::Element {
        return None;
    }
    if let Some(stored) = node.attr(c"data-nd-ns-uri") {
        return (!stored.is_empty()).then(|| stored.to_bytes());
    }
    if node.flags() & FLAG_SVG_NS != 0 {
        return Some(SVG_NS);
    }
    if node.flags() & FLAG_FOREIGN_NS != 0 {
        return None;
    }
    Some(HTML_NS)
}

fn element_prefix(node: Element) -> Option<&'static [u8]> {
    if node.kind() != Kind::Element {
        return None;
    }
    if let Some(stored) = node.attr(c"data-nd-ns-prefix").filter(|s| !s.is_empty()) {
        return Some(stored.to_bytes());
    }
    if node.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) == 0 {
        return None;
    }
    let name = node.name()?.to_bytes();
    name.iter()
        .position(|&b| b == b':')
        .map(|colon| &name[..colon])
}

fn parent_element(node: Element) -> Option<Element> {
    node.parent().filter(|p| p.kind() == Kind::Element)
}

fn non_empty(value: Option<&'static core::ffi::CStr>) -> Option<&'static [u8]> {
    value.map(|v| v.to_bytes()).filter(|v| !v.is_empty())
}

fn locate_namespace(node: Element, prefix: Option<&[u8]>, depth: i32) -> Option<&'static [u8]> {
    if depth >= MAX_DEPTH || node.flags() & FLAG_FRAGMENT != 0 {
        return None;
    }
    match node.kind() {
        Kind::Element => {
            match prefix {
                Some(b"xml") => return Some(XML_NS),
                Some(b"xmlns") => return Some(XMLNS_NS),
                _ => {}
            }
            let ns = element_namespace(node);
            if ns.is_some() && element_prefix(node) == prefix {
                return ns;
            }
            for a in node.attrs() {
                if a.namespace_uri().map(|n| n.to_bytes()) != Some(XMLNS_NS) {
                    continue;
                }
                let attr_prefix = a.prefix().map(|p| p.to_bytes());
                let local = a.local_name().map(|l| l.to_bytes());
                if attr_prefix == Some(b"xmlns")
                    && local.is_some()
                    && prefix.is_some()
                    && local == prefix
                {
                    return non_empty(a.value());
                }
                if attr_prefix.is_none() && local == Some(b"xmlns") && prefix.is_none() {
                    return non_empty(a.value());
                }
            }
            parent_element(node).and_then(|p| locate_namespace(p, prefix, depth + 1))
        }
        Kind::Document => children(node)
            .find(|c| c.kind() == Kind::Element)
            .and_then(|c| locate_namespace(c, prefix, depth + 1)),
        Kind::Doctype => None,
        _ => parent_element(node).and_then(|p| locate_namespace(p, prefix, depth + 1)),
    }
}

fn locate_prefix(node: Element, ns: &[u8], depth: i32) -> Option<&'static [u8]> {
    if depth >= MAX_DEPTH || node.kind() != Kind::Element {
        return None;
    }
    if element_namespace(node) == Some(ns)
        && let Some(prefix) = element_prefix(node)
    {
        return Some(prefix);
    }
    for a in node.attrs() {
        if a.prefix().map(|p| p.to_bytes()) == Some(b"xmlns")
            && a.value().map(|v| v.to_bytes()) == Some(ns)
            && let Some(local) = a.local_name()
        {
            return Some(local.to_bytes());
        }
    }
    parent_element(node).and_then(|p| locate_prefix(p, ns, depth + 1))
}

fn non_empty_arg(scope: &mut Scope<'_>, args: &[Value]) -> JsResult<Option<Vec<u8>>> {
    let value = arg(args, 0);
    if is_nullish(&value) {
        return Ok(None);
    }
    let text = c_text(scope, &value)?;
    Ok((!text.is_empty()).then_some(text))
}

fn string_or_null(scope: &mut Scope<'_>, bytes: Option<&[u8]>) -> Value {
    bytes.map_or_else(Value::null, |b| scope.string_from_bytes(b))
}

pub(crate) fn lookup_namespace_uri(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    let prefix = non_empty_arg(scope, args)?;
    let result = locate_namespace(node, prefix.as_deref(), 0);
    Ok(string_or_null(scope, result))
}

pub(crate) fn lookup_prefix(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::null());
    };
    let Some(ns) = non_empty_arg(scope, args)? else {
        return Ok(Value::null());
    };
    let result = if node.flags() & FLAG_FRAGMENT != 0 {
        None
    } else {
        match node.kind() {
            Kind::Element => locate_prefix(node, &ns, 0),
            Kind::Document => children(node)
                .find(|c| c.kind() == Kind::Element)
                .and_then(|c| locate_prefix(c, &ns, 0)),
            Kind::Doctype => None,
            _ => parent_element(node).and_then(|p| locate_prefix(p, &ns, 0)),
        }
    };
    Ok(string_or_null(scope, result))
}

pub(crate) fn is_default_namespace(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    let Some(node) = ffi::unwrap_node(this) else {
        return Ok(Value::boolean(false));
    };
    let ns = non_empty_arg(scope, args)?;
    let default = locate_namespace(node, None, 0);
    Ok(Value::boolean(default == ns.as_deref()))
}

fn owner_element(scope: &mut Scope<'_>, this: &Value) -> Option<Value> {
    scope
        .get(this, "ownerElement")
        .ok()
        .filter(|owner| !is_nullish(owner))
}

pub(crate) fn attr_lookup_namespace_uri(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    match owner_element(scope, this) {
        Some(owner) => lookup_namespace_uri(scope, &owner, args),
        None => Ok(Value::null()),
    }
}

pub(crate) fn attr_lookup_prefix(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    match owner_element(scope, this) {
        Some(owner) => lookup_prefix(scope, &owner, args),
        None => Ok(Value::null()),
    }
}

pub(crate) fn attr_is_default_namespace(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    if let Some(owner) = owner_element(scope, this) {
        return is_default_namespace(scope, &owner, args);
    }
    Ok(Value::boolean(non_empty_arg(scope, args)?.is_none()))
}
