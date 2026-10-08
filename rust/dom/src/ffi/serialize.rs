//! Southstar — the C ABI of text collection, serialization, the debug dump and image maps declared in src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::{Node, NsNode, c_str};
use crate::image_map;
use crate::serialize::{self, Options};

#[repr(C)]
pub struct NsHtmlSerOpts {
    include_serializable: GBoolean,
    roots: *const *const NsNode,
    n_roots: c_int,
}

unsafe extern "C" {
    fn g_string_new_len(init: *const c_char, len: isize) -> *mut c_void;
}

unsafe fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_collect_text(root: *const NsNode) -> *mut c_char {
    glib::strdup(&serialize::collect_text(unsafe { node(root) }))
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

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_dump(n: *const NsNode) -> *mut c_void {
    let text = serialize::dump(unsafe { node(n) });
    unsafe { g_string_new_len(text.as_ptr().cast(), text.len() as isize) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_image_map_resolve(
    doc: *const NsNode,
    usemap: *const c_char,
    lx: f64,
    ly: f64,
    iw: f64,
    ih: f64,
    out_target: *mut *const c_char,
) -> *mut c_char {
    if !out_target.is_null() {
        unsafe { *out_target = ptr::null() };
    }
    let (Some(doc), Some(usemap)) = (unsafe { node(doc) }, c_str(usemap)) else {
        return ptr::null_mut();
    };
    let Some((href, target)) = image_map::resolve(doc, usemap, (lx, ly), (iw, ih)) else {
        return ptr::null_mut();
    };
    if !out_target.is_null() {
        unsafe { *out_target = target.map_or(ptr::null(), |t| t.as_ptr()) };
    }
    unsafe { glib::g_strdup(href.as_ptr()) }
}
