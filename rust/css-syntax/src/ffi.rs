//! Southstar — the C ABI of the CSS Syntax parser, as declared in src/css_syntax.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::mem::{size_of, transmute};
use core::ptr;

use southstar_glib::{
    GBoolean, GPtrArray, TRUE, boolean, g_free, g_malloc0, g_ptr_array_add, g_ptr_array_free,
    g_ptr_array_new_with_free_func, g_strndup, slice, strdup,
};

use crate::{Component, Kind};

#[repr(C)]
pub struct NsCssComponent {
    kind: c_int,
    start: usize,
    end: usize,
    value: *mut c_char,
    number: f64,
    children: *mut GPtrArray,
    delimiter: c_char,
}

unsafe fn input_bytes<'a>(input: *const c_char, len: isize) -> &'a [u8] {
    if input.is_null() {
        &[]
    } else if len < 0 {
        unsafe { CStr::from_ptr(input) }.to_bytes()
    } else {
        unsafe { slice(input.cast(), len as usize) }
    }
}

unsafe fn component_value(input: &[u8], component: &Component) -> *mut c_char {
    match &component.value {
        None => ptr::null_mut(),
        Some(range) if component.kind == Kind::String => strdup(&input[range.clone()]),
        Some(range) => unsafe { g_strndup(input[range.clone()].as_ptr().cast(), range.len()) },
    }
}

unsafe fn component_new(input: &[u8], component: &Component) -> *mut NsCssComponent {
    unsafe {
        let out = g_malloc0(size_of::<NsCssComponent>()).cast::<NsCssComponent>();
        (*out).kind = component.kind as c_int;
        (*out).start = component.start;
        (*out).end = component.end;
        (*out).value = component_value(input, component);
        (*out).number = component.number;
        (*out).children = match &component.children {
            Some(children) => component_array(input, children),
            None => ptr::null_mut(),
        };
        (*out).delimiter = component.delimiter as c_char;
        out
    }
}

unsafe fn component_array(input: &[u8], components: &[Component]) -> *mut GPtrArray {
    unsafe {
        let free_func = transmute::<
            unsafe extern "C" fn(*mut NsCssComponent),
            unsafe extern "C" fn(*mut c_void),
        >(ns_css_component_free);
        let array = g_ptr_array_new_with_free_func(Some(free_func));
        for component in components {
            g_ptr_array_add(array, component_new(input, component).cast());
        }
        array
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_component_values_parse(
    input: *const c_char,
    len: isize,
    valid: *mut GBoolean,
) -> *mut GPtrArray {
    let input = unsafe { input_bytes(input, len) };
    let (components, ok) = crate::parse(input);
    if !valid.is_null() {
        unsafe { *valid = boolean(ok) };
    }
    unsafe { component_array(input, &components) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_component_free(component: *mut NsCssComponent) {
    if component.is_null() {
        return;
    }
    unsafe {
        g_free((*component).value.cast());
        if !(*component).children.is_null() {
            g_ptr_array_free((*component).children, TRUE);
        }
        g_free(component.cast());
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_syntax_scan(
    input: *const c_char,
    end: *const c_char,
    terminators: *const c_char,
    terminator: *mut c_char,
) -> *const c_char {
    let len = (end as usize).saturating_sub(input as usize);
    let text = unsafe { slice(input.cast(), len) };
    let terminators = unsafe { southstar_glib::bytes(terminators) }.unwrap_or_default();
    let (offset, found) = crate::scan(text, terminators);
    if !terminator.is_null() {
        unsafe { *terminator = found as c_char };
    }
    input.wrapping_add(offset)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_component_value_valid(input: *const c_char) -> GBoolean {
    boolean(crate::value_valid(unsafe { input_bytes(input, -1) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_syntax_is_self_contained(
    input: *const c_char,
    len: usize,
) -> GBoolean {
    boolean(crate::is_self_contained(unsafe {
        slice(input.cast(), len)
    }))
}
