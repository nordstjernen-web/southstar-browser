//! Southstar — the HTML parser's tree sink, which builds DOM nodes straight from rust/html5, and the parse entry points that hand back an owned Tree.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::collections::{HashMap, HashSet};
use std::ffi::CString;

use southstar_dom::attrs::{self, NsName};
use southstar_dom::{Kind, MAX_DEPTH, Node, Tree, ancestors, children, node, select};
use southstar_html5::{AttrNs, Attrs, Context, ElementName, Ns, Position, QuirksMode, TreeSink};

const NODE_SVG_NS: u32 = 1 << 7;
const NODE_FOREIGN_NS: u32 = 1 << 9;
const NODE_PI: u32 = 1 << 11;
const NODE_QUIRKS: u32 = 1 << 5;
const NODE_LIMITED_QUIRKS: u32 = 1 << 6;
const NODE_SCRIPTING_DISABLED: u32 = 1 << 15;

const SVG_URI: &CStr = c"http://www.w3.org/2000/svg";
const MATHML_URI: &CStr = c"http://www.w3.org/1998/Math/MathML";

pub mod dom {
    use core::ffi::CStr;

    use southstar_dom::Node;

    pub fn set_attr(node: Node, name: &CStr, value: &CStr) {
        southstar_dom::attrs::set_attribute(node, name, Some(value.to_bytes()));
    }

    pub fn remove_attr(node: Node, name: &CStr) {
        southstar_dom::attrs::remove_attribute(node, name);
    }

    pub fn rename_static(node: Node, name: &'static CStr) {
        node.set_name(name.to_bytes());
    }

    pub fn find_first_element<'a>(root: Node<'a>, tag: &CStr) -> Option<Node<'a>> {
        southstar_dom::index::find_first_element(root, tag)
    }

    pub fn find_by_id<'a>(root: Node<'a>, id: &CStr) -> Option<Node<'a>> {
        southstar_dom::index::find_by_id(root, id)
    }
}

type Handle = Node<'static>;

const LEFT_OUT_OF_SELECTEDCONTENT: [&[u8]; 5] =
    [b"script", b"frame", b"iframe", b"embed", b"object"];

struct SelectState {
    selectedcontent: Handle,
    chosen: Option<Handle>,
}

struct NsSink {
    doc: Handle,
    positions: bool,
    template_hosts: HashMap<Handle, Handle>,
    merge: Option<(Handle, HashSet<Vec<u8>>)>,
    fresh_selectedcontent: Option<Handle>,
    selects: HashMap<Handle, SelectState>,
    replaced: Vec<Handle>,
    declarative_shadow: bool,
    media_candidate: bool,
}

fn left_out_of_selectedcontent(node: Handle) -> bool {
    node.element_name()
        .is_some_and(|name| LEFT_OUT_OF_SELECTEDCONTENT.contains(&name))
}

fn prune_selectedcontent_copy(node: Handle) {
    let mut next = node.first_child();
    while let Some(child) = next {
        next = child.next_sibling();
        if left_out_of_selectedcontent(child) {
            child.free_tree();
        } else {
            prune_selectedcontent_copy(child);
        }
    }
    if let Some(content) = node.tpl_content() {
        prune_selectedcontent_copy(content);
    }
}

fn copy_option(option: Handle, selectedcontent: Handle) {
    for child in children(option).filter(|c| !left_out_of_selectedcontent(*c)) {
        if let Some(copy) = node::clone(child, true, 0) {
            prune_selectedcontent_copy(copy.root());
            selectedcontent.append(copy.into_root());
        }
    }
}

impl NsSink {
    fn new(doc: Handle, positions: bool) -> Self {
        NsSink {
            doc,
            positions,
            template_hosts: HashMap::new(),
            merge: None,
            fresh_selectedcontent: None,
            selects: HashMap::new(),
            replaced: Vec::new(),
            declarative_shadow: false,
            media_candidate: false,
        }
    }

    fn note_attribute(&mut self, name: &[u8], value: &[u8]) {
        match name {
            b"shadowrootmode" | b"shadowroot" => self.declarative_shadow = true,
            b"id" if value == b"player" => self.media_candidate = true,
            b"class" if value.windows(6).any(|w| w == b"player") => {
                self.media_candidate = true;
            }
            _ => {}
        }
    }

    fn inserted(&mut self, child: Handle) {
        if self.fresh_selectedcontent == Some(child) {
            self.fresh_selectedcontent = None;
            self.selectedcontent_inserted(child);
        }
    }

    fn selectedcontent_select(&self, selectedcontent: Handle) -> Option<Handle> {
        let mut found = None;
        for ancestor in ancestors(selectedcontent).take(MAX_DEPTH as usize) {
            match ancestor.element_name() {
                Some(b"select") => {
                    if found.is_some() || self.selects.contains_key(&ancestor) {
                        return None;
                    }
                    found = Some(ancestor);
                }
                Some(b"option" | b"selectedcontent") => return None,
                _ => {}
            }
        }
        found
    }

    fn selectedcontent_inserted(&mut self, selectedcontent: Handle) {
        let Some(select) = self.selectedcontent_select(selectedcontent) else {
            return;
        };
        if select.attr(c"multiple").is_some() {
            return;
        }
        let chosen = select::chosen_option(select);
        self.selects.insert(
            select,
            SelectState {
                selectedcontent,
                chosen,
            },
        );
        if let Some(option) = chosen {
            copy_option(option, selectedcontent);
        }
    }

    fn free_replaced(&mut self) {
        let orphans: HashSet<Handle> = self
            .replaced
            .drain(..)
            .filter(|n| n.parent().is_none())
            .collect();
        for orphan in orphans {
            orphan.free_tree();
        }
    }

    fn tree_parent(&self, node: Handle) -> Option<Handle> {
        node.parent()
            .or_else(|| self.template_hosts.get(&node).copied())
    }

    fn is_inclusive_ancestor(&self, ancestor: Handle, node: Handle) -> bool {
        let mut cur = Some(node);
        while let Some(n) = cur {
            if n == ancestor {
                return true;
            }
            cur = self.tree_parent(n);
        }
        false
    }

    fn can_adopt(&self, parent: Handle, child: Handle) -> bool {
        if child.first_child().is_none() && child.tpl_content().is_none() {
            return parent != child;
        }
        !self.is_inclusive_ancestor(child, parent)
    }

    fn merge_names(&mut self, el: Handle) -> &mut HashSet<Vec<u8>> {
        if self
            .merge
            .as_ref()
            .is_some_and(|(element, _)| *element != el)
        {
            self.merge = None;
        }
        &mut self
            .merge
            .get_or_insert_with(|| (el, attribute_names(el)))
            .1
    }

    fn text_node(text: &[u8]) -> Handle {
        node::new_text(Kind::Text, text)
    }

    fn add_attribute(el: Handle, attr: southstar_html5::Attribute<'_>) {
        if attrs::is_internal(attr.name) {
            return;
        }
        let uri = match attr.ns {
            AttrNs::None => {
                attrs::append(el, attr.name, attr.value);
                return;
            }
            AttrNs::XLink => c"http://www.w3.org/1999/xlink",
            AttrNs::Xml => c"http://www.w3.org/XML/1998/namespace",
            AttrNs::Xmlns => c"http://www.w3.org/2000/xmlns/",
        };
        let prefix = attr.prefix().map(CString::new);
        let local = CString::new(attr.local_name());
        let name = CString::new(attr.name);
        let value = CString::new(attr.value);
        let (Ok(local), Ok(name), Ok(value)) = (local, name, value) else {
            return;
        };
        let prefix = prefix.and_then(Result::ok);
        let qname = NsName {
            namespace_uri: Some(uri),
            prefix: prefix.as_deref(),
            local_name: &local,
            name: Some(&name),
        };
        attrs::set_ns(el, qname, Some(&value));
    }
}

fn attribute_names(el: Handle) -> HashSet<Vec<u8>> {
    el.attrs()
        .filter_map(|a| a.name())
        .map(|n| n.to_bytes().to_ascii_lowercase())
        .collect()
}

impl TreeSink for NsSink {
    type Handle = Handle;

    fn document(&self) -> Handle {
        self.doc
    }

    fn create_element(
        &mut self,
        name: ElementName<'_>,
        attrs: Attrs<'_>,
        position: Position,
    ) -> Handle {
        let element = node::new_element(name.local.bytes());
        if name.local.bytes() == b"video" {
            self.media_candidate = true;
        }
        for attr in attrs {
            self.note_attribute(attr.name, attr.value);
            Self::add_attribute(element, attr);
        }
        match name.ns {
            Ns::Svg => {
                element.add_flags(NODE_SVG_NS);
                dom::set_attr(element, c"data-nd-ns-uri", SVG_URI);
            }
            Ns::MathMl => {
                element.add_flags(NODE_FOREIGN_NS);
                dom::set_attr(element, c"data-nd-ns-uri", MATHML_URI);
            }
            Ns::Html => {}
        }
        if self.positions && name.local.bytes().eq_ignore_ascii_case(b"script") {
            element.set_source_position(position.line as c_int, position.column as c_int);
        }
        if name.ns == Ns::Html && name.local.bytes() == b"selectedcontent" {
            self.fresh_selectedcontent = Some(element);
        }
        element
    }

    fn create_comment(&mut self, text: &[u8]) -> Handle {
        node::new_text(Kind::Comment, text)
    }

    fn create_processing_instruction(&mut self, target: &[u8], data: &[u8]) -> Handle {
        let out = self.create_comment(data);
        out.set_name(target);
        out.add_flags(NODE_PI);
        out
    }

    fn append_doctype(&mut self, name: &[u8], public_id: &[u8], system_id: &[u8]) {
        let out = node::new_element(if name.is_empty() { b"html" } else { name });
        attrs::set_attribute(out, c"publicId", Some(public_id));
        attrs::set_attribute(out, c"systemId", Some(system_id));
        out.set_kind(Kind::Doctype);
        self.doc.append(out);
    }

    fn append(&mut self, parent: Handle, child: Handle) {
        if self.can_adopt(parent, child) {
            parent.append(child);
            self.inserted(child);
        }
    }

    fn insert_before(&mut self, sibling: Handle, child: Handle) {
        if sibling
            .parent()
            .is_none_or(|parent| self.can_adopt(parent, child))
        {
            sibling.insert_before(child);
            self.inserted(child);
        }
    }

    fn append_text(&mut self, parent: Handle, text: &[u8]) {
        match parent.last_child().filter(|n| n.is_text()) {
            Some(last) => last.append_text(text),
            None => parent.append(Self::text_node(text)),
        }
    }

    fn insert_text_before(&mut self, sibling: Handle, text: &[u8]) {
        match sibling.prev_sibling().filter(|n| n.is_text()) {
            Some(prev) => prev.append_text(text),
            None => sibling.insert_before(Self::text_node(text)),
        }
    }

    fn parent(&self, n: Handle) -> Option<Handle> {
        n.parent()
    }

    fn remove(&mut self, n: Handle) {
        n.detach();
    }

    fn reparent_children(&mut self, from: Handle, to: Handle) {
        if self.is_inclusive_ancestor(from, to) {
            return;
        }
        while let Some(child) = from.first_child() {
            to.append(child);
        }
    }

    fn template_contents(&mut self, template: Handle) -> Handle {
        let fresh = template.tpl_content().is_none();
        let content = node::template_content(template);
        if fresh {
            self.template_hosts.insert(content, template);
        }
        content
    }

    fn add_missing_attributes(&mut self, el: Handle, attrs: Attrs<'_>) {
        for attr in attrs {
            self.note_attribute(attr.name, attr.value);
            if attr.name.contains(&0)
                || attrs::is_internal(attr.name)
                || !self.merge_names(el).insert(attr.name.to_ascii_lowercase())
            {
                continue;
            }
            attrs::append(el, attr.name, attr.value);
        }
    }

    fn option_children_parsed(&mut self, option: Handle) {
        let Some(select) = select::option_select(option) else {
            return;
        };
        let Some(state) = self.selects.get_mut(&select) else {
            return;
        };
        let takes_choice = option.attr(c"selected").is_some()
            || (state.chosen.is_none()
                && select::picks_first_option(select)
                && select::option_selectable(option));
        if takes_choice {
            state.chosen = Some(option);
        }
        if state.chosen != Some(option) {
            return;
        }
        let selectedcontent = state.selectedcontent;
        while let Some(child) = selectedcontent.first_child() {
            child.detach();
            self.replaced.push(child);
        }
        copy_option(option, selectedcontent);
    }

    fn discard(&mut self, n: Handle) {
        self.merge = None;
        self.template_hosts.clear();
        self.fresh_selectedcontent = None;
        self.selects.clear();
        self.free_replaced();
        n.free_tree();
    }
}

pub fn parse_document(bytes: &[u8], scripting: bool) -> Tree {
    let tree = Tree::new(Kind::Document);
    let root = tree.root();
    let mut sink = NsSink::new(root, true);
    match southstar_html5::parse_document(bytes, scripting, &mut sink) {
        QuirksMode::Quirks => root.add_flags(NODE_QUIRKS),
        QuirksMode::LimitedQuirks => root.add_flags(NODE_LIMITED_QUIRKS),
        QuirksMode::NoQuirks => {}
    }
    sink.free_replaced();
    if !scripting {
        root.add_flags(NODE_SCRIPTING_DISABLED);
    }
    crate::prune_html_interelement_whitespace(root);
    if sink.declarative_shadow {
        crate::convert_declarative_shadow(Some(root), 0);
    }
    if sink.media_candidate {
        crate::extract_standard_media(root);
    }
    tree
}

pub fn parse_fragment_for_tag(tag: &CStr, bytes: &[u8], scripting: bool) -> Tree {
    parse_fragment_in_namespace(Some(tag), Ns::Html, bytes, scripting)
}

pub fn parse_fragment(context: Option<Node>, bytes: &[u8], scripting: bool) -> Tree {
    let tag = context.filter(|c| c.is_element()).and_then(|c| c.name());
    let ns = match (tag, context) {
        (Some(_), Some(c)) if c.flags() & NODE_SVG_NS != 0 => Ns::Svg,
        (Some(_), Some(c))
            if c.flags() & NODE_FOREIGN_NS != 0
                && c.attr(c"data-nd-ns-uri").is_some_and(|u| u == MATHML_URI) =>
        {
            Ns::MathMl
        }
        _ => Ns::Html,
    };
    parse_fragment_in_namespace(tag, ns, bytes, scripting)
}

pub(crate) fn parse_fragment_in_namespace(
    context_tag: Option<&CStr>,
    ns: Ns,
    bytes: &[u8],
    scripting: bool,
) -> Tree {
    let qualified = context_tag
        .map(CStr::to_bytes)
        .filter(|n| !n.is_empty())
        .unwrap_or(b"body");
    let name = match (ns, qualified.iter().rposition(|&c| c == b':')) {
        (Ns::Svg | Ns::MathMl, Some(colon)) => &qualified[colon + 1..],
        _ => qualified,
    };
    let tree = Tree::new(Kind::Document);
    if !scripting {
        tree.root().add_flags(NODE_SCRIPTING_DISABLED);
    }
    let mut sink = NsSink::new(tree.root(), false);
    southstar_html5::parse_fragment(Context { ns, name }, bytes, scripting, &mut sink);
    sink.free_replaced();
    tree
}
