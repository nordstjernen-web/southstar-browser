//! Southstar — the dataset DOMStringMap: data-* attributes as camel-cased properties.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::ffi::CString;

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_SVG_NS, attrs};
use southstar_js_engine::{Scope, Value};

use crate::{Element, INVALID_CHARACTER_ERR, JsResult, SYNTAX_ERR, c_string, ffi, string_arg};

const MATHML_NS: &[u8] = b"http://www.w3.org/1998/Math/MathML";

fn tail_has_upper(name: &[u8]) -> bool {
    name.iter().any(u8::is_ascii_uppercase)
}

fn has_dash_lower(prop: &[u8]) -> bool {
    prop.windows(2)
        .any(|pair| pair[0] == b'-' && pair[1].is_ascii_lowercase())
}

fn attr_to_prop(name: &[u8]) -> Option<Vec<u8>> {
    let tail = name.strip_prefix(b"data-")?;
    if attrs::is_internal(name) || tail_has_upper(tail) {
        return None;
    }
    let mut out = Vec::with_capacity(tail.len());
    let mut i = 0;
    while i < tail.len() {
        if tail[i] == b'-' && tail.get(i + 1).is_some_and(u8::is_ascii_lowercase) {
            out.push(tail[i + 1].to_ascii_uppercase());
            i += 2;
        } else {
            out.push(tail[i]);
            i += 1;
        }
    }
    Some(out)
}

enum Invalid {
    Syntax,
    Character,
}

fn prop_to_attr(prop: &[u8]) -> Result<CString, Invalid> {
    if has_dash_lower(prop) {
        return Err(Invalid::Syntax);
    }
    let mut out = b"data-".to_vec();
    for &c in prop {
        if c.is_ascii_uppercase() {
            out.push(b'-');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    let attr = c_string(&out);
    if ffi::valid_element_local_name(&attr) {
        Ok(attr)
    } else {
        Err(Invalid::Character)
    }
}

pub(crate) fn has_dataset(node: Element) -> bool {
    if !node.is_element() {
        return false;
    }
    let flags = node.flags();
    if flags & FLAG_SVG_NS != 0 || flags & FLAG_FOREIGN_NS == 0 {
        return true;
    }
    node.attr(c"data-nd-ns-uri")
        .is_some_and(|ns| ns.to_bytes() == MATHML_NS)
}

pub(crate) fn named_value(node: Element, prop: &[u8]) -> Option<&'static CStr> {
    let attr = prop_to_attr(prop).ok()?;
    node.attrs()
        .find(|a| {
            a.name().is_some_and(|name| {
                let name = name.to_bytes();
                name == attr.to_bytes() && !attrs::is_internal(name) && !tail_has_upper(&name[5..])
            })
        })
        .map(|a| a.value().unwrap_or(c""))
}

pub(crate) fn names(node: Element) -> Vec<Vec<u8>> {
    node.attrs()
        .filter_map(|a| a.name().and_then(|name| attr_to_prop(name.to_bytes())))
        .collect()
}

pub(crate) fn named_set(
    scope: &mut Scope<'_>,
    node: Element,
    prop: &[u8],
    value: &Value,
) -> JsResult<()> {
    let attr = match prop_to_attr(prop) {
        Ok(attr) => attr,
        Err(Invalid::Syntax) => {
            return Err(ffi::dom_exception(
                scope,
                "SyntaxError",
                SYNTAX_ERR,
                "invalid dataset property name",
            ));
        }
        Err(Invalid::Character) => {
            return Err(ffi::dom_exception(
                scope,
                "InvalidCharacterError",
                INVALID_CHARACTER_ERR,
                "invalid dataset attribute name",
            ));
        }
    };
    let value = string_arg(scope, value)?;
    if !attrs::is_internal(attr.to_bytes()) {
        let qname = ffi::QualifiedName {
            namespace_uri: None,
            prefix: None,
            local_name: &attr,
            name: &attr,
        };
        ffi::set_attr_ns(ffi::js_of(scope), node, &qname, &value);
    }
    Ok(())
}

pub(crate) fn named_delete(scope: &mut Scope<'_>, node: Element, prop: &[u8]) {
    if let Ok(attr) = prop_to_attr(prop)
        && !attrs::is_internal(attr.to_bytes())
    {
        ffi::remove_attr(ffi::js_of(scope), node, &attr);
    }
}
