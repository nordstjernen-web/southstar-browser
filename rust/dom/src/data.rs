//! Southstar — the node slot: tree links and scalars in `Cell`s, and the name, text and attributes a node owns.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, RefCell, UnsafeCell};
use core::ffi::{CStr, c_char, c_int, c_void};
use std::ffi::CString;

use crate::Node;
use crate::index::Indexes;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Document,
    Doctype,
    Element,
    Text,
    Comment,
    Other,
}

pub type Invalidator = Option<fn(Node<'static>)>;

pub(crate) type Link = Cell<Option<&'static NodeSlot>>;

pub(crate) type ClassTokens = Box<[Box<[u8]>]>;

pub(crate) type AttrList = Vec<&'static AttrSlot>;

#[repr(C)]
pub struct NsNode {
    _opaque: [u8; 0],
}

pub(crate) const COMMON_NAMES: [&CStr; 225] = [
    c"a",
    c"abbr",
    c"accept",
    c"action",
    c"address",
    c"align",
    c"alt",
    c"area",
    c"aria-hidden",
    c"aria-label",
    c"article",
    c"aside",
    c"async",
    c"audio",
    c"autocomplete",
    c"autofocus",
    c"b",
    c"base",
    c"bdi",
    c"bdo",
    c"blockquote",
    c"body",
    c"br",
    c"button",
    c"canvas",
    c"caption",
    c"charset",
    c"checked",
    c"circle",
    c"cite",
    c"class",
    c"clip-rule",
    c"code",
    c"col",
    c"colgroup",
    c"cols",
    c"colspan",
    c"content",
    c"crossorigin",
    c"cx",
    c"cy",
    c"d",
    c"data",
    c"data-nd-ns-uri",
    c"data-src",
    c"datalist",
    c"datetime",
    c"dd",
    c"decoding",
    c"defer",
    c"defs",
    c"del",
    c"details",
    c"dfn",
    c"dialog",
    c"dir",
    c"disabled",
    c"div",
    c"dl",
    c"download",
    c"draggable",
    c"dt",
    c"ellipse",
    c"em",
    c"embed",
    c"enctype",
    c"fieldset",
    c"figcaption",
    c"figure",
    c"fill",
    c"fill-rule",
    c"footer",
    c"for",
    c"form",
    c"g",
    c"h1",
    c"h2",
    c"h3",
    c"h4",
    c"h5",
    c"h6",
    c"head",
    c"header",
    c"height",
    c"hgroup",
    c"hidden",
    c"hr",
    c"href",
    c"hreflang",
    c"html",
    c"http-equiv",
    c"i",
    c"id",
    c"iframe",
    c"img",
    c"input",
    c"ins",
    c"integrity",
    c"itemprop",
    c"itemscope",
    c"itemtype",
    c"kbd",
    c"label",
    c"lang",
    c"legend",
    c"li",
    c"line",
    c"link",
    c"loading",
    c"main",
    c"map",
    c"mark",
    c"math",
    c"max",
    c"maxlength",
    c"media",
    c"menu",
    c"meta",
    c"meter",
    c"method",
    c"min",
    c"multiple",
    c"name",
    c"nav",
    c"nonce",
    c"noscript",
    c"object",
    c"ol",
    c"onclick",
    c"onload",
    c"opacity",
    c"optgroup",
    c"option",
    c"output",
    c"p",
    c"param",
    c"path",
    c"pattern",
    c"picture",
    c"placeholder",
    c"points",
    c"polygon",
    c"polyline",
    c"poster",
    c"pre",
    c"preload",
    c"progress",
    c"property",
    c"publicId",
    c"q",
    c"r",
    c"readonly",
    c"rect",
    c"referrerpolicy",
    c"rel",
    c"required",
    c"role",
    c"rows",
    c"rowspan",
    c"rp",
    c"rt",
    c"ruby",
    c"rx",
    c"ry",
    c"s",
    c"samp",
    c"sandbox",
    c"script",
    c"search",
    c"section",
    c"select",
    c"selected",
    c"size",
    c"sizes",
    c"slot",
    c"small",
    c"source",
    c"span",
    c"src",
    c"srcset",
    c"start",
    c"stroke",
    c"stroke-width",
    c"strong",
    c"style",
    c"sub",
    c"summary",
    c"sup",
    c"svg",
    c"symbol",
    c"systemId",
    c"tabindex",
    c"table",
    c"target",
    c"tbody",
    c"td",
    c"template",
    c"textarea",
    c"tfoot",
    c"th",
    c"thead",
    c"time",
    c"title",
    c"tr",
    c"track",
    c"transform",
    c"translate",
    c"type",
    c"u",
    c"ul",
    c"use",
    c"value",
    c"var",
    c"video",
    c"viewBox",
    c"wbr",
    c"width",
    c"x",
    c"x1",
    c"x2",
    c"xlink:href",
    c"xmlns",
    c"y",
    c"y1",
    c"y2",
];

pub(crate) enum Name {
    Common(&'static CStr),
    Owned(Box<CStr>),
}

impl Name {
    pub(crate) fn new(bytes: &[u8]) -> Name {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        let bytes = &bytes[..end];
        match COMMON_NAMES.binary_search_by(|name| name.to_bytes().cmp(bytes)) {
            Ok(index) => Name::Common(COMMON_NAMES[index]),
            Err(_) => Name::Owned(CString::new(bytes).unwrap_or_default().into_boxed_c_str()),
        }
    }

    pub(crate) fn as_c_str(&self) -> &CStr {
        match self {
            Name::Common(name) => name,
            Name::Owned(name) => name,
        }
    }
}

const INLINE: usize = 24;

pub(crate) enum NulString {
    Inline { len: u8, buf: [u8; INLINE] },
    Heap(Vec<u8>),
}

impl NulString {
    pub(crate) fn new(bytes: &[u8]) -> Self {
        if bytes.len() < INLINE {
            let mut buf = [0; INLINE];
            buf[..bytes.len()].copy_from_slice(bytes);
            return NulString::Inline {
                len: bytes.len() as u8,
                buf,
            };
        }
        let mut buf = Vec::with_capacity(bytes.len() + 1);
        buf.extend_from_slice(bytes);
        buf.push(0);
        NulString::Heap(buf)
    }

    fn with_nul(&self) -> &[u8] {
        match self {
            NulString::Inline { len, buf } => &buf[..=usize::from(*len)],
            NulString::Heap(buf) => buf,
        }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.with_nul().split_last().map_or(&[], |(_, bytes)| bytes)
    }

    pub(crate) fn c_str(&self) -> &CStr {
        CStr::from_bytes_until_nul(self.with_nul()).unwrap_or_default()
    }

    pub(crate) fn as_ptr(&self) -> *const c_char {
        self.with_nul().as_ptr().cast()
    }

    pub(crate) fn push(&mut self, more: &[u8]) {
        match self {
            NulString::Heap(buf) => {
                buf.pop();
                buf.extend_from_slice(more);
                buf.push(0);
            }
            NulString::Inline { len, buf } => {
                let old = usize::from(*len);
                let total = old + more.len();
                if total < INLINE {
                    buf[old..total].copy_from_slice(more);
                    *len = total as u8;
                    return;
                }
                let mut heap = Vec::with_capacity((total + 1).next_power_of_two());
                heap.extend_from_slice(&buf[..old]);
                heap.extend_from_slice(more);
                heap.push(0);
                *self = NulString::Heap(heap);
            }
        }
    }
}

pub(crate) struct Namespaced {
    pub(crate) namespace_uri: Option<Name>,
    pub(crate) prefix: Option<Name>,
    pub(crate) local_name: Option<Name>,
}

pub(crate) struct AttrName {
    pub(crate) name: Name,
    pub(crate) namespaced: Option<Box<Namespaced>>,
    pub(crate) lowercase: bool,
}

impl AttrName {
    fn empty() -> AttrName {
        AttrName {
            name: Name::Common(c""),
            namespaced: None,
            lowercase: true,
        }
    }
}

pub(crate) struct NewAttr<'a> {
    pub name: &'a [u8],
    pub value: &'a [u8],
    pub namespace_uri: Option<&'a [u8]>,
    pub prefix: Option<&'a [u8]>,
    pub local_name: Option<&'a [u8]>,
}

pub(crate) struct AttrSlot {
    qualified: UnsafeCell<AttrName>,
    value: UnsafeCell<NulString>,
}

impl AttrSlot {
    pub(crate) fn vacant() -> AttrSlot {
        AttrSlot {
            qualified: UnsafeCell::new(AttrName::empty()),
            value: UnsafeCell::new(NulString::new(b"")),
        }
    }

    pub(crate) fn fill(&self, attr: NewAttr) {
        let namespaced =
            (attr.namespace_uri.is_some() || attr.prefix.is_some() || attr.local_name.is_some())
                .then(|| {
                    Box::new(Namespaced {
                        namespace_uri: attr.namespace_uri.map(Name::new),
                        prefix: attr.prefix.map(Name::new),
                        local_name: attr.local_name.map(Name::new),
                    })
                });
        self.replace_qualified(AttrName {
            name: Name::new(attr.name),
            namespaced,
            lowercase: !attr.name.iter().any(u8::is_ascii_uppercase),
        });
        self.replace_value(NulString::new(attr.value));
    }

    pub(crate) fn vacate(&self) {
        drop(self.replace_qualified(AttrName::empty()));
        drop(self.replace_value(NulString::new(b"")));
    }

    pub(crate) fn qualified(&self) -> &AttrName {
        // SAFETY: the names are only replaced by `replace_qualified`, when the slot is filled or vacated, and no caller holds a borrow of them across either.
        unsafe { &*self.qualified.get() }
    }

    fn replace_qualified(&self, qualified: AttrName) -> AttrName {
        // SAFETY: no reference into the cell is live while it is replaced; see `qualified`.
        unsafe { self.qualified.get().replace(qualified) }
    }

    pub(crate) fn value(&self) -> &NulString {
        // SAFETY: the value is only replaced by `replace_value`, and no caller replaces it while holding a borrow of it.
        unsafe { &*self.value.get() }
    }

    pub(crate) fn replace_value(&self, value: NulString) -> NulString {
        // SAFETY: no reference into the cell is live while it is replaced; see `value`.
        unsafe { self.value.get().replace(value) }
    }
}

pub(crate) struct NodeSlot {
    pub(crate) generation: Cell<u32>,
    pub(crate) kind: Cell<Kind>,
    pub(crate) flags: Cell<u32>,
    pub(crate) attr_gen: Cell<u32>,
    pub(crate) attr_bloom: Cell<u64>,
    pub(crate) source_position: Cell<(c_int, c_int)>,
    pub(crate) parent: Link,
    pub(crate) first_child: Link,
    pub(crate) last_child: Link,
    pub(crate) prev_sibling: Link,
    pub(crate) next_sibling: Link,
    pub(crate) template_content: Link,
    pub(crate) js_wrapper: Cell<*mut c_void>,
    pub(crate) js_invalidate: Cell<Invalidator>,
    pub(crate) indexes: RefCell<Option<Box<Indexes>>>,
    pub(crate) class_tokens: RefCell<Option<ClassTokens>>,
    name: UnsafeCell<Option<Name>>,
    text: UnsafeCell<Option<NulString>>,
    attrs: UnsafeCell<AttrList>,
}

impl NodeSlot {
    pub(crate) fn vacant() -> NodeSlot {
        NodeSlot {
            generation: Cell::new(0),
            kind: Cell::new(Kind::Other),
            flags: Cell::new(0),
            attr_gen: Cell::new(0),
            attr_bloom: Cell::new(0),
            source_position: Cell::new((0, 0)),
            parent: Cell::new(None),
            first_child: Cell::new(None),
            last_child: Cell::new(None),
            prev_sibling: Cell::new(None),
            next_sibling: Cell::new(None),
            template_content: Cell::new(None),
            js_wrapper: Cell::new(core::ptr::null_mut()),
            js_invalidate: Cell::new(None),
            indexes: RefCell::new(None),
            class_tokens: RefCell::new(None),
            name: UnsafeCell::new(None),
            text: UnsafeCell::new(None),
            attrs: UnsafeCell::new(Vec::new()),
        }
    }

    pub(crate) fn is_live(&self) -> bool {
        self.generation.get() & 1 == 1
    }

    pub(crate) fn name(&self) -> Option<&CStr> {
        // SAFETY: the name is only replaced by `replace_name`, and no caller replaces it while holding a borrow of it.
        unsafe { &*self.name.get() }.as_ref().map(Name::as_c_str)
    }

    pub(crate) fn replace_name(&self, name: Option<Name>) -> Option<Name> {
        // SAFETY: no reference into the cell is live while it is replaced; see `name`.
        unsafe { self.name.get().replace(name) }
    }

    pub(crate) fn text(&self) -> Option<&NulString> {
        // SAFETY: the text is only replaced by `replace_text`, and no caller replaces it while holding a borrow of it.
        unsafe { &*self.text.get() }.as_ref()
    }

    pub(crate) fn replace_text(&self, text: Option<NulString>) -> Option<NulString> {
        // SAFETY: no reference into the cell is live while it is replaced; see `text`.
        unsafe { self.text.get().replace(text) }
    }

    pub(crate) fn attr_at(&self, index: usize) -> Option<&'static AttrSlot> {
        // SAFETY: the list is only replaced by `replace_attrs`, and the shared borrow ends before this returns.
        unsafe { &*self.attrs.get() }.get(index).copied()
    }

    pub(crate) fn replace_attrs(&self, attrs: AttrList) -> AttrList {
        // SAFETY: no reference to the list itself outlives `attr_at`, so none is live while it is replaced.
        unsafe { self.attrs.get().replace(attrs) }
    }

    pub(crate) fn edit_attrs<R>(&self, edit: impl FnOnce(&mut AttrList) -> R) -> R {
        let mut attrs = self.replace_attrs(Vec::new());
        let result = edit(&mut attrs);
        self.replace_attrs(attrs);
        result
    }

    pub(crate) fn edit_text(&self, edit: impl FnOnce(&mut NulString)) {
        let mut text = self
            .replace_text(None)
            .unwrap_or_else(|| NulString::new(b""));
        edit(&mut text);
        self.replace_text(Some(text));
    }

    pub(crate) fn occupy(&self, kind: Kind) {
        self.generation.set(self.generation.get().wrapping_add(1));
        self.kind.set(kind);
    }

    pub(crate) fn vacate(&self) {
        drop(self.replace_name(None));
        drop(self.replace_text(None));
        for attr in self.replace_attrs(Vec::new()) {
            crate::store::drop_attr(attr);
        }
        drop(self.indexes.take());
        drop(self.class_tokens.take());
        self.kind.set(Kind::Other);
        self.flags.set(0);
        self.attr_gen.set(0);
        self.attr_bloom.set(0);
        self.source_position.set((0, 0));
        for link in [
            &self.parent,
            &self.first_child,
            &self.last_child,
            &self.prev_sibling,
            &self.next_sibling,
            &self.template_content,
        ] {
            link.set(None);
        }
        self.js_wrapper.set(core::ptr::null_mut());
        self.js_invalidate.set(None);
        self.generation.set(self.generation.get().wrapping_add(1));
    }
}
