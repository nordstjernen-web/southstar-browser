//! Southstar — text collection, HTML and XML serialization and the debug dump of DOM subtrees.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_html_util::{escape_append, is_raw_text, is_void};

use crate::ffi::{Kind, Node};
use crate::{FLAG_FOREIGN_NS, FLAG_PI, FLAG_SCRIPTING_DISABLED, FLAG_SVG_NS, MAX_DEPTH, children};

const SHADOW_ATTR: &CStr = c"data-nd-shadow-root";
const HTML_NAMESPACE: &[u8] = b"http://www.w3.org/1999/xhtml";
const SVG_NAMESPACE: &[u8] = b"http://www.w3.org/2000/svg";

pub struct Options<'a> {
    pub include_serializable: bool,
    pub roots: Vec<Node<'a>>,
}

fn bytes(s: Option<&CStr>) -> &[u8] {
    s.map_or(&[], CStr::to_bytes)
}

fn is_internal_attr(name: Option<&CStr>) -> bool {
    crate::attrs::is_internal(bytes(name))
}

fn collect_text_into(node: Node, out: &mut Vec<u8>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    if node.is_text() {
        out.extend_from_slice(bytes(node.text()));
        return;
    }
    if matches!(
        node.element_name(),
        Some(b"style" | b"script" | b"noscript" | b"template")
    ) {
        return;
    }
    for child in children(node) {
        collect_text_into(child, out, depth + 1);
    }
}

pub fn collect_text(root: Option<Node>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(root) = root {
        collect_text_into(root, &mut out, 0);
    }
    out
}

impl Node<'_> {
    pub fn collect_text(self) -> Vec<u8> {
        collect_text(Some(self))
    }
}

fn is_shadow_root_marked(node: Node) -> bool {
    node.is_element() && node.attr(SHADOW_ATTR).is_some()
}

pub fn is_embedded_doc(node: Node) -> bool {
    node.kind() == Kind::Document && node.parent().is_some()
}

fn collect_all_text_into(node: Node, out: &mut Vec<u8>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    if node.is_text() {
        out.extend_from_slice(node.text_with_len().unwrap_or_default());
        return;
    }
    for child in children(node) {
        if !is_shadow_root_marked(child) && !is_embedded_doc(child) {
            collect_all_text_into(child, out, depth + 1);
        }
    }
}

pub fn collect_all_text(root: Option<Node>) -> Vec<u8> {
    let Some(root) = root else { return Vec::new() };
    let Some(first) = root.first_child() else {
        return Vec::new();
    };
    if root.last_child() == Some(first) && first.is_text() {
        return first.text_with_len().unwrap_or_default().to_vec();
    }
    let mut out = Vec::new();
    collect_all_text_into(root, &mut out, 0);
    out
}

fn pre_like(name: &[u8]) -> bool {
    [&b"pre"[..], b"textarea", b"listing"]
        .iter()
        .any(|tag| name.eq_ignore_ascii_case(tag))
}

fn raw_text_element(node: Node) -> bool {
    let Some(name) = node.element_name() else {
        return false;
    };
    if !is_raw_text(name) {
        return false;
    }
    if !name.eq_ignore_ascii_case(b"noscript") {
        return true;
    }
    node.root().flags() & FLAG_SCRIPTING_DISABLED == 0
}

fn shadow_child(host: Node) -> Option<Node> {
    children(host).find(|child| is_shadow_root_marked(*child))
}

fn shadow_included(root: Node, opts: Option<&Options>) -> bool {
    let Some(opts) = opts else { return false };
    opts.roots.contains(&root)
        || (opts.include_serializable && root.attr(c"data-nd-shadow-serializable").is_some())
}

fn serialize_shadow_template(root: Node, out: &mut Vec<u8>, depth: i32, opts: Option<&Options>) {
    out.extend_from_slice(b"<template shadowrootmode=\"");
    out.extend_from_slice(root.attr(SHADOW_ATTR).map_or(&b"open"[..], CStr::to_bytes));
    out.push(b'"');
    if root.attr(c"data-nd-shadow-delegates").is_some() {
        out.extend_from_slice(b" shadowrootdelegatesfocus=\"\"");
    }
    if root.attr(c"data-nd-shadow-serializable").is_some() {
        out.extend_from_slice(b" shadowrootserializable=\"\"");
    }
    if root.attr(c"data-nd-shadow-clonable").is_some() {
        out.extend_from_slice(b" shadowrootclonable=\"\"");
    }
    out.push(b'>');
    for child in children(root) {
        serialize_node(child, out, true, depth + 1, opts);
    }
    out.extend_from_slice(b"</template>");
}

fn serialize_leaf(node: Node, out: &mut Vec<u8>) -> bool {
    match node.kind() {
        Kind::Text => escape_append(out, bytes(node.text()), false),
        Kind::Comment if node.flags() & FLAG_PI != 0 => {
            out.extend_from_slice(b"<?");
            out.extend_from_slice(bytes(node.name()));
            out.push(b' ');
            out.extend_from_slice(bytes(node.text()));
            out.push(b'>');
        }
        Kind::Comment => {
            out.extend_from_slice(b"<!--");
            out.extend_from_slice(bytes(node.text()));
            out.extend_from_slice(b"-->");
        }
        Kind::Doctype => {
            out.extend_from_slice(b"<!DOCTYPE ");
            out.extend_from_slice(bytes(node.name()));
            out.push(b'>');
        }
        _ => return false,
    }
    true
}

fn serialize_start_tag(node: Node, out: &mut Vec<u8>) -> bool {
    let name = bytes(node.name());
    out.push(b'<');
    out.extend_from_slice(name);
    for attr in node.attrs() {
        if is_internal_attr(attr.name()) {
            continue;
        }
        out.push(b' ');
        out.extend_from_slice(bytes(attr.name()));
        out.extend_from_slice(b"=\"");
        escape_append(out, bytes(attr.value()), true);
        out.push(b'"');
    }
    out.push(b'>');
    if node.name().is_some() && is_void(name) {
        return false;
    }
    let leading_newline = node
        .first_child()
        .filter(|child| child.is_text())
        .and_then(|child| child.text())
        .is_some_and(|text| text.to_bytes().first() == Some(&b'\n'));
    if pre_like(name) && leading_newline {
        out.push(b'\n');
    }
    true
}

fn serialize_children(
    node: Node,
    shadow: Option<Node>,
    raw_text: bool,
    out: &mut Vec<u8>,
    depth: i32,
    opts: Option<&Options>,
) {
    for child in children(node) {
        if Some(child) == shadow || is_embedded_doc(child) {
            continue;
        }
        if raw_text && child.is_text() {
            out.extend_from_slice(bytes(child.text()));
        } else {
            serialize_node(child, out, true, depth + 1, opts);
        }
    }
}

fn serialize_node(
    node: Node,
    out: &mut Vec<u8>,
    include_self: bool,
    depth: i32,
    opts: Option<&Options>,
) {
    if depth >= MAX_DEPTH || serialize_leaf(node, out) {
        return;
    }
    let raw_text = raw_text_element(node);
    let tag = include_self && node.is_element();
    if tag && !serialize_start_tag(node, out) {
        return;
    }
    if let Some(content) = node.tpl_content() {
        for child in children(content) {
            serialize_node(child, out, true, depth + 1, opts);
        }
    }
    let shadow = shadow_child(node);
    if let Some(root) = shadow.filter(|root| shadow_included(*root, opts)) {
        serialize_shadow_template(root, out, depth, opts);
    }
    serialize_children(node, shadow, raw_text, out, depth, opts);
    if tag {
        out.extend_from_slice(b"</");
        out.extend_from_slice(bytes(node.name()));
        out.push(b'>');
    }
}

pub fn get_html(root: Option<Node>, opts: Option<&Options>) -> Vec<u8> {
    let mut out = Vec::new();
    let Some(root) = root else { return out };
    if root.element_name().is_some_and(is_void) {
        return out;
    }
    let raw_text = raw_text_element(root);
    let root = root.tpl_content().unwrap_or(root);
    let shadow = shadow_child(root);
    if let Some(shadow_root) = shadow.filter(|sr| shadow_included(*sr, opts)) {
        serialize_shadow_template(shadow_root, &mut out, 0, opts);
    }
    serialize_children(root, shadow, raw_text, &mut out, -1, opts);
    out
}

pub fn outer_html(node: Option<Node>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(node) = node {
        serialize_node(node, &mut out, true, 0, None);
    }
    out
}

fn element_namespace(node: Node<'_>) -> &[u8] {
    if node.flags() & FLAG_SVG_NS != 0 {
        return SVG_NAMESPACE;
    }
    if node.flags() & FLAG_FOREIGN_NS != 0 {
        return bytes(node.attr(c"data-nd-ns-uri"));
    }
    HTML_NAMESPACE
}

fn xml_escape(out: &mut Vec<u8>, s: &[u8], attr: bool) {
    for &c in s {
        match c {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'"' if attr => out.extend_from_slice(b"&quot;"),
            _ => out.push(c),
        }
    }
}

fn xml_qualified_name(out: &mut Vec<u8>, prefix: &[u8], name: &[u8]) {
    if !prefix.is_empty() {
        out.extend_from_slice(prefix);
        out.push(b':');
    }
    out.extend_from_slice(name);
}

fn xml_serialize(node: Node, out: &mut Vec<u8>, parent_ns: Option<&[u8]>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    match node.kind() {
        Kind::Text => return xml_escape(out, bytes(node.text()), false),
        Kind::Comment if node.flags() & FLAG_PI != 0 => {
            out.extend_from_slice(b"<?");
            out.extend_from_slice(bytes(node.name()));
            let text = bytes(node.text());
            if !text.is_empty() {
                out.push(b' ');
                out.extend_from_slice(text);
            }
            return out.extend_from_slice(b"?>");
        }
        Kind::Comment => {
            out.extend_from_slice(b"<!--");
            out.extend_from_slice(bytes(node.text()));
            return out.extend_from_slice(b"-->");
        }
        Kind::Element => {}
        _ => return,
    }
    let ns = element_namespace(node);
    let prefix = bytes(node.attr(c"data-nd-ns-prefix"));
    let name = bytes(node.name());
    out.push(b'<');
    xml_qualified_name(out, prefix, name);
    if parent_ns != Some(ns) {
        out.extend_from_slice(if prefix.is_empty() {
            b" xmlns"
        } else {
            b" xmlns:"
        });
        out.extend_from_slice(prefix);
        out.extend_from_slice(b"=\"");
        xml_escape(out, ns, true);
        out.push(b'"');
    }
    for attr in node.attrs() {
        if is_internal_attr(attr.name()) {
            continue;
        }
        out.push(b' ');
        out.extend_from_slice(bytes(attr.name()));
        out.extend_from_slice(b"=\"");
        xml_escape(out, bytes(attr.value()), true);
        out.push(b'"');
    }
    if node.first_child().is_none() {
        if ns == HTML_NAMESPACE && !(node.name().is_some() && is_void(name)) {
            out.extend_from_slice(b"></");
            xml_qualified_name(out, prefix, name);
            out.push(b'>');
        } else {
            out.extend_from_slice(b" />");
        }
        return;
    }
    out.push(b'>');
    for child in children(node) {
        if !is_embedded_doc(child) {
            xml_serialize(child, out, Some(ns), depth + 1);
        }
    }
    out.extend_from_slice(b"</");
    xml_qualified_name(out, prefix, name);
    out.push(b'>');
}

pub fn xml_outer_html(node: Option<Node>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(node) = node {
        xml_serialize(node, &mut out, None, 0);
    }
    out
}

fn dump_text(out: &mut Vec<u8>, s: Option<&CStr>, max: usize) {
    let Some(s) = s else { return };
    let s = s.to_bytes();
    let truncated = max > 0 && s.len() > max;
    for &c in &s[..if truncated { max } else { s.len() }] {
        match c {
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b'\r' => out.extend_from_slice(b"\\r"),
            b'\t' => out.extend_from_slice(b"\\t"),
            0..0x20 => out.extend_from_slice(format!("\\x{c:02x}").as_bytes()),
            _ => out.push(c),
        }
    }
    if truncated {
        out.extend_from_slice("…".as_bytes());
    }
}

fn dump_node(node: Node, out: &mut Vec<u8>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for _ in 0..depth {
        out.extend_from_slice(b"  ");
    }
    match node.kind() {
        Kind::Document => out.extend_from_slice(b"#document\n"),
        Kind::Doctype => {
            out.extend_from_slice(b"<!DOCTYPE ");
            out.extend_from_slice(bytes(node.name()));
            out.extend_from_slice(b">\n");
        }
        Kind::Element => {
            out.push(b'<');
            out.extend_from_slice(node.name().map_or(&b"?"[..], CStr::to_bytes));
            for attr in node.attrs() {
                out.push(b' ');
                out.extend_from_slice(attr.name().map_or(&b"(null)"[..], CStr::to_bytes));
                out.extend_from_slice(b"=\"");
                dump_text(out, attr.value(), 0);
                out.push(b'"');
            }
            out.extend_from_slice(b">\n");
        }
        Kind::Text => {
            out.push(b'"');
            dump_text(out, node.text(), 120);
            out.extend_from_slice(b"\"\n");
        }
        Kind::Comment if node.flags() & FLAG_PI != 0 => {
            out.extend_from_slice(b"<?");
            out.extend_from_slice(bytes(node.name()));
            out.push(b' ');
            dump_text(out, node.text(), 120);
            out.extend_from_slice(b"?>\n");
        }
        Kind::Comment => {
            out.extend_from_slice(b"<!--");
            dump_text(out, node.text(), 120);
            out.extend_from_slice(b"-->\n");
        }
        Kind::Other => {}
    }
    for child in children(node) {
        dump_node(child, out, depth + 1);
    }
}

pub fn dump(node: Option<Node>) -> Vec<u8> {
    let mut out = Vec::new();
    if let Some(node) = node {
        dump_node(node, &mut out, 0);
    }
    out
}
