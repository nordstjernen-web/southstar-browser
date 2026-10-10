//! Southstar — NamedNodeMap, element.attributes: length, item, the named lookups and setNamedItem/removeNamedItem.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::attrs;
use southstar_js_engine::{Scope, Value};

use crate::attr::{self, page_attr};
use crate::{JsResult, NOT_FOUND_ERR, ffi, optional_string_arg, string_arg};

pub(crate) fn get_named_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(arg) = args.first() else {
        return Ok(Value::null());
    };
    let wanted = string_arg(scope, arg)?;
    let length = scope.get(this, "length")?;
    let length = scope.to_int32(&length)? as u32;
    for index in 0..length {
        let entry = scope.get_index(this, index)?;
        let name = scope.get(&entry, "name")?;
        let name = string_arg(scope, &name)?;
        if name.to_bytes().eq_ignore_ascii_case(wanted.to_bytes()) {
            return Ok(entry);
        }
    }
    Ok(Value::null())
}

pub(crate) fn item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(arg) = args.first() else {
        return Ok(Value::null());
    };
    let index = scope.to_int32(arg)? as u32;
    let entry = scope.get_index(this, index)?;
    Ok(if entry.is_undefined() {
        Value::null()
    } else {
        entry
    })
}

pub(crate) fn get_length(_scope: &mut Scope<'_>, this: &Value, _args: &[Value]) -> JsResult {
    let count = ffi::named_map_owner(this)
        .filter(|n| n.is_element())
        .map_or(0, |n| {
            n.attrs()
                .filter(|a| {
                    a.name()
                        .is_some_and(|name| !attrs::is_internal(name.to_bytes()))
                })
                .count()
        });
    Ok(Value::int(count as i32))
}

pub(crate) fn get_named_item_ns(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    if args.len() < 2 {
        return Ok(Value::null());
    }
    let node = ffi::named_map_owner(this);
    let namespace_uri = optional_string_arg(scope, &args[0])?;
    let local = string_arg(scope, &args[1])?;
    let Some(node) = node else {
        return Ok(Value::null());
    };
    let Some(found) = page_attr(node, namespace_uri.as_deref(), &local) else {
        return Ok(Value::null());
    };
    let owner = ffi::wrap_node(scope, node);
    attr::to_js(scope, &owner, found, true)
}

pub(crate) fn set_named_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(node) = ffi::named_map_owner(this) else {
        return Err(scope.type_error("NamedNodeMap has no owner element"));
    };
    let owner = ffi::wrap_node(scope, node);
    attr::set_attribute_node(scope, &owner, args)
}

pub(crate) fn remove_named_item(scope: &mut Scope<'_>, this: &Value, args: &[Value]) -> JsResult {
    let Some(arg) = args.first() else {
        return Err(scope.type_error("1 argument required"));
    };
    let node = ffi::named_map_owner(this);
    let name = string_arg(scope, arg)?;
    let found = node.filter(|n| n.is_element()).and_then(|n| {
        n.attrs().find(|a| {
            a.name().is_some_and(|attr_name| {
                !attrs::is_internal(attr_name.to_bytes())
                    && attr_name.to_bytes().eq_ignore_ascii_case(name.to_bytes())
            })
        })
    });
    let (Some(node), Some(found)) = (node, found) else {
        return Err(ffi::dom_exception(
            scope,
            "NotFoundError",
            NOT_FOUND_ERR,
            "no attribute with that name",
        ));
    };
    let owner = ffi::wrap_node(scope, node);
    let removed = attr::to_js(scope, &owner, found, false)?;
    ffi::remove_attr(ffi::js_of(scope), node, &name);
    Ok(removed)
}

pub(crate) fn remove_named_item_ns(
    scope: &mut Scope<'_>,
    this: &Value,
    args: &[Value],
) -> JsResult {
    if args.len() < 2 {
        return Err(scope.type_error("2 arguments required"));
    }
    let node = ffi::named_map_owner(this);
    let namespace_uri = optional_string_arg(scope, &args[0])?.filter(|ns| !ns.is_empty());
    let local = string_arg(scope, &args[1])?;
    let found = node.and_then(|n| page_attr(n, namespace_uri.as_deref(), &local));
    let (Some(node), Some(found)) = (node, found) else {
        return Err(ffi::dom_exception(
            scope,
            "NotFoundError",
            NOT_FOUND_ERR,
            "no attribute with that namespace and local name",
        ));
    };
    let owner = ffi::wrap_node(scope, node);
    let removed = attr::to_js(scope, &owner, found, false)?;
    ffi::remove_attr_ns(ffi::js_of(scope), node, namespace_uri.as_deref(), &local);
    Ok(removed)
}
