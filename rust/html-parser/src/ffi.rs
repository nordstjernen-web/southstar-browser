//! Southstar — the C ABI of the HTML parser: a rust/html5 tree sink that writes ns_nodes directly, with their strings in one arena per document.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};
use southstar_html5::{
    AttrNs, Attrs, Context, ElementName, LocalName, Ns, Position, QuirksMode, TreeSink,
};

const NODE_SVG_NS: u32 = 1 << 7;
const NODE_FOREIGN_NS: u32 = 1 << 9;
const NODE_PI: u32 = 1 << 11;
const NODE_QUIRKS: u32 = 1 << 5;
const NODE_LIMITED_QUIRKS: u32 = 1 << 6;
const NODE_SCRIPTING_DISABLED: u32 = 1 << 15;
const CHUNK: usize = 64 * 1024;

const SVG_URI: &CStr = c"http://www.w3.org/2000/svg";
const MATHML_URI: &CStr = c"http://www.w3.org/1998/Math/MathML";

unsafe extern "C" {
    fn ns_node_new_document() -> *mut NsNode;
    fn ns_node_new_element(name: *mut c_char) -> *mut NsNode;
    fn ns_node_new_text(text: *mut c_char) -> *mut NsNode;
    fn ns_node_new_comment(text: *mut c_char) -> *mut NsNode;
    fn ns_node_set_name_borrow(node: *mut NsNode, name: *const c_char);
    fn ns_node_set_text_borrow(node: *mut NsNode, text: *const c_char);
    fn ns_node_mark_doctype(node: *mut NsNode);
    fn ns_node_free(node: *mut NsNode);
    fn ns_node_attach_backing(
        root: *mut NsNode,
        backing: *mut c_void,
        destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    );
    fn ns_node_find_first_element(root: *const NsNode, tag: *const c_char) -> *mut NsNode;
    fn ns_node_find_by_id(root: *const NsNode, id: *const c_char) -> *mut NsNode;
    fn ns_template_content_get(template: *mut NsNode) -> *mut NsNode;
    fn ns_element_set_attr(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_element_set_attr_ns(
        el: *mut NsNode,
        namespace_uri: *const c_char,
        prefix: *const c_char,
        local_name: *const c_char,
        qualified_name: *const c_char,
        value: *const c_char,
    );
    fn ns_element_append_attr_borrow(el: *mut NsNode, name: *const c_char, value: *const c_char);
    fn ns_element_get_attr(el: *const NsNode, name: *const c_char) -> *const c_char;
    fn ns_attr_name_is_internal(name: *const c_char) -> GBoolean;
}

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
        unsafe { super::ns_node_set_name_borrow(node.as_mut_ptr(), name.as_ptr()) };
    }

    pub fn detach(node: Node) {
        node.detach();
    }

    pub fn append_child(parent: Node, child: Node) {
        parent.append(child);
    }

    pub fn free(node: Node) {
        unsafe { super::ns_node_free(node.as_mut_ptr()) };
    }

    pub fn find_first_element<'a>(root: Node<'a>, tag: &CStr) -> Option<Node<'a>> {
        unsafe {
            Node::from_ptr(super::ns_node_find_first_element(
                root.as_ptr(),
                tag.as_ptr(),
            ))
        }
    }

    pub fn find_by_id<'a>(root: Node<'a>, id: &CStr) -> Option<Node<'a>> {
        unsafe { Node::from_ptr(super::ns_node_find_by_id(root.as_ptr(), id.as_ptr())) }
    }
}

struct Arena {
    chunks: Vec<Vec<u8>>,
    next_chunk: usize,
}

impl Arena {
    fn new(input_len: usize) -> Self {
        Arena {
            chunks: Vec::new(),
            next_chunk: input_len.saturating_add(64).clamp(64, 16 * CHUNK),
        }
    }

    fn reserve(&mut self, need: usize) -> &mut Vec<u8> {
        let fits = self
            .chunks
            .last()
            .is_some_and(|c| c.capacity() - c.len() >= need);
        if !fits {
            self.chunks
                .push(Vec::with_capacity(need.max(self.next_chunk)));
            self.next_chunk = CHUNK;
        }
        let last = self.chunks.len() - 1;
        &mut self.chunks[last]
    }

    fn alloc(&mut self, bytes: &[u8]) -> *mut c_char {
        let chunk = self.reserve(bytes.len() + 1);
        let start = chunk.len();
        chunk.extend_from_slice(bytes);
        chunk.push(0);
        chunk[start..].as_mut_ptr().cast()
    }

    fn extend_in_place(&mut self, text: *mut c_char, old_len: usize, more: &[u8]) -> bool {
        let Some(chunk) = self.chunks.last_mut() else {
            return false;
        };
        if chunk.len() <= old_len
            || chunk.capacity() - chunk.len() < more.len()
            || !ptr::eq(chunk[chunk.len() - old_len - 1..].as_ptr(), text.cast())
        {
            return false;
        }
        chunk.pop();
        chunk.extend_from_slice(more);
        chunk.push(0);
        true
    }
}

unsafe extern "C" fn destroy_arena(arena: *mut c_void) {
    if !arena.is_null() {
        drop(unsafe { Box::from_raw(arena.cast::<Arena>()) });
    }
}

struct NsSink {
    doc: *mut NsNode,
    arena: Box<Arena>,
    positions: bool,
}

fn node<'a>(n: *mut NsNode) -> Node<'a> {
    unsafe { Node::from_ptr(n) }.unwrap_or_else(|| std::process::abort())
}

impl NsSink {
    fn new(doc: *mut NsNode, input_len: usize, positions: bool) -> Self {
        NsSink {
            doc,
            arena: Box::new(Arena::new(input_len)),
            positions,
        }
    }

    fn finish(self) {
        let arena = Box::into_raw(self.arena);
        unsafe { ns_node_attach_backing(self.doc, arena.cast(), Some(destroy_arena)) };
    }

    fn text_node(&mut self, text: &[u8]) -> *mut NsNode {
        let out = unsafe { ns_node_new_text(ptr::null_mut()) };
        let data = self.arena.alloc(text);
        unsafe { ns_node_set_text_borrow(out, data) };
        out
    }

    fn extend_text(&mut self, text_node: Node, more: &[u8]) {
        let text = text_node.text_ptr();
        let data = if self
            .arena
            .extend_in_place(text, text_node.text_len() as usize, more)
        {
            text
        } else {
            let joined = [text_node.text_with_len().unwrap_or(b""), more].concat();
            self.arena.alloc(&joined)
        };
        unsafe { ns_node_set_text_borrow(text_node.as_mut_ptr(), data) };
    }

    fn add_attribute(&mut self, el: *mut NsNode, attr: southstar_html5::Attribute<'_>) {
        let name = self.arena.alloc(attr.name);
        if unsafe { ns_attr_name_is_internal(name) } != 0 {
            return;
        }
        let value = self.arena.alloc(attr.value);
        let uri = match attr.ns {
            AttrNs::None => {
                unsafe { ns_element_append_attr_borrow(el, name, value) };
                return;
            }
            AttrNs::XLink => c"http://www.w3.org/1999/xlink",
            AttrNs::Xml => c"http://www.w3.org/XML/1998/namespace",
            AttrNs::Xmlns => c"http://www.w3.org/2000/xmlns/",
        };
        let prefix = attr
            .prefix()
            .map_or(ptr::null_mut(), |p| self.arena.alloc(p));
        let local = self.arena.alloc(attr.local_name());
        unsafe { ns_element_set_attr_ns(el, uri.as_ptr(), prefix, local, name, value) };
    }
}

impl TreeSink for NsSink {
    type Handle = *mut NsNode;

    fn document(&self) -> *mut NsNode {
        self.doc
    }

    fn create_element(
        &mut self,
        name: ElementName<'_>,
        attrs: Attrs<'_>,
        position: Position,
    ) -> *mut NsNode {
        let out = unsafe { ns_node_new_element(ptr::null_mut()) };
        let local = match name.local {
            LocalName::Static(s) => s.as_ptr(),
            LocalName::Dynamic(bytes) => self.arena.alloc(bytes).cast_const(),
        };
        unsafe { ns_node_set_name_borrow(out, local) };
        for attr in attrs {
            self.add_attribute(out, attr);
        }
        let element = node(out);
        match name.ns {
            Ns::Svg => {
                element.add_flags(NODE_SVG_NS);
                unsafe { ns_element_set_attr(out, c"data-nd-ns-uri".as_ptr(), SVG_URI.as_ptr()) };
            }
            Ns::MathMl => {
                element.add_flags(NODE_FOREIGN_NS);
                unsafe {
                    ns_element_set_attr(out, c"data-nd-ns-uri".as_ptr(), MATHML_URI.as_ptr())
                };
            }
            Ns::Html => {}
        }
        if self.positions && name.local.bytes().eq_ignore_ascii_case(b"script") {
            element.set_source_position(position.line as c_int, position.column as c_int);
        }
        out
    }

    fn create_comment(&mut self, text: &[u8]) -> *mut NsNode {
        let out = unsafe { ns_node_new_comment(ptr::null_mut()) };
        let data = self.arena.alloc(text);
        unsafe { ns_node_set_text_borrow(out, data) };
        out
    }

    fn create_processing_instruction(&mut self, target: &[u8], data: &[u8]) -> *mut NsNode {
        let out = self.create_comment(data);
        let target = self.arena.alloc(target);
        unsafe { ns_node_set_name_borrow(out, target) };
        node(out).add_flags(NODE_PI);
        out
    }

    fn append_doctype(&mut self, name: &[u8], public_id: &[u8], system_id: &[u8]) {
        let out = unsafe { ns_node_new_element(ptr::null_mut()) };
        let name = if name.is_empty() {
            c"html".as_ptr()
        } else {
            self.arena.alloc(name).cast_const()
        };
        let public_id = self.arena.alloc(public_id);
        let system_id = self.arena.alloc(system_id);
        unsafe {
            ns_node_set_name_borrow(out, name);
            ns_element_set_attr(out, c"publicId".as_ptr(), public_id);
            ns_element_set_attr(out, c"systemId".as_ptr(), system_id);
            ns_node_mark_doctype(out);
        }
        node(self.doc).append(node(out));
    }

    fn append(&mut self, parent: *mut NsNode, child: *mut NsNode) {
        node(parent).append(node(child));
    }

    fn insert_before(&mut self, sibling: *mut NsNode, child: *mut NsNode) {
        node(sibling).insert_before(node(child));
    }

    fn append_text(&mut self, parent: *mut NsNode, text: &[u8]) {
        let parent = node(parent);
        if let Some(last) = parent.last_child().filter(|n| n.is_text()) {
            self.extend_text(last, text);
            return;
        }
        let out = self.text_node(text);
        parent.append(node(out));
    }

    fn insert_text_before(&mut self, sibling: *mut NsNode, text: &[u8]) {
        let sibling = node(sibling);
        if let Some(prev) = sibling.prev_sibling().filter(|n| n.is_text()) {
            self.extend_text(prev, text);
            return;
        }
        let out = self.text_node(text);
        sibling.insert_before(node(out));
    }

    fn parent(&self, n: *mut NsNode) -> Option<*mut NsNode> {
        node(n).parent().map(Node::as_mut_ptr)
    }

    fn remove(&mut self, n: *mut NsNode) {
        node(n).detach();
    }

    fn reparent_children(&mut self, from: *mut NsNode, to: *mut NsNode) {
        let (from, to) = (node(from), node(to));
        while let Some(child) = from.first_child() {
            to.append(child);
        }
    }

    fn template_contents(&mut self, template: *mut NsNode) -> *mut NsNode {
        unsafe { ns_template_content_get(template) }
    }

    fn add_missing_attributes(&mut self, el: *mut NsNode, attrs: Attrs<'_>) {
        for attr in attrs {
            let name = self.arena.alloc(attr.name);
            if unsafe { ns_attr_name_is_internal(name) } != 0
                || !unsafe { ns_element_get_attr(el, name) }.is_null()
            {
                continue;
            }
            let value = self.arena.alloc(attr.value);
            unsafe { ns_element_set_attr(el, name, value) };
        }
    }

    fn discard(&mut self, n: *mut NsNode) {
        unsafe { ns_node_free(n) };
    }
}

unsafe fn input_bytes<'a>(input: *const c_char, len: isize) -> &'a [u8] {
    if len < 0 {
        unsafe { CStr::from_ptr(input) }.to_bytes()
    } else {
        unsafe { glib::slice(input.cast(), len as usize) }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse_with_scripting(
    input: *const c_char,
    len: isize,
    scripting: GBoolean,
) -> *mut NsNode {
    if input.is_null() {
        return ptr::null_mut();
    }
    let bytes = unsafe { input_bytes(input, len) };
    let root = unsafe { ns_node_new_document() };
    let mut sink = NsSink::new(root, bytes.len(), true);
    let quirks = southstar_html5::parse_document(bytes, scripting != 0, &mut sink);
    let root_node = node(root);
    match quirks {
        QuirksMode::Quirks => root_node.add_flags(NODE_QUIRKS),
        QuirksMode::LimitedQuirks => root_node.add_flags(NODE_LIMITED_QUIRKS),
        QuirksMode::NoQuirks => {}
    }
    if scripting == 0 {
        root_node.add_flags(NODE_SCRIPTING_DISABLED);
    }
    crate::prune_html_interelement_whitespace(root_node);
    crate::convert_declarative_shadow(Some(root_node), 0);
    crate::extract_standard_media(root_node);
    sink.finish();
    root
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse(input: *const c_char, len: isize) -> *mut NsNode {
    unsafe { ns_html_parse_with_scripting(input, len, glib::TRUE) }
}

unsafe fn parse_fragment_in_namespace(
    context_tag: Option<&CStr>,
    ns: Ns,
    input: *const c_char,
    len: isize,
    scripting: GBoolean,
) -> *mut NsNode {
    if input.is_null() {
        return ptr::null_mut();
    }
    let bytes = unsafe { input_bytes(input, len) };
    let name = context_tag
        .map(CStr::to_bytes)
        .filter(|n| !n.is_empty())
        .unwrap_or(b"body");
    let out = unsafe { ns_node_new_document() };
    if scripting == 0 {
        node(out).add_flags(NODE_SCRIPTING_DISABLED);
    }
    let mut sink = NsSink::new(out, bytes.len(), false);
    southstar_html5::parse_fragment(Context { ns, name }, bytes, scripting != 0, &mut sink);
    sink.finish();
    out
}

fn c_str<'a>(p: *const c_char) -> Option<&'a CStr> {
    (!p.is_null()).then(|| unsafe { CStr::from_ptr(p) })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse_fragment_with_scripting(
    context_tag: *const c_char,
    input: *const c_char,
    len: isize,
    scripting: GBoolean,
) -> *mut NsNode {
    unsafe { parse_fragment_in_namespace(c_str(context_tag), Ns::Html, input, len, scripting) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse_fragment_in(
    context_tag: *const c_char,
    input: *const c_char,
    len: isize,
) -> *mut NsNode {
    unsafe { ns_html_parse_fragment_with_scripting(context_tag, input, len, glib::TRUE) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse_fragment_in_context(
    context: *const NsNode,
    input: *const c_char,
    len: isize,
    scripting: GBoolean,
) -> *mut NsNode {
    let context = unsafe { Node::from_ptr(context) };
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
    unsafe { parse_fragment_in_namespace(tag, ns, input, len, scripting) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_convert_declarative_shadow(root: *mut NsNode) {
    crate::convert_declarative_shadow(unsafe { Node::from_ptr(root) }, 0);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_xml_well_formed(
    input: *const c_char,
    len: isize,
    out_root_ns: *mut *mut c_char,
) -> GBoolean {
    if !out_root_ns.is_null() {
        unsafe { *out_root_ns = ptr::null_mut() };
    }
    if input.is_null() {
        return glib::FALSE;
    }
    let (ok, root_ns) = crate::xml_well_formed(unsafe { input_bytes(input, len) });
    if ok
        && !out_root_ns.is_null()
        && let Some(ns) = root_ns
    {
        unsafe { *out_root_ns = glib::strdup(&ns) };
    }
    glib::boolean(ok)
}
