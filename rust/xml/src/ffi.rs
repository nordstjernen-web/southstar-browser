//! Southstar — the C ABI of the XML parser, as declared in src/html.h, building nodes through the DOM functions of src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_glib as glib;

use crate::Dom;

unsafe extern "C" {
    fn ns_node_new_document() -> *mut c_void;
    fn ns_node_new_element(name: *mut c_char) -> *mut c_void;
    fn ns_node_new_text(text: *mut c_char) -> *mut c_void;
    fn ns_node_new_comment(text: *mut c_char) -> *mut c_void;
    fn ns_node_set_name_owned(node: *mut c_void, name: *mut c_char);
    fn ns_node_add_flags(node: *mut c_void, flags: u32);
    fn ns_node_mark_doctype(node: *mut c_void);
    fn ns_node_append_child(parent: *mut c_void, child: *mut c_void);
    fn ns_node_free(node: *mut c_void);
    fn ns_element_set_attr(element: *mut c_void, name: *const c_char, value: *const c_char);
    fn ns_element_set_attr_ns(
        element: *mut c_void,
        namespace_uri: *const c_char,
        prefix: *const c_char,
        local_name: *const c_char,
        name: *const c_char,
        value: *const c_char,
    );
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&c| c == 0)
        .map_or(bytes, |nul| &bytes[..nul])
}

fn owned(bytes: &[u8]) -> *mut c_char {
    glib::strdup(until_nul(bytes))
}

fn borrowed(bytes: &[u8]) -> CString {
    CString::new(until_nul(bytes)).unwrap_or_default()
}

fn optional(bytes: Option<&[u8]>) -> Option<CString> {
    bytes.map(borrowed)
}

fn pointer(text: &Option<CString>) -> *const c_char {
    text.as_ref().map_or(ptr::null(), |text| text.as_ptr())
}

struct EngineDom;

impl Dom for EngineDom {
    type Node = *mut c_void;

    fn document(&mut self) -> *mut c_void {
        unsafe { ns_node_new_document() }
    }

    fn element(&mut self, name: Option<&[u8]>) -> *mut c_void {
        unsafe { ns_node_new_element(name.map_or(ptr::null_mut(), owned)) }
    }

    fn text(&mut self, text: &[u8]) -> *mut c_void {
        unsafe { ns_node_new_text(owned(text)) }
    }

    fn comment(&mut self, text: &[u8]) -> *mut c_void {
        unsafe { ns_node_new_comment(owned(text)) }
    }

    fn set_name(&mut self, node: *mut c_void, name: &[u8]) {
        unsafe { ns_node_set_name_owned(node, owned(name)) };
    }

    fn add_flags(&mut self, node: *mut c_void, flags: u32) {
        unsafe { ns_node_add_flags(node, flags) };
    }

    fn mark_doctype(&mut self, node: *mut c_void) {
        unsafe { ns_node_mark_doctype(node) };
    }

    fn set_attr(&mut self, element: *mut c_void, name: &[u8], value: &[u8]) {
        let (name, value) = (borrowed(name), borrowed(value));
        unsafe { ns_element_set_attr(element, name.as_ptr(), value.as_ptr()) };
    }

    fn set_attr_ns(
        &mut self,
        element: *mut c_void,
        namespace: Option<&[u8]>,
        prefix: Option<&[u8]>,
        local: &[u8],
        qualified: &[u8],
        value: &[u8],
    ) {
        let (namespace, prefix) = (optional(namespace), optional(prefix));
        let (local, qualified, value) = (borrowed(local), borrowed(qualified), borrowed(value));
        unsafe {
            ns_element_set_attr_ns(
                element,
                pointer(&namespace),
                pointer(&prefix),
                local.as_ptr(),
                qualified.as_ptr(),
                value.as_ptr(),
            )
        };
    }

    fn append(&mut self, parent: *mut c_void, child: *mut c_void) {
        unsafe { ns_node_append_child(parent, child) };
    }

    fn free(&mut self, node: *mut c_void) {
        unsafe { ns_node_free(node) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_xml_parse_reporting(
    input: *const c_char,
    len: isize,
    line: *mut c_int,
    column: *mut c_int,
) -> *mut c_void {
    unsafe {
        if !line.is_null() {
            *line = 1;
        }
        if !column.is_null() {
            *column = 1;
        }
    }
    if input.is_null() {
        return ptr::null_mut();
    }
    let input = if len < 0 {
        unsafe { glib::bytes(input) }.unwrap_or_default()
    } else {
        unsafe { glib::slice(input.cast(), len as usize) }
    };
    match crate::parse(&mut EngineDom, input) {
        Ok(doc) => doc,
        Err((at_line, at_column)) => {
            unsafe {
                if !line.is_null() {
                    *line = at_line;
                }
                if !column.is_null() {
                    *column = at_column;
                }
            }
            ptr::null_mut()
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_xml_parse(input: *const c_char, len: isize) -> *mut c_void {
    unsafe { ns_xml_parse_reporting(input, len, ptr::null_mut(), ptr::null_mut()) }
}
