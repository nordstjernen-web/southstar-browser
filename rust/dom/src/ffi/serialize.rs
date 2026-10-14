//! Southstar — the C ABI of text collection, serialization, the debug dump and image maps.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int};

use southstar_glib::{self as glib, GBoolean};

use super::{Node, NsNode};
use crate::serialize::{self, Options};

#[repr(C)]
pub struct NsHtmlSerOpts {
    include_serializable: GBoolean,
    roots: *const *const NsNode,
    n_roots: c_int,
}

unsafe fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_collect_all_text_len(
    root: *const NsNode,
    out_len: *mut usize,
) -> *mut c_char {
    let text = serialize::collect_all_text(unsafe { node(root) });
    if !out_len.is_null() {
        unsafe { *out_len = text.len() };
    }
    glib::strdup(&text)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_collect_all_text(root: *const NsNode) -> *mut c_char {
    glib::strdup(&serialize::collect_all_text(unsafe { node(root) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_is_embedded_doc(n: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { node(n) }.is_some_and(serialize::is_embedded_doc))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_inner_html(root: *const NsNode) -> *mut c_char {
    glib::strdup(&serialize::get_html(unsafe { node(root) }, None))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_get_html(
    root: *const NsNode,
    opts: *const NsHtmlSerOpts,
) -> *mut c_char {
    let opts = unsafe { opts.as_ref() }.map(|opts| {
        let roots = if opts.roots.is_null() || opts.n_roots <= 0 {
            &[][..]
        } else {
            unsafe { core::slice::from_raw_parts(opts.roots, opts.n_roots as usize) }
        };
        Options {
            include_serializable: opts.include_serializable != 0,
            roots: roots
                .iter()
                .filter_map(|&root| unsafe { node(root) })
                .collect(),
        }
    });
    glib::strdup(&serialize::get_html(unsafe { node(root) }, opts.as_ref()))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_outer_html(n: *const NsNode) -> *mut c_char {
    glib::strdup(&serialize::outer_html(unsafe { node(n) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_xml_outer_html(n: *const NsNode) -> *mut c_char {
    glib::strdup(&serialize::xml_outer_html(unsafe { node(n) }))
}
