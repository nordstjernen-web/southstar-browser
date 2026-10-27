//! Southstar — text collection, HTML and XML serialization and the debug dump of DOM subtrees.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_html_util::{escape_append, is_raw_text, is_void};

use crate::{FLAG_FOREIGN_NS, FLAG_PI, FLAG_SCRIPTING_DISABLED, FLAG_SVG_NS, MAX_DEPTH, children};
use crate::{Kind, Node};

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

pub fn is_shadow_root_marked(node: Node) -> bool {
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
        html_attr_name(out, attr);
        out.extend_from_slice(b"=\"");
        escape_append(out, bytes(attr.value()), true);
        out.push(b'"');
    }
    out.push(b'>');
    if node.name().is_some() && is_void(name) {
        return false;
    }
    true
}

fn html_attr_name(out: &mut Vec<u8>, attr: crate::Attr<'_>) {
    let ns = attr.namespace_uri().map(CStr::to_bytes);
    let local = crate::attrs::local_name(attr).to_bytes();
    let prefix: Option<&[u8]> = match ns {
        Some(b"http://www.w3.org/XML/1998/namespace") => Some(b"xml"),
        Some(b"http://www.w3.org/2000/xmlns/") if local == b"xmlns" => None,
        Some(b"http://www.w3.org/2000/xmlns/") => Some(b"xmlns"),
        Some(b"http://www.w3.org/1999/xlink") => Some(b"xlink"),
        _ => {
            out.extend_from_slice(bytes(attr.name()));
            return;
        }
    };
    if let Some(prefix) = prefix {
        out.extend_from_slice(prefix);
        out.push(b':');
    }
    out.extend_from_slice(local);
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

const XML_NS: &[u8] = b"http://www.w3.org/XML/1998/namespace";
const XMLNS_NS: &[u8] = b"http://www.w3.org/2000/xmlns/";
const XML_FLAG_FRAGMENT: u32 = 1 << 2;
const XML_FLAG_CDATA: u32 = 1 << 10;

type PrefixEntry = (Option<Vec<u8>>, Vec<Vec<u8>>);

#[derive(Clone, Default)]
struct PrefixMap(Vec<PrefixEntry>);

impl PrefixMap {
    fn candidates(&self, ns: Option<&[u8]>) -> Option<&Vec<Vec<u8>>> {
        self.0
            .iter()
            .find(|(key, _)| key.as_deref() == ns)
            .map(|(_, prefixes)| prefixes)
    }

    fn contains(&self, ns: Option<&[u8]>, prefix: &[u8]) -> bool {
        self.candidates(ns)
            .is_some_and(|prefixes| prefixes.iter().any(|p| p == prefix))
    }

    fn add(&mut self, ns: Option<&[u8]>, prefix: &[u8]) {
        match self.0.iter_mut().find(|(key, _)| key.as_deref() == ns) {
            Some((_, prefixes)) => prefixes.push(prefix.to_vec()),
            None => self.0.push((ns.map(<[u8]>::to_vec), vec![prefix.to_vec()])),
        }
    }

    fn preferred(&self, ns: Option<&[u8]>, preferred: Option<&[u8]>) -> Option<Vec<u8>> {
        let candidates = self.candidates(ns)?;
        if let Some(found) = candidates.iter().find(|p| Some(p.as_slice()) == preferred) {
            return Some(found.clone());
        }
        candidates.last().cloned()
    }
}

fn xml_element_namespace(node: Node<'_>) -> Option<&[u8]> {
    if let Some(stored) = node.attr(c"data-nd-ns-uri") {
        return Some(stored.to_bytes()).filter(|ns| !ns.is_empty());
    }
    if node.flags() & FLAG_SVG_NS != 0 {
        return Some(SVG_NAMESPACE);
    }
    if node.flags() & FLAG_FOREIGN_NS != 0 {
        return None;
    }
    Some(HTML_NAMESPACE)
}

fn split_qualified(name: &[u8]) -> (Option<&[u8]>, &[u8]) {
    match name.iter().position(|&c| c == b':') {
        Some(colon) => (Some(&name[..colon]), &name[colon + 1..]),
        None => (None, name),
    }
}

fn xml_element_prefix(node: Node<'_>) -> Option<&[u8]> {
    if let Some(stored) = node.attr(c"data-nd-ns-prefix") {
        return Some(stored.to_bytes()).filter(|p| !p.is_empty());
    }
    split_qualified(bytes(node.name())).0
}

fn xml_local_name(node: Node<'_>) -> &[u8] {
    split_qualified(bytes(node.name())).1
}

struct XmlAttr<'a> {
    ns: Option<&'a [u8]>,
    prefix: Option<&'a [u8]>,
    local: &'a [u8],
    value: &'a [u8],
}

fn xml_attrs(node: Node<'_>) -> Vec<XmlAttr<'_>> {
    node.attrs()
        .filter(|attr| !is_internal_attr(attr.name()))
        .map(|attr| {
            let name = bytes(attr.name());
            let ns = attr
                .namespace_uri()
                .map(CStr::to_bytes)
                .filter(|ns| !ns.is_empty());
            let (prefix, local) = match attr.local_name() {
                Some(local) => (attr.prefix().map(CStr::to_bytes), local.to_bytes()),
                None if ns.is_some() => split_qualified(name),
                None => (None, name),
            };
            XmlAttr {
                ns,
                prefix: prefix.filter(|p| !p.is_empty()),
                local,
                value: bytes(attr.value()),
            }
        })
        .collect()
}

fn xml_text_escape(out: &mut Vec<u8>, s: &[u8]) {
    for &c in s {
        match c {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            _ => out.push(c),
        }
    }
}

fn xml_attr_escape(out: &mut Vec<u8>, s: &[u8]) {
    for &c in s {
        match c {
            b'&' => out.extend_from_slice(b"&amp;"),
            b'"' => out.extend_from_slice(b"&quot;"),
            b'<' => out.extend_from_slice(b"&lt;"),
            b'>' => out.extend_from_slice(b"&gt;"),
            b'\t' => out.extend_from_slice(b"&#x9;"),
            b'\n' => out.extend_from_slice(b"&#xA;"),
            b'\r' => out.extend_from_slice(b"&#xD;"),
            _ => out.push(c),
        }
    }
}

struct XmlWriter {
    out: Vec<u8>,
    index: u32,
}

impl XmlWriter {
    fn generate_prefix(&mut self, map: &mut PrefixMap, ns: Option<&[u8]>) -> Vec<u8> {
        let prefix = format!("ns{}", self.index).into_bytes();
        self.index += 1;
        map.add(ns, &prefix);
        prefix
    }

    fn node(&mut self, node: Node<'_>, ns: Option<&[u8]>, map: &PrefixMap, depth: i32) {
        if depth >= MAX_DEPTH {
            return;
        }
        match node.kind() {
            Kind::Element => self.element(node, ns, map, depth),
            Kind::Document => {
                for child in children(node) {
                    if !is_embedded_doc(child) {
                        self.node(child, ns, map, depth + 1);
                    }
                }
            }
            Kind::Doctype => {
                self.out.extend_from_slice(b"<!DOCTYPE ");
                self.out.extend_from_slice(bytes(node.name()));
                let public = bytes(node.attr(c"publicId"));
                let system = bytes(node.attr(c"systemId"));
                if !public.is_empty() {
                    self.out.extend_from_slice(b" PUBLIC \"");
                    self.out.extend_from_slice(public);
                    self.out.push(b'"');
                } else if !system.is_empty() {
                    self.out.extend_from_slice(b" SYSTEM");
                }
                if !system.is_empty() {
                    self.out.extend_from_slice(b" \"");
                    self.out.extend_from_slice(system);
                    self.out.push(b'"');
                }
                self.out.push(b'>');
            }
            Kind::Text if node.flags() & XML_FLAG_CDATA != 0 => {
                self.out.extend_from_slice(b"<![CDATA[");
                self.out.extend_from_slice(bytes(node.text()));
                self.out.extend_from_slice(b"]]>");
            }
            Kind::Text => xml_text_escape(&mut self.out, bytes(node.text())),
            Kind::Comment if node.flags() & FLAG_PI != 0 => {
                self.out.extend_from_slice(b"<?");
                self.out.extend_from_slice(bytes(node.name()));
                self.out.push(b' ');
                self.out.extend_from_slice(bytes(node.text()));
                self.out.extend_from_slice(b"?>");
            }
            Kind::Comment => {
                self.out.extend_from_slice(b"<!--");
                self.out.extend_from_slice(bytes(node.text()));
                self.out.extend_from_slice(b"-->");
            }
            Kind::Other => {}
        }
    }

    fn element(&mut self, node: Node<'_>, inherited: Option<&[u8]>, outer: &PrefixMap, depth: i32) {
        let mut map = outer.clone();
        let attrs = xml_attrs(node);
        let mut local_prefixes: Vec<(Vec<u8>, Option<Vec<u8>>)> = Vec::new();
        let mut local_default: Option<&[u8]> = None;
        for attr in attrs.iter().filter(|a| a.ns == Some(XMLNS_NS)) {
            match attr.prefix {
                None => local_default = Some(attr.value),
                Some(_) => {
                    let definition = Some(attr.value).filter(|v| !v.is_empty());
                    if definition == Some(XML_NS) || map.contains(definition, attr.local) {
                        continue;
                    }
                    map.add(definition, attr.local);
                    local_prefixes.push((attr.local.to_vec(), definition.map(<[u8]>::to_vec)));
                }
            }
        }
        let local_default_ns =
            |d: &[u8]| -> Option<Vec<u8>> { Some(d.to_vec()).filter(|d| !d.is_empty()) };
        let mut inherited: Option<Vec<u8>> = inherited.map(<[u8]>::to_vec);
        let ns = xml_element_namespace(node);
        let local = xml_local_name(node);
        let mut ignore_default_attr = false;
        let qualified: Vec<u8>;
        self.out.push(b'<');
        if inherited.as_deref() == ns {
            if local_default.is_some() {
                ignore_default_attr = true;
            }
            qualified = if ns == Some(XML_NS) {
                [b"xml:", local].concat()
            } else {
                local.to_vec()
            };
            self.out.extend_from_slice(&qualified);
        } else {
            let mut prefix = xml_element_prefix(node).map(<[u8]>::to_vec);
            let mut candidate = map.preferred(ns, prefix.as_deref());
            if prefix.as_deref() == Some(b"xmlns") {
                candidate = prefix.clone();
            }
            if let Some(candidate) = candidate {
                qualified = [&candidate[..], b":", local].concat();
                if let Some(d) = local_default.filter(|d| *d != XML_NS) {
                    inherited = local_default_ns(d);
                }
                self.out.extend_from_slice(&qualified);
            } else if let Some(p) = prefix.take() {
                let p = if local_prefixes.iter().any(|(k, _)| *k == p) {
                    self.generate_prefix(&mut map, ns)
                } else {
                    map.add(ns, &p);
                    p
                };
                qualified = [&p[..], b":", local].concat();
                self.out.extend_from_slice(&qualified);
                self.out.extend_from_slice(b" xmlns:");
                self.out.extend_from_slice(&p);
                self.out.extend_from_slice(b"=\"");
                xml_attr_escape(&mut self.out, ns.unwrap_or_default());
                self.out.push(b'"');
                if let Some(d) = local_default {
                    inherited = local_default_ns(d);
                }
            } else if local_default.is_none_or(|d| Some(d) != ns) {
                ignore_default_attr = true;
                qualified = local.to_vec();
                inherited = ns.map(<[u8]>::to_vec);
                self.out.extend_from_slice(&qualified);
                self.out.extend_from_slice(b" xmlns=\"");
                xml_attr_escape(&mut self.out, ns.unwrap_or_default());
                self.out.push(b'"');
            } else {
                qualified = local.to_vec();
                inherited = ns.map(<[u8]>::to_vec);
                self.out.extend_from_slice(&qualified);
            }
        }
        for attr in &attrs {
            if attr.ns.is_none() && attr.prefix.is_none() && attr.local == b"xmlns" {
                continue;
            }
            let mut candidate: Option<Vec<u8>> = None;
            if let Some(attr_ns) = attr.ns {
                candidate = map.preferred(Some(attr_ns), attr.prefix);
                if attr_ns == XMLNS_NS {
                    let defined = Some(attr.value).filter(|v| !v.is_empty());
                    let dropped_duplicate = attr.prefix.is_some()
                        && local_prefixes
                            .iter()
                            .any(|(k, v)| k == attr.local && v.as_deref() != defined);
                    if attr.value == XML_NS
                        || (attr.prefix.is_none() && ignore_default_attr)
                        || dropped_duplicate
                    {
                        continue;
                    }
                    if attr.prefix == Some(b"xmlns") {
                        candidate = Some(b"xmlns".to_vec());
                    }
                } else if candidate.is_none() {
                    let generated = self.generate_prefix(&mut map, Some(attr_ns));
                    self.out.extend_from_slice(b" xmlns:");
                    self.out.extend_from_slice(&generated);
                    self.out.extend_from_slice(b"=\"");
                    xml_attr_escape(&mut self.out, attr_ns);
                    self.out.push(b'"');
                    candidate = Some(generated);
                }
            }
            self.out.push(b' ');
            if let Some(candidate) = candidate {
                self.out.extend_from_slice(&candidate);
                self.out.push(b':');
            }
            self.out.extend_from_slice(attr.local);
            self.out.extend_from_slice(b"=\"");
            xml_attr_escape(&mut self.out, attr.value);
            self.out.push(b'"');
        }
        let html = ns == Some(HTML_NAMESPACE);
        let content = if html && local == b"template" {
            node.tpl_content().unwrap_or(node)
        } else {
            node
        };
        let empty = content.first_child().is_none();
        if empty && html && is_void(local) {
            self.out.extend_from_slice(b" />");
            return;
        }
        if empty && !html {
            self.out.extend_from_slice(b"/>");
            return;
        }
        self.out.push(b'>');
        for child in children(content) {
            if !is_embedded_doc(child) {
                self.node(child, inherited.as_deref(), &map, depth + 1);
            }
        }
        self.out.extend_from_slice(b"</");
        self.out.extend_from_slice(&qualified);
        self.out.push(b'>');
    }
}

pub fn xml_serialize(node: Option<Node>) -> Vec<u8> {
    let Some(node) = node else {
        return Vec::new();
    };
    let mut map = PrefixMap::default();
    map.add(Some(XML_NS), b"xml");
    let mut writer = XmlWriter {
        out: Vec::new(),
        index: 1,
    };
    if node.kind() == Kind::Document && node.flags() & XML_FLAG_FRAGMENT != 0 {
        for child in children(node) {
            writer.node(child, None, &map, 0);
        }
    } else {
        writer.node(node, None, &map, 0);
    }
    writer.out
}

pub fn xml_outer_html(node: Option<Node>) -> Vec<u8> {
    xml_serialize(node)
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
