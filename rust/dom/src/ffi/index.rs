//! Southstar — the C ABI of the document indexes, document order and the tag, id and fragment lookups.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::node;
use crate::NsNode;
use crate::index;

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_id_index_build(doc: *mut NsNode) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(doc) = unsafe { node(doc) } {
        index::id_build(doc);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_class_index_build(doc: *mut NsNode) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(doc) = unsafe { node(doc) } {
        index::class_build(doc);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_doc_tag_index_build(doc: *mut NsNode) {
    // SAFETY: the C callers pass null or a node of this thread's store.
    if let Some(doc) = unsafe { node(doc) } {
        index::tag_build(doc);
    }
}
