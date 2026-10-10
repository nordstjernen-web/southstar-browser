//! Southstar — node names: nodeName, tagName, localName, prefix and namespaceURI, built without allocating.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{FLAG_FOREIGN_NS, FLAG_PI, FLAG_SVG_NS, Kind};
use southstar_js_engine::{Scope, Value};

use crate::tree::is_shadow_root;
use crate::{Element, FLAG_CDATA, FLAG_FRAGMENT};

const FLAG_KEEP_CASE: u32 = 1 << 12;
const FLAG_XML_DOC: u32 = 1 << 14;
const NS_URI_ATTR: &CStr = c"data-nd-ns-uri";
const NS_PREFIX_ATTR: &CStr = c"data-nd-ns-prefix";
const SVG_NS: &[u8] = b"http://www.w3.org/2000/svg";
const XHTML_NS: &[u8] = b"http://www.w3.org/1999/xhtml";
const PREFIX_LIMIT: usize = 64;
const INLINE: usize = 128;

pub(crate) enum Text<'a> {
    Null,
    Plain(&'a [u8]),
    Upper(&'a [u8]),
    Qualified(&'a [u8], &'a [u8]),
    UpperQualified(&'a [u8], &'a [u8]),
}

pub(crate) fn to_value(scope: &mut Scope<'_>, text: Text<'_>) -> Value {
    let (prefix, name, upper) = match text {
        Text::Null => return Value::null(),
        Text::Plain(name) => return scope.string_from_bytes(name),
        Text::Upper(name) => (None, name, true),
        Text::Qualified(prefix, name) => (Some(prefix), name, false),
        Text::UpperQualified(prefix, name) => (Some(prefix), name, true),
    };
    let len = prefix.map_or(0, |p| p.len() + 1) + name.len();
    let mut inline = [0u8; INLINE];
    let mut heap = Vec::new();
    let out: &mut [u8] = if len <= INLINE {
        &mut inline[..len]
    } else {
        heap.resize(len, 0);
        &mut heap
    };
    let mut at = 0;
    if let Some(prefix) = prefix {
        out[..prefix.len()].copy_from_slice(prefix);
        out[prefix.len()] = b':';
        at = prefix.len() + 1;
    }
    out[at..].copy_from_slice(name);
    if upper {
        out.make_ascii_uppercase();
    }
    scope.string_from_bytes(out)
}

fn bytes(text: Option<&CStr>) -> Option<&[u8]> {
    text.map(CStr::to_bytes)
}

fn foreign(node: Element) -> bool {
    node.flags() & (FLAG_SVG_NS | FLAG_FOREIGN_NS) != 0
}

fn split_colon(name: &[u8]) -> Option<(&[u8], &[u8])> {
    let colon = name.iter().position(|&b| b == b':')?;
    Some((&name[..colon], &name[colon + 1..]))
}

pub(crate) fn namespace_uri(node: Element) -> Option<&'static [u8]> {
    if !node.is_element() {
        return None;
    }
    if let Some(stored) = node.attr(NS_URI_ATTR) {
        return Some(stored.to_bytes());
    }
    if node.flags() & FLAG_SVG_NS != 0 {
        return Some(SVG_NS);
    }
    if node.flags() & (FLAG_FOREIGN_NS | FLAG_XML_DOC) != 0 {
        return None;
    }
    Some(XHTML_NS)
}

pub(crate) fn prefix(node: Element) -> Option<&'static [u8]> {
    if !node.is_element() {
        return None;
    }
    if let Some(stored) = node.attr(NS_PREFIX_ATTR) {
        return Some(stored.to_bytes());
    }
    split_colon(bytes(node.name())?)
        .map(|(prefix, _)| prefix)
        .filter(|prefix| prefix.len() < PREFIX_LIMIT)
}

pub(crate) fn local_name(node: Element) -> Option<&'static [u8]> {
    if !node.is_element() {
        return None;
    }
    let name = bytes(node.name())?;
    Some(split_colon(name).map_or(name, |(_, local)| local))
}

pub(crate) fn node_name(node: Option<Element>) -> Text<'static> {
    let Some(node) = node else {
        return Text::Plain(b"#text");
    };
    if node.flags() & FLAG_FRAGMENT != 0 || is_shadow_root(node) {
        return Text::Plain(b"#document-fragment");
    }
    match node.kind() {
        Kind::Text if node.flags() & FLAG_CDATA != 0 => return Text::Plain(b"#cdata-section"),
        Kind::Text => return Text::Plain(b"#text"),
        Kind::Comment => {
            return match bytes(node.name()).filter(|_| node.flags() & FLAG_PI != 0) {
                Some(name) => Text::Plain(name),
                None => Text::Plain(b"#comment"),
            };
        }
        Kind::Document => return Text::Plain(b"#document"),
        Kind::Doctype => return Text::Plain(bytes(node.name()).unwrap_or(b"html")),
        Kind::Element | Kind::Other => {}
    }
    let Some(name) = bytes(node.name()) else {
        return Text::Plain(b"");
    };
    if node.flags() & FLAG_KEEP_CASE != 0 {
        return Text::Plain(name);
    }
    if let Some(prefix) = node.attr(NS_PREFIX_ATTR) {
        return Text::UpperQualified(prefix.to_bytes(), name);
    }
    if foreign(node) || name.iter().any(u8::is_ascii_uppercase) {
        return Text::Plain(name);
    }
    Text::Upper(name)
}

pub(crate) fn tag_name(node: Option<Element>, doc_is_xml: impl FnOnce() -> bool) -> Text<'static> {
    let Some(node) = node else {
        return Text::Null;
    };
    let Some(name) = bytes(node.name()) else {
        return Text::Null;
    };
    if foreign(node) || doc_is_xml() {
        return match node.attr(NS_PREFIX_ATTR).map(CStr::to_bytes) {
            Some(prefix) if !prefix.is_empty() => Text::Qualified(prefix, name),
            _ => Text::Plain(name),
        };
    }
    if node.is_element()
        && let Some(prefix) = node.attr(NS_PREFIX_ATTR)
    {
        return Text::UpperQualified(prefix.to_bytes(), name);
    }
    Text::Upper(name)
}

fn namespaced_name(node: Element) -> Option<(&'static [u8], &'static [u8])> {
    if !foreign(node) || node.attr(NS_URI_ATTR).is_none() {
        return None;
    }
    split_colon(bytes(node.name())?)
}

pub(crate) fn local_name_of(node: Option<Element>) -> Text<'static> {
    let Some(node) = node else {
        return Text::Null;
    };
    let Some(name) = bytes(node.name()) else {
        return Text::Null;
    };
    match namespaced_name(node) {
        Some((_, local)) => Text::Plain(local),
        None => Text::Plain(name),
    }
}

pub(crate) fn prefix_of(node: Option<Element>) -> Text<'static> {
    let Some(node) = node.filter(|n| n.name().is_some()) else {
        return Text::Null;
    };
    if node.is_element()
        && let Some(stored) = node.attr(NS_PREFIX_ATTR)
    {
        return Text::Plain(stored.to_bytes());
    }
    match namespaced_name(node) {
        Some((prefix, _)) => Text::Plain(prefix),
        None => Text::Null,
    }
}

pub(crate) fn namespace_uri_of(node: Option<Element>) -> Text<'static> {
    if let Some(stored) = node
        .filter(|n| n.is_element())
        .and_then(|n| n.attr(NS_URI_ATTR))
    {
        return Text::Plain(stored.to_bytes());
    }
    match node.map(Element::flags) {
        Some(flags) if flags & FLAG_SVG_NS != 0 => Text::Plain(SVG_NS),
        Some(flags) if flags & FLAG_FOREIGN_NS != 0 => Text::Null,
        _ => Text::Plain(XHTML_NS),
    }
}
