//! Southstar — the per-thread box store whose slots are recycled rather than deallocated, and the box-tree links, text, attribute, link and atomic arrays tree building rewrites.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::ffi::{CStr, c_int, c_uint, c_void};
use std::ffi::CString;
use std::rc::Rc;

use southstar_dom::Node;

use super::{BoxRef, InlineAtomic, InlineAttr, LinkRange, NsBox, Style, TableColHint, peek};

const CHUNK_SLOTS: usize = 256;

thread_local! {
    static VACANT: RefCell<Vec<&'static NsBox>> = const { RefCell::new(Vec::new()) };
}

fn take_slot() -> &'static NsBox {
    VACANT.with_borrow_mut(|vacant| {
        if vacant.is_empty() {
            let chunk: &'static [NsBox] =
                Box::leak((0..CHUNK_SLOTS).map(|_| NsBox::vacant()).collect());
            vacant.extend(chunk.iter().rev());
        }
        vacant
            .pop()
            .expect("a refilled box store has a vacant slot")
    })
}

fn give_back(slot: &'static NsBox) {
    if !slot.live.get() {
        return;
    }
    slot.reset();
    VACANT.with_borrow_mut(|vacant| vacant.push(slot));
}

pub struct BoxPool;

impl BoxPool {
    pub fn new_box(kind: c_uint) -> BoxRef<'static> {
        let b = take_slot();
        b.live.set(true);
        b.kind.set(kind);
        BoxRef::of(b)
    }

    pub fn new_inline() -> BoxRef<'static> {
        let b = BoxPool::new_box(1);
        b.raw().attrs.set(Some(Rc::from([])));
        b
    }

    pub fn free_tree(root: Option<BoxRef<'_>>) {
        let Some(root) = root else {
            return;
        };
        let mut stack = vec![root.raw()];
        while let Some(cur) = stack.pop() {
            let mut c = cur.first_child.get();
            while let Some(child) = c {
                c = child.next_sibling.get();
                stack.push(child);
            }
            if let Some(atomics) = cur.inline_atomics.take() {
                stack.extend(atomics.iter().map(|a| a.b.raw()));
            }
            give_back(cur);
        }
    }
}

#[derive(Default)]
pub struct LinkVec(Vec<LinkRange>);

impl LinkVec {
    pub fn new() -> LinkVec {
        LinkVec(Vec::new())
    }

    pub fn push_copy(&mut self, l: &LinkRange, start: usize, len: usize) {
        self.0.push(l.at(start, len));
    }

    pub fn items(&self) -> &[LinkRange] {
        &self.0
    }

    pub fn items_mut(&mut self) -> &mut [LinkRange] {
        &mut self.0
    }

    pub fn push(&mut self, link: LinkRange) {
        self.0.push(link);
    }
}

impl<'a> BoxRef<'a> {
    pub fn set_dom(self, dom: Option<Node<'_>>) {
        self.raw().dom.set(dom.map(Node::downgrade));
    }

    pub fn set_style(self, style: *const Style) {
        self.raw().style.set(style);
    }

    pub fn set_spans(self, colspan: c_int, rowspan: c_int) {
        self.raw().colspan.set(colspan);
        self.raw().rowspan.set(rowspan);
    }

    pub fn inline_split_tail(self) -> bool {
        self.raw().inline_split_tail.get()
    }

    pub fn set_inline_split_tail(self, tail: bool) {
        self.raw().inline_split_tail.set(tail);
    }

    pub fn set_parent(self, parent: Option<BoxRef<'_>>) {
        self.raw().parent.set(parent.map(BoxRef::raw));
    }

    pub fn set_next_sibling(self, next: Option<BoxRef<'_>>) {
        self.raw().next_sibling.set(next.map(BoxRef::raw));
    }

    pub fn set_first_child(self, child: Option<BoxRef<'_>>) {
        self.raw().first_child.set(child.map(BoxRef::raw));
    }

    pub fn set_last_child(self, child: Option<BoxRef<'_>>) {
        self.raw().last_child.set(child.map(BoxRef::raw));
    }

    pub fn set_text_bytes(self, text: &[u8]) {
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        let text = CString::new(&text[..end]).unwrap_or_default();
        self.raw().text.set(Some(Rc::from(text)));
    }

    pub fn truncate_text(self, len: usize) {
        if let Some(text) = peek(&self.raw().text) {
            let bytes = text.to_bytes();
            self.set_text_bytes(&bytes[..len.min(bytes.len())]);
        }
    }

    pub fn take_attrs(self) -> Option<Vec<InlineAttr>> {
        self.raw().attrs.take().map(|a| a.to_vec())
    }

    pub fn put_attrs(self, attrs: Option<&[InlineAttr]>) {
        self.raw().attrs.set(attrs.map(Rc::from));
    }

    pub fn push_link(self, link: LinkRange) {
        let mut links = self.take_links().unwrap_or_default();
        links.push(link);
        self.put_links(Some(links));
    }

    pub fn has_links(self) -> bool {
        let links = self.raw().links.take();
        let has = links.is_some();
        self.raw().links.set(links);
        has
    }

    pub fn take_links(self) -> Option<LinkVec> {
        self.raw().links.take().map(|l| LinkVec(l.to_vec()))
    }

    pub fn put_links(self, links: Option<LinkVec>) {
        self.raw().links.set(links.map(|l| Rc::from(l.0)));
    }

    pub fn take_atomics(self) -> Option<Vec<InlineAtomic>> {
        self.raw().inline_atomics.take().map(|a| a.to_vec())
    }

    pub fn put_atomics(self, atomics: Option<&[InlineAtomic]>) {
        self.raw().inline_atomics.set(atomics.map(Rc::from));
    }

    pub fn push_table_col_hint(self, style: *const Style, span: c_int) {
        let mut hints = self
            .raw()
            .table_col_hints
            .take()
            .map(|h| h.to_vec())
            .unwrap_or_default();
        hints.push(TableColHint { style, span });
        self.raw().table_col_hints.set(Some(Rc::from(hints)));
    }

    pub fn invalidate_inline_caches(self) {
        let b = self.raw();
        b.inline_layout_cache.set(None);
        b.inline_natural_cache.set(None);
        b.inline_min_cache.set(None);
        b.paint_layout.set(None);
    }

    pub fn set_bg_image_if_unset(self, src: &CStr, image: *mut c_void) {
        self.ensure_media().set_bg_image_if_unset(src, image);
    }
}
