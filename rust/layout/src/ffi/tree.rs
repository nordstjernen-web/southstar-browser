//! Southstar — allocating and freeing boxes through the box pool, and the box-tree links, text, attribute, link and atomic arrays tree building rewrites.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::ptr::{self, NonNull};
use std::sync::Mutex;

use southstar_glib::{GBoolean, g_free, g_malloc, g_malloc0, g_strdup};

use super::{BoxRef, InlineAtomic, InlineAttr, LinkRange, NsBox, NsBoxMedia, Style, TableColHint};

const POOL_MAX: usize = 16384;

static POOL: Mutex<Vec<usize>> = Mutex::new(Vec::new());

pub struct BoxPool;

impl BoxPool {
    pub fn new_box(kind: c_uint) -> BoxRef<'static> {
        let reused = POOL.lock().ok().and_then(|mut pool| pool.pop());
        let b = match reused {
            Some(addr) => {
                let b = addr as *mut NsBox;
                unsafe { ptr::write_bytes(b, 0, 1) };
                b
            }
            None => unsafe { g_malloc0(size_of::<NsBox>()) }.cast::<NsBox>(),
        };
        unsafe {
            (*b).kind = kind;
            (*b).colspan = 1;
            (*b).rowspan = 1;
        }
        unsafe { BoxRef::from_ptr(b) }.expect("g_malloc0 aborts on failure")
    }

    pub fn new_inline() -> BoxRef<'static> {
        let b = BoxPool::new_box(1);
        // SAFETY: b is the box just allocated, and nothing else refers to it yet.
        unsafe { (*b.0.as_ptr()).attrs = Some(Box::default()) };
        b
    }

    pub fn free_tree(root: Option<BoxRef<'_>>) {
        let Some(root) = root else {
            return;
        };
        let mut stack = vec![root.0.as_ptr()];
        while let Some(cur) = stack.pop() {
            unsafe { release(cur, &mut stack) };
            let mut pool = POOL.lock().ok();
            match pool.as_mut() {
                Some(pool) if pool.len() < POOL_MAX => pool.push(cur as usize),
                _ => unsafe { g_free(cur.cast()) },
            }
        }
    }
}

unsafe fn release(cur: *mut NsBox, stack: &mut Vec<*mut NsBox>) {
    unsafe {
        let mut c = (*cur).first_child.cast_mut();
        while !c.is_null() {
            let next = (*c).next_sibling.cast_mut();
            stack.push(c);
            c = next;
        }
        if !(*cur).paint_layout.is_null()
            && let Some(b) = NonNull::new(cur)
        {
            crate::paint_links::drop_box_cache(BoxRef(b, PhantomData));
        }
        let b = &mut *cur;
        b.links = None;
        b.attrs = None;
        b.table_col_hints = None;
        b.grid_col_tracks = None;
        b.grid_row_tracks = None;
        if let Some(atomics) = b.inline_atomics.take() {
            stack.extend(
                atomics
                    .iter()
                    .map(|a| a.b.cast_mut())
                    .filter(|ab| !ab.is_null()),
            );
        }
        b.atomic_line_heights = None;
        g_free((*cur).text.cast_mut().cast());
        let media = (*cur).media;
        if !media.is_null() {
            let m = &mut *media;
            g_free(m.image_src.cast());
            g_free(m.bg_image_src.cast());
            g_free(m.marker_image_src.cast());
            g_free(m.border_image_src.cast());
            m.bg_layer_srcs = None;
            m.bg_layer_images = None;
            g_free(m.video_src.cast());
            g_free(m.video_poster.cast());
            g_free(m.video_audio_src.cast());
            g_free(media.cast());
        }
    }
}

fn dup(p: *const c_char) -> *const c_char {
    if p.is_null() {
        ptr::null()
    } else {
        unsafe { g_strdup(p) }
    }
}

pub struct LinkVec(Vec<LinkRange>);

impl Default for LinkVec {
    fn default() -> LinkVec {
        LinkVec::new()
    }
}

impl LinkVec {
    pub fn new() -> LinkVec {
        LinkVec(Vec::new())
    }

    pub fn push_copy(&mut self, l: &LinkRange, start: usize, len: usize) {
        self.0.push(LinkRange {
            start,
            len,
            href: dup(l.href),
            target: dup(l.target),
            dom: l.dom,
        });
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

impl Drop for LinkVec {
    fn drop(&mut self) {
        for l in &self.0 {
            unsafe {
                g_free(l.href.cast_mut().cast());
                g_free(l.target.cast_mut().cast());
            }
        }
    }
}

fn raw_ptr(b: Option<BoxRef<'_>>) -> *const NsBox {
    b.map_or(ptr::null(), BoxRef::as_ptr)
}

impl<'a> BoxRef<'a> {
    fn ptr_mut(self) -> *mut NsBox {
        self.0.as_ptr()
    }

    pub fn set_dom(self, dom: *const c_void) {
        unsafe { (*self.ptr_mut()).dom = dom };
    }

    pub fn set_style(self, style: *const Style) {
        unsafe { (*self.ptr_mut()).style = style };
    }

    pub fn set_spans(self, colspan: c_int, rowspan: c_int) {
        unsafe {
            (*self.ptr_mut()).colspan = colspan;
            (*self.ptr_mut()).rowspan = rowspan;
        }
    }

    pub fn inline_split_tail(self) -> bool {
        self.raw().inline_split_tail != 0
    }

    pub fn set_inline_split_tail(self, tail: bool) {
        unsafe { (*self.ptr_mut()).inline_split_tail = GBoolean::from(tail) };
    }

    pub fn last_child(self) -> Option<BoxRef<'a>> {
        unsafe { BoxRef::from_ptr(self.raw().last_child) }
    }

    pub fn set_parent(self, parent: Option<BoxRef<'_>>) {
        unsafe { (*self.ptr_mut()).parent = raw_ptr(parent) };
    }

    pub fn set_next_sibling(self, next: Option<BoxRef<'_>>) {
        unsafe { (*self.ptr_mut()).next_sibling = raw_ptr(next) };
    }

    pub fn set_first_child(self, child: Option<BoxRef<'_>>) {
        unsafe { (*self.ptr_mut()).first_child = raw_ptr(child) };
    }

    pub fn set_last_child(self, child: Option<BoxRef<'_>>) {
        unsafe { (*self.ptr_mut()).last_child = raw_ptr(child) };
    }

    pub fn set_text_bytes(self, text: &[u8]) {
        unsafe {
            let buf = g_malloc(text.len() + 1).cast::<u8>();
            ptr::copy_nonoverlapping(text.as_ptr(), buf, text.len());
            *buf.add(text.len()) = 0;
            g_free((*self.ptr_mut()).text.cast_mut().cast());
            (*self.ptr_mut()).text = buf.cast();
        }
    }

    pub fn truncate_text(self, len: usize) {
        let text = self.raw().text.cast_mut();
        if !text.is_null() {
            unsafe { *text.add(len) = 0 };
        }
    }

    fn fields_mut(self) -> &'a mut NsBox {
        // SAFETY: self.0 points to a live box, and callers hold no other reference into it across the update.
        unsafe { &mut *self.ptr_mut() }
    }

    pub fn take_attrs(self) -> Option<Vec<InlineAttr>> {
        self.fields_mut().attrs.take().map(|a| *a)
    }

    pub fn put_attrs(self, attrs: Option<&[InlineAttr]>) {
        self.fields_mut().attrs = attrs.map(|a| Box::new(a.to_vec()));
    }

    pub fn push_link(self, link: LinkRange) {
        self.fields_mut().links.get_or_insert_default().push(link);
    }

    pub fn has_links(self) -> bool {
        self.raw().links.is_some()
    }

    pub fn take_links(self) -> Option<LinkVec> {
        self.fields_mut().links.take().map(|l| *l)
    }

    pub fn put_links(self, links: Option<LinkVec>) {
        self.fields_mut().links = links.map(Box::new);
    }

    pub fn take_atomics(self) -> Option<Vec<InlineAtomic>> {
        self.fields_mut().inline_atomics.take().map(|a| *a)
    }

    pub fn put_atomics(self, atomics: Option<&[InlineAtomic]>) {
        self.fields_mut().inline_atomics = atomics.map(|a| Box::new(a.to_vec()));
    }

    pub fn push_table_col_hint(self, style: *const Style, span: c_int) {
        self.fields_mut()
            .table_col_hints
            .get_or_insert_default()
            .push(TableColHint { style, span });
    }

    pub fn invalidate_inline_caches(self) {
        let b = self.ptr_mut();
        unsafe {
            (*b).inline_layout_cache_valid = 0;
            (*b).inline_natural_cache_valid = 0;
            (*b).inline_min_cache_valid = 0;
            if !(*b).paint_layout.is_null() {
                crate::paint_links::drop_box_cache(self);
            }
        }
    }

    pub fn set_bg_image_if_unset(self, src: &CStr, image: *mut c_void) {
        let b = self.ptr_mut();
        unsafe {
            if (*b).media.is_null() {
                (*b).media = g_malloc0(size_of::<NsBoxMedia>()).cast();
            }
            let m = &mut *(*b).media;
            if m.bg_image_src.is_null() {
                m.bg_image_src = g_strdup(src.as_ptr());
                m.bg_image = image;
            }
        }
    }
}

impl InlineAtomic {
    pub fn box_handle<'a>(&self) -> Option<BoxRef<'a>> {
        unsafe { BoxRef::from_ptr(self.b) }
    }
}
