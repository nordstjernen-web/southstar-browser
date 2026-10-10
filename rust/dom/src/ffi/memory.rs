//! Southstar — allocating, linking, rewriting and freeing ns_node and ns_attr in GLib memory.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint, c_void};
use core::mem::size_of;
use core::ptr::{self, NonNull};

use southstar_glib as glib;

use super::{Attr, BackingFree, Node, NsAttr, NsNode};

pub const NODE_OWN_NAME: u32 = 1 << 0;
pub const NODE_OWN_TEXT: u32 = 1 << 1;
pub const ATTR_OWN_NAME: u8 = 1 << 0;
pub const ATTR_OWN_VALUE: u8 = 1 << 1;
pub const ATTR_NAME_LOWER: u8 = 1 << 2;

unsafe extern "C" {
    fn ns_css_forget_node(node: *const NsNode);
    fn g_return_if_fail_warning(
        log_domain: *const c_char,
        pretty_function: *const c_char,
        expression: *const c_char,
    );
    fn g_memdup2(mem: *const c_void, byte_size: usize) -> *mut c_void;
}

pub fn return_if_fail(function: &CStr, expression: &CStr) {
    unsafe { g_return_if_fail_warning(ptr::null(), function.as_ptr(), expression.as_ptr()) };
}

pub fn dup(s: &CStr) -> *mut c_char {
    unsafe { glib::g_strdup(s.as_ptr()) }
}

pub fn dup_bytes(bytes: &[u8]) -> *mut c_char {
    glib::strdup(bytes)
}

pub fn dup_with_len(text: *const c_char, len: u32) -> *mut c_char {
    unsafe { g_memdup2(text.cast(), len as usize + 1).cast() }
}

pub fn free_string(s: *mut c_char) {
    unsafe { glib::g_free(s.cast()) };
}

pub fn c_strlen(s: *const c_char) -> u32 {
    if s.is_null() {
        0
    } else {
        unsafe { CStr::from_ptr(s) }.to_bytes().len() as u32
    }
}

pub struct NewAttr {
    pub name: *mut c_char,
    pub value: *mut c_char,
    pub value_len: c_uint,
    pub namespace_uri: *mut c_char,
    pub prefix: *mut c_char,
    pub local_name: *mut c_char,
    pub flags: u8,
}

struct ClassSet {
    tokens: Vec<(*const u8, usize)>,
}

impl<'a> Node<'a> {
    fn raw(self) -> *mut NsNode {
        self.as_mut_ptr()
    }

    pub fn alloc(kind: c_uint) -> Node<'static> {
        let node = unsafe { glib::g_malloc0(size_of::<NsNode>()) }.cast::<NsNode>();
        unsafe { (*node).kind = kind };
        unsafe { Node::from_ptr(node) }.unwrap_or_else(|| std::process::abort())
    }

    pub fn set_kind(self, kind: c_uint) {
        unsafe { (*self.raw()).kind = kind };
    }

    pub fn name_ptr(self) -> *mut c_char {
        unsafe { (*self.raw()).name }
    }

    pub fn text_ptr(self) -> *mut c_char {
        unsafe { (*self.raw()).text }
    }

    pub fn set_name(self, name: *mut c_char, owned: bool) {
        unsafe {
            let n = self.raw();
            if (*n).flags & NODE_OWN_NAME != 0 {
                free_string((*n).name);
            }
            (*n).name = name;
            if owned {
                (*n).flags |= NODE_OWN_NAME;
            } else {
                (*n).flags &= !NODE_OWN_NAME;
            }
        }
    }

    pub fn set_text(self, text: *mut c_char, len: u32, owned: bool) {
        unsafe {
            let n = self.raw();
            if (*n).flags & NODE_OWN_TEXT != 0 {
                free_string((*n).text);
            }
            (*n).text = text;
            (*n).text_len = len;
            if owned {
                (*n).flags |= NODE_OWN_TEXT;
            } else {
                (*n).flags &= !NODE_OWN_TEXT;
            }
        }
    }

    pub fn adopt_name(self, name: *mut c_char) {
        unsafe {
            (*self.raw()).name = name;
            (*self.raw()).flags |= NODE_OWN_NAME;
        }
    }

    pub fn adopt_text(self, text: *mut c_char, len: u32) {
        unsafe {
            (*self.raw()).text = text;
            (*self.raw()).text_len = len;
            (*self.raw()).flags |= NODE_OWN_TEXT;
        }
    }

    pub fn own_node_strings(self) {
        unsafe {
            let n = self.raw();
            if !(*n).name.is_null() && (*n).flags & NODE_OWN_NAME == 0 {
                (*n).name = glib::g_strdup((*n).name);
                (*n).flags |= NODE_OWN_NAME;
            }
            if !(*n).text.is_null() && (*n).flags & NODE_OWN_TEXT == 0 {
                (*n).text = glib::g_strdup((*n).text);
                (*n).flags |= NODE_OWN_TEXT;
            }
        }
        for attr in self.attrs() {
            attr.own_strings();
        }
    }

    pub fn set_tpl_content(self, content: Option<Node>) {
        unsafe { (*self.raw()).tpl_content = content.map_or(ptr::null_mut(), Node::as_mut_ptr) };
    }

    pub fn attach_backing(self, backing: *mut c_void, destroy: BackingFree) {
        unsafe {
            let n = self.raw();
            if !(*n).backing.is_null()
                && let Some(free) = (*n).backing_free
            {
                free((*n).backing);
            }
            (*n).backing = backing;
            (*n).backing_free = destroy;
        }
    }

    pub fn attr_bloom(self) -> u64 {
        unsafe { (*self.raw()).attr_bloom }
    }

    pub fn set_attr_bloom(self, bloom: u64) {
        unsafe { (*self.raw()).attr_bloom = bloom };
    }

    pub fn attrs_changed(self) {
        unsafe {
            (*self.raw()).attr_bloom = 0;
            (*self.raw()).attr_gen = (*self.raw()).attr_gen.wrapping_add(1);
        }
    }

    pub fn has_class_set(self) -> bool {
        unsafe { !(*self.raw()).class_set.is_null() }
    }

    pub fn clear_class_set(self) {
        unsafe {
            let n = self.raw();
            if !(*n).class_set.is_null() {
                drop(Box::from_raw((*n).class_set.cast::<ClassSet>()));
            }
            (*n).class_set = ptr::null_mut();
        }
    }

    pub fn class_set_contains(self, name: &[u8]) -> Option<bool> {
        let set = NonNull::new(unsafe { (*self.raw()).class_set }.cast::<ClassSet>())?;
        let set = unsafe { set.as_ref() };
        Some(set.tokens.iter().any(|&(p, len)| {
            len == name.len() && unsafe { core::slice::from_raw_parts(p, len) } == name
        }))
    }

    pub fn set_class_set(self, tokens: &[&'a [u8]]) {
        let set = ClassSet {
            tokens: tokens.iter().map(|t| (t.as_ptr(), t.len())).collect(),
        };
        unsafe { (*self.raw()).class_set = Box::into_raw(Box::new(set)).cast() };
    }

    pub fn detach(self) {
        unsafe {
            let child = self.raw();
            let parent = (*child).parent;
            if parent.is_null() {
                return;
            }
            let prev = (*child).prev_sibling;
            let next = (*child).next_sibling;
            if prev.is_null() {
                (*parent).first_child = next;
            } else {
                (*prev).next_sibling = next;
            }
            if next.is_null() {
                (*parent).last_child = prev;
            } else {
                (*next).prev_sibling = prev;
            }
            (*child).parent = ptr::null_mut();
            (*child).prev_sibling = ptr::null_mut();
            (*child).next_sibling = ptr::null_mut();
        }
    }

    pub fn append(self, child: Node) {
        child.detach();
        unsafe {
            let parent = self.raw();
            let child = child.raw();
            (*child).parent = parent;
            (*child).prev_sibling = (*parent).last_child;
            if (*parent).last_child.is_null() {
                (*parent).first_child = child;
            } else {
                (*(*parent).last_child).next_sibling = child;
            }
            (*parent).last_child = child;
        }
    }

    pub fn insert_after(self, child: Node) {
        if self == child || self.parent().is_none() {
            return;
        }
        child.detach();
        unsafe {
            let reference = self.raw();
            let parent = (*reference).parent;
            let child = child.raw();
            (*child).parent = parent;
            (*child).prev_sibling = reference;
            (*child).next_sibling = (*reference).next_sibling;
            if (*reference).next_sibling.is_null() {
                (*parent).last_child = child;
            } else {
                (*(*reference).next_sibling).prev_sibling = child;
            }
            (*reference).next_sibling = child;
        }
    }

    pub fn push_attr(self, attr: NewAttr) {
        let a = unsafe { glib::g_malloc0(size_of::<NsAttr>()) }.cast::<NsAttr>();
        unsafe {
            (*a).name = attr.name;
            (*a).value = attr.value;
            (*a).value_len = attr.value_len;
            (*a).namespace_uri = attr.namespace_uri;
            (*a).prefix = attr.prefix;
            (*a).local_name = attr.local_name;
            (*a).flags = attr.flags;
            let mut link = &raw mut (*self.raw()).attrs;
            while !(*link).is_null() {
                link = &raw mut (**link).next;
            }
            *link = a;
        }
    }

    pub fn remove_first_attr(self, matches: impl Fn(Attr) -> bool) {
        unsafe {
            let mut link = &raw mut (*self.raw()).attrs;
            while let Some(attr) = Attr::link(*link) {
                if matches(attr) {
                    *link = (**link).next;
                    free_attr(attr.attr.as_ptr());
                    return;
                }
                link = &raw mut (**link).next;
            }
        }
    }

    pub fn free_tree(self) {
        let mut stack = vec![self.raw()];
        while let Some(&cur) = stack.last() {
            unsafe {
                if !(*cur).tpl_content.is_null() {
                    stack.push((*cur).tpl_content);
                    (*cur).tpl_content = ptr::null_mut();
                    continue;
                }
                if !(*cur).first_child.is_null() {
                    let mut c = (*cur).first_child;
                    (*cur).first_child = ptr::null_mut();
                    while !c.is_null() {
                        let next = (*c).next_sibling;
                        (*c).next_sibling = ptr::null_mut();
                        (*c).parent = ptr::null_mut();
                        stack.push(c);
                        c = next;
                    }
                    continue;
                }
                stack.pop();
                free_one(cur);
            }
        }
    }
}

unsafe fn free_one(cur: *mut NsNode) {
    unsafe {
        if let Some(invalidate) = (*cur).js_invalidate {
            invalidate(cur);
        }
        ns_css_forget_node(cur);
        if (*cur).flags & NODE_OWN_NAME != 0 {
            free_string((*cur).name);
        }
        if (*cur).flags & NODE_OWN_TEXT != 0 {
            free_string((*cur).text);
        }
        if let Some(node) = Node::from_ptr(cur) {
            node.clear_class_set();
        }
        let mut attr = (*cur).attrs;
        while !attr.is_null() {
            let next = (*attr).next;
            free_attr(attr);
            attr = next;
        }
        if !(*cur).backing.is_null()
            && let Some(free) = (*cur).backing_free
        {
            free((*cur).backing);
        }
        for table in [
            &raw mut (*cur).id_index,
            &raw mut (*cur).class_index,
            &raw mut (*cur).tag_index,
        ] {
            if !(*table).is_null() {
                glib::g_hash_table_destroy(*table);
                *table = ptr::null_mut();
            }
        }
        glib::g_free(cur.cast());
    }
}

unsafe fn free_attr(a: *mut NsAttr) {
    unsafe {
        if (*a).flags & ATTR_OWN_NAME != 0 {
            free_string((*a).name);
        }
        if (*a).flags & ATTR_OWN_VALUE != 0 {
            free_string((*a).value);
        }
        free_string((*a).namespace_uri);
        free_string((*a).prefix);
        free_string((*a).local_name);
        glib::g_free(a.cast());
    }
}

impl Attr<'_> {
    fn raw(self) -> *mut NsAttr {
        self.attr.as_ptr()
    }

    pub fn set_value(self, value: *mut c_char, len: c_uint) {
        unsafe {
            let a = self.raw();
            if (*a).flags & ATTR_OWN_VALUE != 0 {
                free_string((*a).value);
            }
            (*a).value = value;
            (*a).value_len = len;
            (*a).flags |= ATTR_OWN_VALUE;
        }
    }

    fn own_strings(self) {
        unsafe {
            let a = self.raw();
            if !(*a).name.is_null() && (*a).flags & ATTR_OWN_NAME == 0 {
                (*a).name = glib::g_strdup((*a).name);
                (*a).flags |= ATTR_OWN_NAME;
            }
            if !(*a).value.is_null() && (*a).flags & ATTR_OWN_VALUE == 0 {
                (*a).value = value_dup((*a).value, (*a).value_len as usize);
                (*a).flags |= ATTR_OWN_VALUE;
            }
        }
    }
}

pub fn value_dup(value: *const c_char, len: usize) -> *mut c_char {
    unsafe {
        let v = glib::g_malloc(len + 1).cast::<c_char>();
        if len > 0 && !value.is_null() {
            ptr::copy_nonoverlapping(value, v, len);
        }
        *v.add(len) = 0;
        v
    }
}
