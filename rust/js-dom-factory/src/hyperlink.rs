//! Southstar — the HTMLHyperlinkElementUtils accessors on a and area: href and its protocol, host, hostname, port, pathname, search, hash, origin, username and password parts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};

use southstar_dom::Kind;
use southstar_js_engine::{Scope, Value};

use crate::ffi::{self, Js};
use crate::{Element, JsResult, PLAIN};

pub(crate) const HREF: c_int = 0;
pub(crate) const PROTOCOL: c_int = 1;
pub(crate) const HOST: c_int = 2;
pub(crate) const HOSTNAME: c_int = 3;
pub(crate) const PORT: c_int = 4;
pub(crate) const PATHNAME: c_int = 5;
pub(crate) const SEARCH: c_int = 6;
pub(crate) const HASH: c_int = 7;
pub(crate) const ORIGIN: c_int = 8;
pub(crate) const USERNAME: c_int = 9;
pub(crate) const PASSWORD: c_int = 10;

fn is_url_element(node: Option<Element>) -> bool {
    node.filter(|n| n.kind() == Kind::Element)
        .and_then(|n| n.name())
        .is_some_and(|name| matches!(name.to_bytes(), b"a" | b"area"))
}

fn is_hyperlink(node: Element) -> bool {
    node.kind() == Kind::Element
        && node.name().is_some_and(|name| {
            let name = name.to_bytes();
            name.eq_ignore_ascii_case(b"a") || name.eq_ignore_ascii_case(b"area")
        })
}

fn anchor_url(node: Element, js: Option<Js>) -> Option<Vec<u8>> {
    let raw = ffi::attr(node, c"href")?;
    let base = ffi::doc_base_url(js).filter(|b| !b.is_empty());
    ffi::resolve_url(base.as_deref(), raw)
}

pub(crate) fn resolved_href(node: Element, js: Option<Js>) -> Option<Vec<u8>> {
    anchor_url(node, js).or_else(|| ffi::attr(node, c"href").map(|raw| ffi::c_prefix(raw).to_vec()))
}

pub(crate) fn part_get(scope: &mut Scope<'_>, this: &Value, magic: c_int) -> JsResult {
    let node = ffi::unwrap_node(this);
    let js = ffi::js_of(scope);
    let Some(node) = node.filter(|_| is_url_element(node)) else {
        if magic != HREF {
            return Ok(Value::undefined());
        }
        let Some(raw) = node.and_then(|n| ffi::attr(n, c"href")) else {
            return Ok(scope.string(""));
        };
        let resolved = node.and_then(|n| resolved_href(n, js));
        let out = resolved.unwrap_or_else(|| ffi::c_prefix(raw).to_vec());
        return Ok(scope.string_from_bytes(&out));
    };
    let href = anchor_url(node, js);
    let part = href.as_deref().and_then(|h| ffi::url_part(h, magic));
    let (Some(href), Some(part)) = (href, part) else {
        return match magic {
            HREF => ffi::reflect_href_get(scope, this),
            PROTOCOL => Ok(scope.string(":")),
            _ => Ok(scope.string("")),
        };
    };
    if magic == HREF {
        return Ok(scope.string_from_bytes(&href));
    }
    Ok(scope.string_from_bytes(&part))
}

fn component(magic: c_int) -> Option<&'static CStr> {
    Some(match magic {
        PROTOCOL => c"protocol",
        HOST => c"host",
        HOSTNAME => c"hostname",
        PORT => c"port",
        PATHNAME => c"pathname",
        SEARCH => c"search",
        HASH => c"hash",
        USERNAME => c"username",
        PASSWORD => c"password",
        _ => return None,
    })
}

pub(crate) fn part_set(
    scope: &mut Scope<'_>,
    this: &Value,
    val: &Value,
    magic: c_int,
) -> JsResult<()> {
    let Some(name) = component(magic) else {
        return Ok(());
    };
    let Some(node) = ffi::unwrap_node(this).filter(|n| is_hyperlink(*n)) else {
        let key = name.to_str().unwrap_or_default();
        let _ = scope.define(this, key, val.clone(), PLAIN);
        return Ok(());
    };
    let value = scope.to_bytes(val)?;
    let js = ffi::js_of(scope);
    let next = anchor_url(node, js).and_then(|href| ffi::set_url_component(&href, name, &value));
    if let Some(next) = next {
        ffi::set_attr_recorded(js, node, c"href", &next);
    }
    Ok(())
}
