//! Southstar — `Node` and `Attr`, the copyable handles every crate reads and edits the tree through.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::fmt;
use core::hash::{Hash, Hasher};
use core::marker::PhantomData;
use core::ptr;

use crate::Tree;
use crate::data::{
    AttrSlot, Invalidator, Kind, Name, Namespaced, NewAttr, NodeSlot, NsNode, NulString,
};
use crate::index::Indexes;
use crate::node::{KIND_COMMENT, KIND_DOCTYPE, KIND_DOCUMENT, KIND_ELEMENT, KIND_TEXT};
use crate::store::{self, WeakNode};

#[derive(Clone, Copy)]
pub struct Node<'a> {
    slot: &'static NodeSlot,
    tree: PhantomData<&'a NodeSlot>,
}

impl PartialEq for Node<'_> {
    fn eq(&self, other: &Self) -> bool {
        ptr::eq(self.slot, other.slot)
    }
}

impl Eq for Node<'_> {}

impl Hash for Node<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        ptr::hash(self.slot, state);
    }
}

impl fmt::Debug for Node<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Node")
            .field("kind", &self.kind())
            .field("name", &self.name())
            .field("at", &self.as_ptr())
            .finish()
    }
}

impl<'a> Node<'a> {
    pub(crate) fn of(slot: &'static NodeSlot) -> Self {
        Node {
            slot,
            tree: PhantomData,
        }
    }

    pub(crate) fn slot(self) -> &'static NodeSlot {
        self.slot
    }

    pub fn alloc(kind: Kind) -> Node<'static> {
        Node::of(store::occupy(kind))
    }

    /// # Safety
    ///
    /// `node` is null or a pointer `Node::as_ptr` returned on this thread.
    pub unsafe fn from_ptr(node: *const NsNode) -> Option<Self> {
        // SAFETY: the caller passes null or a slot of this thread's store, and store slots are never deallocated.
        unsafe { node.cast::<NodeSlot>().as_ref() }
            .filter(|slot| slot.is_live())
            .map(Node::of)
    }

    pub fn as_ptr(self) -> *const NsNode {
        ptr::from_ref(self.slot).cast()
    }

    pub fn as_mut_ptr(self) -> *mut NsNode {
        self.as_ptr().cast_mut()
    }

    pub fn ptr_or_null(node: Option<Self>) -> *const NsNode {
        node.map_or(ptr::null(), Node::as_ptr)
    }

    pub fn downgrade(self) -> WeakNode {
        WeakNode::new(self.slot)
    }

    fn link(link: Option<&'static NodeSlot>) -> Option<Self> {
        link.map(Node::of)
    }

    pub fn kind(self) -> Kind {
        self.slot.kind.get()
    }

    pub fn kind_raw(self) -> c_uint {
        match self.kind() {
            Kind::Document => KIND_DOCUMENT,
            Kind::Doctype => KIND_DOCTYPE,
            Kind::Element => KIND_ELEMENT,
            Kind::Text => KIND_TEXT,
            Kind::Comment => KIND_COMMENT,
            Kind::Other => c_uint::MAX,
        }
    }

    pub fn set_kind(self, kind: Kind) {
        self.slot.kind.set(kind);
    }

    pub fn is_element(self) -> bool {
        self.kind() == Kind::Element
    }

    pub fn is_text(self) -> bool {
        self.kind() == Kind::Text
    }

    pub fn flags(self) -> u32 {
        self.slot.flags.get()
    }

    pub fn add_flags(self, flags: u32) {
        self.slot.flags.set(self.flags() | flags);
    }

    pub fn remove_flags(self, flags: u32) {
        self.slot.flags.set(self.flags() & !flags);
    }

    pub fn name(self) -> Option<&'a CStr> {
        self.slot.name()
    }

    pub fn element_name(self) -> Option<&'a [u8]> {
        self.is_element()
            .then(|| self.name().map(CStr::to_bytes))
            .flatten()
    }

    pub fn set_name(self, name: &[u8]) {
        self.slot.replace_name(Some(Name::new(name)));
    }

    pub fn text(self) -> Option<&'a CStr> {
        self.slot.text().map(NulString::c_str)
    }

    pub fn text_with_len(self) -> Option<&'a [u8]> {
        self.slot.text().map(NulString::bytes)
    }

    pub fn text_len(self) -> u32 {
        self.text_with_len().map_or(0, |t| t.len() as u32)
    }

    pub fn text_ptr(self) -> *const c_char {
        self.slot.text().map_or(ptr::null(), NulString::as_ptr)
    }

    pub fn set_text(self, text: &[u8]) {
        self.slot.replace_text(Some(NulString::new(text)));
    }

    pub fn append_text(self, more: &[u8]) {
        self.slot.edit_text(|text| text.push(more));
    }

    pub fn parent(self) -> Option<Self> {
        Self::link(self.slot.parent.get())
    }

    pub fn first_child(self) -> Option<Self> {
        Self::link(self.slot.first_child.get())
    }

    pub fn last_child(self) -> Option<Self> {
        Self::link(self.slot.last_child.get())
    }

    pub fn prev_sibling(self) -> Option<Self> {
        Self::link(self.slot.prev_sibling.get())
    }

    pub fn next_sibling(self) -> Option<Self> {
        Self::link(self.slot.next_sibling.get())
    }

    pub fn root(self) -> Self {
        crate::tree::root(self)
    }

    pub fn tpl_content(self) -> Option<Self> {
        Self::link(self.slot.template_content.get())
    }

    pub fn set_tpl_content(self, content: Option<Node>) {
        self.slot.template_content.set(content.map(Node::slot));
    }

    pub fn take_tpl_content(self) -> Option<Tree> {
        Self::link(self.slot.template_content.take()).map(Tree::adopt)
    }

    pub fn attr(self, name: &CStr) -> Option<&'a CStr> {
        crate::attrs::get(self, name)
    }

    pub fn attrs(self) -> impl Iterator<Item = Attr<'a>> {
        (0..).map_while(move |index| self.slot.attr_at(index).map(Attr::of))
    }

    pub(crate) fn push_attr(self, attr: NewAttr) {
        let slot = store::new_attr();
        slot.fill(attr);
        self.slot.edit_attrs(|attrs| attrs.push(slot));
    }

    pub(crate) fn remove_first_attr(self, matches: impl Fn(Attr) -> bool) {
        if let Some(index) = self.attrs().position(matches) {
            store::drop_attr(self.slot.edit_attrs(|attrs| attrs.remove(index)));
        }
    }

    pub fn attr_gen(self) -> u32 {
        self.slot.attr_gen.get()
    }

    pub fn attr_bloom(self) -> u64 {
        self.slot.attr_bloom.get()
    }

    pub fn set_attr_bloom(self, bloom: u64) {
        self.slot.attr_bloom.set(bloom);
    }

    pub fn attrs_changed(self) {
        self.slot.attr_bloom.set(0);
        self.slot.attr_gen.set(self.attr_gen().wrapping_add(1));
    }

    pub fn has_class_set(self) -> bool {
        self.slot.class_tokens.borrow().is_some()
    }

    pub fn clear_class_set(self) {
        drop(self.slot.class_tokens.take());
    }

    pub fn class_set_contains(self, name: &[u8]) -> Option<bool> {
        let tokens = self.slot.class_tokens.borrow();
        tokens
            .as_ref()
            .map(|tokens| tokens.iter().any(|token| **token == *name))
    }

    pub fn set_class_set(self, tokens: &[&[u8]]) {
        let tokens = tokens.iter().map(|token| Box::from(*token)).collect();
        self.slot.class_tokens.replace(Some(tokens));
    }

    pub(crate) fn indexes(self) -> &'a RefCell<Option<Box<Indexes>>> {
        &self.slot.indexes
    }

    pub fn has_js_wrapper(self) -> bool {
        !self.js_wrapper().is_null()
    }

    pub fn js_wrapper(self) -> *mut c_void {
        self.slot.js_wrapper.get()
    }

    pub fn set_js_wrapper(self, wrapper: *mut c_void) {
        self.slot.js_wrapper.set(wrapper);
    }

    pub fn js_invalidate(self) -> Invalidator {
        self.slot.js_invalidate.get()
    }

    pub fn set_js_invalidate(self, invalidate: Invalidator) {
        self.slot.js_invalidate.set(invalidate);
    }

    pub fn set_source_position(self, line: c_int, col: c_int) {
        self.slot.source_position.set((line, col));
    }

    pub fn source_position(self) -> (c_int, c_int) {
        self.slot.source_position.get()
    }

    pub fn detach(self) {
        let child = self.slot;
        let Some(parent) = child.parent.take() else {
            return;
        };
        let prev = child.prev_sibling.take();
        let next = child.next_sibling.take();
        match prev {
            Some(prev) => prev.next_sibling.set(next),
            None => parent.first_child.set(next),
        }
        match next {
            Some(next) => next.prev_sibling.set(prev),
            None => parent.last_child.set(prev),
        }
    }

    pub fn append(self, child: Node) {
        child.detach();
        let (parent, child) = (self.slot, child.slot);
        child.parent.set(Some(parent));
        child.prev_sibling.set(parent.last_child.get());
        match parent.last_child.get() {
            Some(last) => last.next_sibling.set(Some(child)),
            None => parent.first_child.set(Some(child)),
        }
        parent.last_child.set(Some(child));
    }

    pub fn insert_after(self, child: Node) {
        let Some(parent) = self.parent().filter(|_| self != child) else {
            return;
        };
        child.detach();
        let (reference, child) = (self.slot, child.slot);
        let next = reference.next_sibling.get();
        child.parent.set(Some(parent.slot));
        child.prev_sibling.set(Some(reference));
        child.next_sibling.set(next);
        match next {
            Some(next) => next.prev_sibling.set(Some(child)),
            None => parent.slot.last_child.set(Some(child)),
        }
        reference.next_sibling.set(Some(child));
    }

    pub fn insert_before(self, child: Node) {
        let Some(parent) = self.parent().filter(|_| self != child) else {
            return;
        };
        child.detach();
        let (reference, child) = (self.slot, child.slot);
        let prev = reference.prev_sibling.get();
        child.parent.set(Some(parent.slot));
        child.next_sibling.set(Some(reference));
        child.prev_sibling.set(prev);
        match prev {
            Some(prev) => prev.next_sibling.set(Some(child)),
            None => parent.slot.first_child.set(Some(child)),
        }
        reference.prev_sibling.set(Some(child));
    }

    pub fn free_tree(self) {
        self.detach();
        let mut stack = vec![self.slot];
        while let Some(&cur) = stack.last() {
            if let Some(content) = cur.template_content.take() {
                stack.push(content);
                continue;
            }
            if let Some(first) = cur.first_child.take() {
                cur.last_child.set(None);
                let mut child = Some(first);
                while let Some(c) = child {
                    child = c.next_sibling.take();
                    c.prev_sibling.set(None);
                    c.parent.set(None);
                    stack.push(c);
                }
                continue;
            }
            stack.pop();
            release(cur);
        }
    }
}

fn release(slot: &'static NodeSlot) {
    if let Some(invalidate) = slot.js_invalidate.take() {
        invalidate(Node::of(slot));
    }
    store::vacate(slot);
}

#[derive(Clone, Copy)]
pub struct AttrView<'a> {
    pub name: &'a CStr,
    pub value: &'a CStr,
    pub namespace_uri: Option<&'a CStr>,
    pub prefix: Option<&'a CStr>,
    pub local_name: Option<&'a CStr>,
}

#[derive(Clone, Copy)]
pub struct Attr<'a> {
    slot: &'static AttrSlot,
    node: PhantomData<&'a NodeSlot>,
}

impl<'a> Attr<'a> {
    fn of(slot: &'static AttrSlot) -> Self {
        Attr {
            slot,
            node: PhantomData,
        }
    }

    pub fn name(self) -> Option<&'a CStr> {
        Some(self.slot.qualified().name.as_c_str())
    }

    pub fn name_is_lowercase(self) -> bool {
        self.slot.qualified().lowercase
    }

    pub fn name_first_byte(self) -> Option<u8> {
        let name = self.slot.qualified().name.as_c_str();
        Some(name.to_bytes().first().copied().unwrap_or(0))
    }

    pub fn value(self) -> Option<&'a CStr> {
        Some(self.slot.value().c_str())
    }

    pub fn value_bytes(self) -> Option<&'a [u8]> {
        Some(self.slot.value().bytes())
    }

    pub fn value_ptr(self) -> *const c_char {
        self.slot.value().as_ptr()
    }

    pub fn value_len(self) -> c_uint {
        self.slot.value().bytes().len() as c_uint
    }

    pub fn set_value(self, value: &[u8]) {
        drop(self.slot.replace_value(NulString::new(value)));
    }

    pub fn view(self) -> AttrView<'a> {
        AttrView {
            name: self.slot.qualified().name.as_c_str(),
            value: self.slot.value().c_str(),
            namespace_uri: self.namespace_uri(),
            prefix: self.prefix(),
            local_name: self.local_name(),
        }
    }

    fn namespaced(self, part: impl FnOnce(&Namespaced) -> Option<&Name>) -> Option<&'a CStr> {
        let namespaced = self.slot.qualified().namespaced.as_deref()?;
        part(namespaced).map(Name::as_c_str)
    }

    pub fn namespace_uri(self) -> Option<&'a CStr> {
        self.namespaced(|n| n.namespace_uri.as_ref())
    }

    pub fn prefix(self) -> Option<&'a CStr> {
        self.namespaced(|n| n.prefix.as_ref())
    }

    pub fn local_name(self) -> Option<&'a CStr> {
        self.namespaced(|n| n.local_name.as_ref())
    }
}
