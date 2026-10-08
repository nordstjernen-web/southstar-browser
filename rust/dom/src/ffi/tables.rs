//! Southstar — the GLib hash tables and pointer arrays behind the document's id, class and tag indexes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_uint, c_void};
use core::ptr::NonNull;

use southstar_glib::{self as glib, GHashTable, GPtrArray};

use super::{Node, NsNode};
use crate::index::{Bucket, document_order_cmp};

#[derive(Clone, Copy)]
pub struct IdTable(NonNull<GHashTable>);

#[derive(Clone, Copy)]
pub struct BucketTable(NonNull<GHashTable>);

pub struct NodeSet(NonNull<GHashTable>);

pub struct NodeArray(NonNull<GPtrArray>);

fn table(raw: *mut GHashTable) -> NonNull<GHashTable> {
    NonNull::new(raw).unwrap_or_else(|| std::process::abort())
}

fn new_string_table(value_free: glib::GDestroyNotify) -> NonNull<GHashTable> {
    table(unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_str_hash),
            Some(glib::g_str_equal),
            Some(glib::g_free),
            value_free,
        )
    })
}

impl IdTable {
    pub fn empty() -> Self {
        IdTable(new_string_table(None))
    }

    pub fn lookup<'a>(self, id: &CStr) -> Option<Node<'a>> {
        unsafe {
            Node::from_ptr(glib::g_hash_table_lookup(self.0.as_ptr(), id.as_ptr().cast()).cast())
        }
    }

    pub fn contains(self, id: &CStr) -> bool {
        unsafe { glib::g_hash_table_contains(self.0.as_ptr(), id.as_ptr().cast()) != 0 }
    }

    pub fn insert(self, id: &CStr, node: Node) {
        unsafe {
            glib::g_hash_table_insert(
                self.0.as_ptr(),
                glib::g_strdup(id.as_ptr()).cast(),
                node.as_mut_ptr().cast(),
            )
        };
    }

    pub fn replace(self, id: &CStr, node: Node) {
        unsafe {
            glib::g_hash_table_replace(
                self.0.as_ptr(),
                glib::g_strdup(id.as_ptr()).cast(),
                node.as_mut_ptr().cast(),
            )
        };
    }

    pub fn remove(self, id: &CStr) {
        unsafe { glib::g_hash_table_remove(self.0.as_ptr(), id.as_ptr().cast()) };
    }

    pub fn clear(self) {
        unsafe { glib::g_hash_table_remove_all(self.0.as_ptr()) };
    }
}

unsafe extern "C" fn bucket_free(bucket: *mut c_void) {
    drop(unsafe { Box::from_raw(bucket.cast::<Bucket>()) });
}

impl BucketTable {
    pub fn empty() -> Self {
        BucketTable(new_string_table(Some(bucket_free)))
    }

    pub fn with_bucket<R>(self, key: &CStr, f: impl FnOnce(&mut Bucket) -> R) -> Option<R> {
        let bucket = unsafe { glib::g_hash_table_lookup(self.0.as_ptr(), key.as_ptr().cast()) };
        NonNull::new(bucket.cast::<Bucket>()).map(|mut bucket| f(unsafe { bucket.as_mut() }))
    }

    pub fn insert(self, key: &CStr, bucket: Bucket) {
        let bucket = Box::into_raw(Box::new(bucket));
        unsafe {
            glib::g_hash_table_insert(
                self.0.as_ptr(),
                glib::g_strdup(key.as_ptr()).cast(),
                bucket.cast(),
            )
        };
    }

    pub fn clear(self) {
        unsafe { glib::g_hash_table_remove_all(self.0.as_ptr()) };
    }
}

impl NodeSet {
    pub fn empty() -> Self {
        NodeSet(table(unsafe {
            glib::g_hash_table_new(Some(glib::g_direct_hash), Some(glib::g_direct_equal))
        }))
    }

    pub fn add(&self, node: Node) -> bool {
        unsafe { glib::g_hash_table_add(self.0.as_ptr(), node.as_mut_ptr().cast()) != 0 }
    }

    pub fn remove(&self, node: Node) -> bool {
        unsafe { glib::g_hash_table_remove(self.0.as_ptr(), node.as_ptr().cast()) != 0 }
    }

    pub fn contains(&self, node: Node) -> bool {
        unsafe { glib::g_hash_table_contains(self.0.as_ptr(), node.as_ptr().cast()) != 0 }
    }

    pub fn len(&self) -> usize {
        unsafe { glib::g_hash_table_size(self.0.as_ptr()) as usize }
    }

    pub fn nodes(&self) -> Vec<Node<'static>> {
        let mut len: c_uint = 0;
        let keys = unsafe { glib::g_hash_table_get_keys_as_array(self.0.as_ptr(), &mut len) };
        let nodes = (0..len as usize)
            .filter_map(|i| unsafe { Node::from_ptr((*keys.add(i)).cast()) })
            .collect();
        unsafe { glib::g_free(keys.cast()) };
        nodes
    }
}

impl Drop for NodeSet {
    fn drop(&mut self) {
        unsafe { glib::g_hash_table_destroy(self.0.as_ptr()) };
    }
}

unsafe extern "C" fn order_compare(a: *const c_void, b: *const c_void) -> c_int {
    unsafe {
        let a = Node::from_ptr(*a.cast::<*const NsNode>());
        let b = Node::from_ptr(*b.cast::<*const NsNode>());
        document_order_cmp(a, b)
    }
}

impl NodeArray {
    pub fn empty() -> Self {
        NodeArray(
            NonNull::new(unsafe { glib::g_ptr_array_new() })
                .unwrap_or_else(|| std::process::abort()),
        )
    }

    fn raw(&self) -> &GPtrArray {
        unsafe { self.0.as_ref() }
    }

    pub fn len(&self) -> usize {
        self.raw().len as usize
    }

    pub fn get(&self, index: usize) -> Node<'static> {
        let node = unsafe { *self.raw().pdata.add(index) };
        unsafe { Node::from_ptr(node.cast()) }.unwrap_or_else(|| std::process::abort())
    }

    pub fn last(&self) -> Option<Node<'static>> {
        self.len().checked_sub(1).map(|i| self.get(i))
    }

    pub fn push(&self, node: Node) {
        unsafe { glib::g_ptr_array_add(self.0.as_ptr(), node.as_mut_ptr().cast()) };
    }

    pub fn insert(&self, index: usize, node: Node) {
        unsafe {
            glib::g_ptr_array_insert(self.0.as_ptr(), index as c_int, node.as_mut_ptr().cast())
        };
    }

    pub fn truncate(&self, len: usize) {
        unsafe { glib::g_ptr_array_set_size(self.0.as_ptr(), len as c_int) };
    }

    pub fn remove(&self, index: usize) {
        unsafe { glib::g_ptr_array_remove_index(self.0.as_ptr(), index as c_uint) };
    }

    pub fn sort_in_document_order(&self) {
        unsafe { glib::g_ptr_array_sort(self.0.as_ptr(), Some(order_compare)) };
    }

    pub fn as_ptr(&self) -> *mut GPtrArray {
        self.0.as_ptr()
    }
}

impl Drop for NodeArray {
    fn drop(&mut self) {
        unsafe { glib::g_ptr_array_free(self.0.as_ptr(), glib::TRUE) };
    }
}

impl Node<'_> {
    pub fn id_table(self) -> Option<IdTable> {
        NonNull::new(unsafe { (*self.as_mut_ptr()).id_index }).map(IdTable)
    }

    pub fn class_table(self) -> Option<BucketTable> {
        NonNull::new(unsafe { (*self.as_mut_ptr()).class_index }).map(BucketTable)
    }

    pub fn tag_table(self) -> Option<BucketTable> {
        NonNull::new(unsafe { (*self.as_mut_ptr()).tag_index }).map(BucketTable)
    }

    pub fn set_id_table(self, table: IdTable) {
        unsafe { (*self.as_mut_ptr()).id_index = table.0.as_ptr() };
    }

    pub fn set_class_table(self, table: BucketTable) {
        unsafe { (*self.as_mut_ptr()).class_index = table.0.as_ptr() };
    }

    pub fn set_tag_table(self, table: BucketTable) {
        unsafe { (*self.as_mut_ptr()).tag_index = table.0.as_ptr() };
    }
}
