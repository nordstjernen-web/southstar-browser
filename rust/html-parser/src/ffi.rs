//! Southstar — the C ABI of the HTML parser, as declared in src/html.h, over lexbor's document, parser and node structs and the DOM functions of src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint, c_void};
use core::ptr;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

const NODE_SVG_NS: u32 = 1 << 7;
const NODE_FOREIGN_NS: u32 = 1 << 9;
const NODE_PI: u32 = 1 << 11;
const NODE_QUIRKS: u32 = 1 << 5;
const NODE_LIMITED_QUIRKS: u32 = 1 << 6;
const NODE_SCRIPTING_DISABLED: u32 = 1 << 15;

const LXB_NS_HTML: usize = 0x02;
const LXB_NS_MATH: usize = 0x03;
const LXB_NS_SVG: usize = 0x04;
const LXB_NS_XLINK: usize = 0x05;
const LXB_NS_XML: usize = 0x06;
const LXB_NS_XMLNS: usize = 0x07;
const LXB_TAG_BODY: usize = 0x0020;
const LXB_TAG_TEMPLATE: usize = 0x00b6;
const LXB_TYPE_ELEMENT: c_uint = 0x01;
const LXB_TYPE_TEXT: c_uint = 0x03;
const LXB_TYPE_CDATA_SECTION: c_uint = 0x04;
const LXB_TYPE_PROCESSING_INSTRUCTION: c_uint = 0x07;
const LXB_TYPE_COMMENT: c_uint = 0x08;
const LXB_TYPE_DOCUMENT: c_uint = 0x09;
const LXB_TYPE_DOCUMENT_TYPE: c_uint = 0x0A;
const LXB_TYPE_DOCUMENT_FRAGMENT: c_uint = 0x0B;
const LXB_CMODE_QUIRKS: c_uint = 0x01;
const LXB_CMODE_LIMITED_QUIRKS: c_uint = 0x02;
const LXB_OPT_WO_EVENTS: u32 = 1;
const LXB_STATUS_OK: c_uint = 0;
const LEXBOR_HASH_SHORT_SIZE: usize = 16;

const SVG_URI: &CStr = c"http://www.w3.org/2000/svg";
const MATHML_URI: &CStr = c"http://www.w3.org/1998/Math/MathML";

#[repr(C)]
struct LxbNode {
    events: *mut c_void,
    local_name: usize,
    prefix: usize,
    ns: usize,
    owner_document: *mut c_void,
    next: *mut LxbNode,
    prev: *mut LxbNode,
    parent: *mut LxbNode,
    first_child: *mut LxbNode,
    last_child: *mut LxbNode,
    user: *mut c_void,
    kind: c_uint,
}

#[repr(C)]
struct LexborStr {
    data: *mut c_char,
    length: usize,
}

#[repr(C)]
struct LxbCharacterData {
    node: LxbNode,
    data: LexborStr,
}

#[repr(C)]
struct LxbProcessingInstruction {
    char_data: LxbCharacterData,
    target: LexborStr,
}

#[repr(C)]
struct LxbElement {
    node: LxbNode,
    upper_name: usize,
    qualified_name: usize,
    is_value: *mut LexborStr,
    first_attr: *mut c_void,
    last_attr: *mut c_void,
    attr_id: *mut c_void,
    attr_class: *mut c_void,
    style: *mut c_void,
    list: *mut c_void,
    condition: c_uint,
    custom_state: c_uint,
}

#[repr(C)]
struct LxbDocumentFragment {
    node: LxbNode,
    host: *mut LxbElement,
}

#[repr(C)]
struct LxbTemplateElement {
    element: LxbElement,
    content: *mut LxbDocumentFragment,
}

#[repr(C)]
struct LxbDocument {
    node: LxbNode,
    compat_mode: c_uint,
    kind: c_uint,
    doctype: *mut c_void,
    element: *mut LxbElement,
    create_interface: *mut c_void,
    clone_interface: *mut c_void,
    destroy_interface: *mut c_void,
    mutation: *const c_void,
    attr_mutation: *const c_void,
    mraw: *mut c_void,
    text: *mut c_void,
    tags: *mut c_void,
    attrs: *mut c_void,
    prefix: *mut c_void,
    ns: *mut c_void,
    parser: *mut c_void,
    user: *mut c_void,
    css: *mut c_void,
    options: u32,
    tags_inherited: bool,
    ns_inherited: bool,
    scripting: bool,
}

#[repr(C)]
union LexborHashEntryKey {
    long_str: *mut u8,
    short_str: [u8; LEXBOR_HASH_SHORT_SIZE + 1],
}

#[repr(C)]
struct LexborHashEntry {
    key: LexborHashEntryKey,
    length: usize,
    next: *mut LexborHashEntry,
}

#[repr(C)]
struct LxbTagData {
    entry: LexborHashEntry,
    tag_id: usize,
    ref_count: usize,
    read_only: bool,
}

unsafe extern "C" {
    fn lxb_html_document_create() -> *mut LxbDocument;
    fn lxb_html_document_destroy(document: *mut LxbDocument) -> *mut LxbDocument;
    fn lxb_html_document_parse(document: *mut LxbDocument, html: *const u8, size: usize) -> c_uint;
    fn lxb_html_document_dom_opt_set_noi(document: *mut LxbDocument, opt: u32);
    fn lxb_html_parser_create() -> *mut c_void;
    fn lxb_html_parser_init(parser: *mut c_void) -> c_uint;
    fn lxb_html_parser_destroy(parser: *mut c_void) -> *mut c_void;
    fn lxb_html_parser_dom_opt_set_noi(parser: *mut c_void, opt: u32);
    fn lxb_html_parser_scripting_set_noi(parser: *mut c_void, scripting: bool);
    fn lxb_html_parse_fragment_by_tag_id(
        parser: *mut c_void,
        document: *mut LxbDocument,
        tag_id: usize,
        ns: usize,
        html: *const u8,
        size: usize,
    ) -> *mut LxbNode;
    fn lxb_tag_data_by_name(hash: *mut c_void, name: *const u8, len: usize) -> *const LxbTagData;
    fn lxb_dom_element_qualified_name(element: *const LxbElement, len: *mut usize)
    -> *const c_char;
    fn lxb_dom_element_first_attribute_noi(element: *mut LxbElement) -> *mut LxbNode;
    fn lxb_dom_element_next_attribute_noi(attr: *mut LxbNode) -> *mut LxbNode;
    fn lxb_dom_attr_qualified_name(attr: *const LxbNode, len: *mut usize) -> *const c_char;
    fn lxb_dom_attr_value_noi(attr: *mut LxbNode, len: *mut usize) -> *const c_char;
    fn lxb_dom_attr_local_name(attr: *const LxbNode, len: *mut usize) -> *const c_char;
    fn lxb_dom_document_type_name_noi(doc_type: *mut LxbNode, len: *mut usize) -> *const c_char;
    fn lxb_dom_document_type_public_id_noi(
        doc_type: *mut LxbNode,
        len: *mut usize,
    ) -> *const c_char;
    fn lxb_dom_document_type_system_id_noi(
        doc_type: *mut LxbNode,
        len: *mut usize,
    ) -> *const c_char;
    fn ns_node_new_document() -> *mut NsNode;
    fn ns_node_new_element(name: *mut c_char) -> *mut NsNode;
    fn ns_node_new_text(text: *mut c_char) -> *mut NsNode;
    fn ns_node_new_comment(text: *mut c_char) -> *mut NsNode;
    fn ns_node_set_name_borrow(node: *mut NsNode, name: *const c_char);
    fn ns_node_set_name_owned(node: *mut NsNode, name: *mut c_char);
    fn ns_node_set_text_borrow(node: *mut NsNode, text: *const c_char);
    fn ns_node_mark_doctype(node: *mut NsNode);
    fn ns_node_append_child(parent: *mut NsNode, child: *mut NsNode);
    fn ns_node_remove(child: *mut NsNode);
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
    fn ns_element_remove_attr(el: *mut NsNode, name: *const c_char);
    fn ns_attr_name_is_internal(name: *const c_char) -> GBoolean;
}

pub mod dom {
    use core::ffi::CStr;

    use southstar_dom::Node;

    pub fn set_attr(node: Node, name: &CStr, value: &CStr) {
        unsafe { super::ns_element_set_attr(node.as_mut_ptr(), name.as_ptr(), value.as_ptr()) };
    }

    pub fn remove_attr(node: Node, name: &CStr) {
        unsafe { super::ns_element_remove_attr(node.as_mut_ptr(), name.as_ptr()) };
    }

    pub fn rename_static(node: Node, name: &'static CStr) {
        unsafe { super::ns_node_set_name_borrow(node.as_mut_ptr(), name.as_ptr()) };
    }

    pub fn detach(node: Node) {
        unsafe { super::ns_node_remove(node.as_mut_ptr()) };
    }

    pub fn append_child(parent: Node, child: Node) {
        unsafe { super::ns_node_append_child(parent.as_mut_ptr(), child.as_mut_ptr()) };
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

fn text_or_empty(p: *const c_char) -> *const c_char {
    if p.is_null() { c"".as_ptr() } else { p }
}

fn non_empty_or(p: *const c_char, len: usize, fallback: &'static CStr) -> *const c_char {
    if p.is_null() || len == 0 {
        fallback.as_ptr()
    } else {
        p
    }
}

unsafe fn borrow_attributes(element: *mut LxbElement, out: *mut NsNode) {
    let mut attr = unsafe { lxb_dom_element_first_attribute_noi(element) };
    while !attr.is_null() {
        unsafe {
            let (mut key_len, mut value_len) = (0usize, 0usize);
            let key = lxb_dom_attr_qualified_name(attr, &mut key_len);
            let value = text_or_empty(lxb_dom_attr_value_noi(attr, &mut value_len));
            if !key.is_null() && key_len > 0 && ns_attr_name_is_internal(key) == 0 {
                let uri = match (*attr).ns {
                    LXB_NS_XLINK => Some(c"http://www.w3.org/1999/xlink"),
                    LXB_NS_XML => Some(c"http://www.w3.org/XML/1998/namespace"),
                    LXB_NS_XMLNS => Some(c"http://www.w3.org/2000/xmlns/"),
                    _ => None,
                };
                match uri {
                    Some(uri) => {
                        let mut local_len = 0usize;
                        let local_name = lxb_dom_attr_local_name(attr, &mut local_len);
                        let qualified = CStr::from_ptr(key).to_bytes();
                        let prefix = qualified
                            .iter()
                            .position(|&b| b == b':')
                            .map(|c| glib::strdup(&qualified[..c]));
                        let local = if !local_name.is_null() && local_len > 0 {
                            glib::strdup(core::slice::from_raw_parts(
                                local_name.cast::<u8>(),
                                local_len,
                            ))
                        } else {
                            glib::g_strdup(key)
                        };
                        ns_element_set_attr_ns(
                            out,
                            uri.as_ptr(),
                            prefix.unwrap_or(ptr::null_mut()),
                            local,
                            key,
                            value,
                        );
                        glib::g_free(prefix.unwrap_or(ptr::null_mut()).cast());
                        glib::g_free(local.cast());
                    }
                    None => ns_element_append_attr_borrow(out, key, value),
                }
            }
            attr = lxb_dom_element_next_attribute_noi(attr);
        }
    }
}

unsafe fn convert_node(src: *mut LxbNode) -> *mut NsNode {
    unsafe {
        match (*src).kind {
            LXB_TYPE_DOCUMENT | LXB_TYPE_DOCUMENT_FRAGMENT => ns_node_new_document(),
            LXB_TYPE_ELEMENT => {
                let element = src.cast::<LxbElement>();
                let mut len = 0usize;
                let name = lxb_dom_element_qualified_name(element, &mut len);
                let out = ns_node_new_element(ptr::null_mut());
                ns_node_set_name_borrow(
                    out,
                    if name.is_null() {
                        c"unknown".as_ptr()
                    } else {
                        name
                    },
                );
                borrow_attributes(element, out);
                let node = Node::from_ptr(out);
                if let Some(node) = node {
                    match (*src).ns {
                        LXB_NS_SVG => {
                            node.add_flags(NODE_SVG_NS);
                            ns_element_set_attr(out, c"data-nd-ns-uri".as_ptr(), SVG_URI.as_ptr());
                        }
                        LXB_NS_MATH => {
                            node.add_flags(NODE_FOREIGN_NS);
                            ns_element_set_attr(
                                out,
                                c"data-nd-ns-uri".as_ptr(),
                                MATHML_URI.as_ptr(),
                            );
                        }
                        _ => {}
                    }
                }
                out
            }
            LXB_TYPE_TEXT | LXB_TYPE_CDATA_SECTION => {
                let data = &(*src.cast::<LxbCharacterData>()).data;
                let out = ns_node_new_text(ptr::null_mut());
                ns_node_set_text_borrow(out, text_or_empty(data.data));
                out
            }
            LXB_TYPE_COMMENT => {
                let data = &(*src.cast::<LxbCharacterData>()).data;
                let out = ns_node_new_comment(ptr::null_mut());
                ns_node_set_text_borrow(out, text_or_empty(data.data));
                out
            }
            LXB_TYPE_PROCESSING_INSTRUCTION => {
                let pi = &*src.cast::<LxbProcessingInstruction>();
                let out = ns_node_new_comment(ptr::null_mut());
                ns_node_set_text_borrow(out, text_or_empty(pi.char_data.data.data));
                ns_node_set_name_owned(out, glib::g_strdup(text_or_empty(pi.target.data)));
                if let Some(node) = Node::from_ptr(out) {
                    node.add_flags(NODE_PI);
                }
                out
            }
            LXB_TYPE_DOCUMENT_TYPE => {
                let (mut name_len, mut public_len, mut system_len) = (0usize, 0usize, 0usize);
                let name = lxb_dom_document_type_name_noi(src, &mut name_len);
                let public_id = lxb_dom_document_type_public_id_noi(src, &mut public_len);
                let system_id = lxb_dom_document_type_system_id_noi(src, &mut system_len);
                let out = ns_node_new_element(ptr::null_mut());
                ns_node_set_name_borrow(out, non_empty_or(name, name_len, c"html"));
                ns_element_set_attr(
                    out,
                    c"publicId".as_ptr(),
                    non_empty_or(public_id, public_len, c""),
                );
                ns_element_set_attr(
                    out,
                    c"systemId".as_ptr(),
                    non_empty_or(system_id, system_len, c""),
                );
                ns_node_mark_doctype(out);
                out
            }
            _ => ptr::null_mut(),
        }
    }
}

unsafe fn template_content_first_child(src: *mut LxbNode) -> *mut LxbNode {
    unsafe {
        let node = &*src;
        if node.kind != LXB_TYPE_ELEMENT
            || node.ns != LXB_NS_HTML
            || node.local_name != LXB_TAG_TEMPLATE
        {
            return ptr::null_mut();
        }
        let template = &*src.cast::<LxbTemplateElement>();
        match template.content.as_ref() {
            Some(content) => content.node.first_child,
            None => ptr::null_mut(),
        }
    }
}

unsafe fn walk_into(src_root: *mut LxbNode, ns_root: *mut NsNode) {
    let mut stack: Vec<(*mut LxbNode, *mut NsNode)> = Vec::new();
    let push =
        |stack: &mut Vec<(*mut LxbNode, *mut NsNode)>, child: *mut LxbNode, parent: *mut NsNode| {
            if !child.is_null() && !parent.is_null() {
                stack.push((child, parent));
            }
        };
    unsafe {
        push(&mut stack, (*src_root).first_child, ns_root);
        push(&mut stack, template_content_first_child(src_root), ns_root);
        while let Some((mut src, mut parent)) = stack.pop() {
            while !src.is_null() {
                let next = (*src).next;
                let converted = convert_node(src);
                if !converted.is_null() {
                    ns_node_append_child(parent, converted);
                    let kids = (*src).first_child;
                    let template_kids = template_content_first_child(src);
                    push(&mut stack, next, parent);
                    if !template_kids.is_null() {
                        push(
                            &mut stack,
                            template_kids,
                            ns_template_content_get(converted),
                        );
                    }
                    if !kids.is_null() {
                        src = kids;
                        parent = converted;
                        continue;
                    }
                } else if !next.is_null() {
                    src = next;
                    continue;
                }
                src = ptr::null_mut();
            }
        }
    }
}

unsafe extern "C" fn destroy_document(document: *mut c_void) {
    if !document.is_null() {
        unsafe { lxb_html_document_destroy(document.cast()) };
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
    unsafe {
        let doc = lxb_html_document_create();
        if doc.is_null() {
            return ptr::null_mut();
        }
        lxb_html_document_dom_opt_set_noi(doc, LXB_OPT_WO_EVENTS);
        (*doc).scripting = scripting != 0;
        if lxb_html_document_parse(doc, bytes.as_ptr(), bytes.len()) != LXB_STATUS_OK {
            lxb_html_document_destroy(doc);
            return ptr::null_mut();
        }
        let src_root = doc.cast::<LxbNode>();
        let root = convert_node(src_root);
        let Some(root_node) = Node::from_ptr(root) else {
            lxb_html_document_destroy(doc);
            return ptr::null_mut();
        };
        walk_into(src_root, root);
        match (*doc).compat_mode {
            LXB_CMODE_QUIRKS => root_node.add_flags(NODE_QUIRKS),
            LXB_CMODE_LIMITED_QUIRKS => root_node.add_flags(NODE_LIMITED_QUIRKS),
            _ => {}
        }
        if scripting == 0 {
            root_node.add_flags(NODE_SCRIPTING_DISABLED);
        }
        crate::assign_script_positions(root_node, bytes);
        crate::prune_html_interelement_whitespace(root_node);
        crate::convert_declarative_shadow(Some(root_node), 0);
        crate::extract_standard_media(root_node);
        ns_node_attach_backing(root, doc.cast(), Some(destroy_document));
        root
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_html_parse(input: *const c_char, len: isize) -> *mut NsNode {
    unsafe { ns_html_parse_with_scripting(input, len, glib::TRUE) }
}

unsafe fn tag_id_from_name(doc: *mut LxbDocument, name: Option<&[u8]>) -> usize {
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        return LXB_TAG_BODY;
    };
    let data = unsafe { lxb_tag_data_by_name((*doc).tags, name.as_ptr(), name.len()) };
    unsafe { data.as_ref() }.map_or(LXB_TAG_BODY, |d| d.tag_id)
}

unsafe fn parse_fragment_in_namespace(
    context_tag: Option<&CStr>,
    ns: usize,
    input: *const c_char,
    len: isize,
    scripting: GBoolean,
) -> *mut NsNode {
    if input.is_null() {
        return ptr::null_mut();
    }
    let bytes = unsafe { input_bytes(input, len) };
    unsafe {
        let parser = lxb_html_parser_create();
        if parser.is_null() || lxb_html_parser_init(parser) != LXB_STATUS_OK {
            if !parser.is_null() {
                lxb_html_parser_destroy(parser);
            }
            return ptr::null_mut();
        }
        lxb_html_parser_dom_opt_set_noi(parser, LXB_OPT_WO_EVENTS);
        lxb_html_parser_scripting_set_noi(parser, scripting != 0);
        let doc = lxb_html_document_create();
        if doc.is_null() {
            lxb_html_parser_destroy(parser);
            return ptr::null_mut();
        }
        (*doc).scripting = scripting != 0;
        let lower = context_tag.map(|t| t.to_bytes().to_ascii_lowercase());
        let lower = lower
            .as_deref()
            .map(|l| &l[..l.iter().position(|&b| b == 0).unwrap_or(l.len())]);
        let tag_id = tag_id_from_name(doc, lower);
        let fragment =
            lxb_html_parse_fragment_by_tag_id(parser, doc, tag_id, ns, bytes.as_ptr(), bytes.len());
        lxb_html_parser_destroy(parser);
        if fragment.is_null() {
            lxb_html_document_destroy(doc);
            return ptr::null_mut();
        }
        let out = ns_node_new_document();
        if scripting == 0 {
            if let Some(node) = Node::from_ptr(out) {
                node.add_flags(NODE_SCRIPTING_DISABLED);
            }
        }
        walk_into(fragment, out);
        ns_node_attach_backing(out, doc.cast(), Some(destroy_document));
        out
    }
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
    unsafe { parse_fragment_in_namespace(c_str(context_tag), LXB_NS_HTML, input, len, scripting) }
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
        (Some(_), Some(c)) if c.flags() & NODE_SVG_NS != 0 => LXB_NS_SVG,
        (Some(_), Some(c))
            if c.flags() & NODE_FOREIGN_NS != 0
                && c.attr(c"data-nd-ns-uri").is_some_and(|u| u == MATHML_URI) =>
        {
            LXB_NS_MATH
        }
        _ => LXB_NS_HTML,
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
    if ok && !out_root_ns.is_null() {
        if let Some(ns) = root_ns {
            unsafe { *out_root_ns = glib::strdup(&ns) };
        }
    }
    glib::boolean(ok)
}
