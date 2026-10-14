//! Southstar — the C ABI of the form-control helpers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int};
use core::ptr;

use southstar_glib::{self as glib, GBoolean};

use super::{Node, NsNode, c_str};
use crate::controls::{self, Step};

const STEP_OK: c_int = 0;
const STEP_NOT_APPLICABLE: c_int = 1;
const STEP_NO_STEP: c_int = 2;
const STEP_UNCHANGED: c_int = 3;

unsafe fn node<'a>(n: *const NsNode) -> Option<Node<'a>> {
    unsafe { Node::from_ptr(n) }
}

fn opt_ptr(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

unsafe fn set_flag(out: *mut GBoolean, value: bool) {
    if !out.is_null() {
        unsafe { *out = glib::boolean(value) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_type_has_number_value(ty: *const c_char) -> GBoolean {
    glib::boolean(controls::type_has_number_value(c_str(ty)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_type_supports_readonly(ty: *const c_char) -> GBoolean {
    glib::boolean(controls::type_supports_readonly(c_str(ty)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_type_supports_text_constraints(ty: *const c_char) -> GBoolean {
    glib::boolean(controls::type_supports_text_constraints(c_str(ty)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_control_readonly_bars_validation(
    control: *const NsNode,
) -> GBoolean {
    glib::boolean(unsafe { node(control) }.is_some_and(controls::readonly_bars_validation))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_control_supports_required(control: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { node(control) }.is_some_and(controls::supports_required))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_value_to_number(
    ty: *const c_char,
    value: *const c_char,
    out: *mut f64,
) -> GBoolean {
    match controls::value_to_number(c_str(ty), c_str(value)) {
        Some(v) => {
            if !out.is_null() {
                unsafe { *out = v };
            }
            glib::TRUE
        }
        None => glib::FALSE,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_value_range_state(
    input: *const NsNode,
    value: *const c_char,
    underflow: *mut GBoolean,
    overflow: *mut GBoolean,
) -> GBoolean {
    unsafe {
        set_flag(underflow, false);
        set_flag(overflow, false);
    }
    let Some((under, over)) =
        unsafe { node(input) }.and_then(|i| controls::value_range_state(i, c_str(value)))
    else {
        return glib::FALSE;
    };
    unsafe {
        set_flag(underflow, under);
        set_flag(overflow, over);
    }
    glib::TRUE
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_step_apply(
    input: *const NsNode,
    sign: c_int,
    n: f64,
    buf: *mut c_char,
    buflen: usize,
) -> c_int {
    let Some(input) = (unsafe { node(input) }) else {
        return STEP_NOT_APPLICABLE;
    };
    match controls::step_apply(input, sign, n, buflen) {
        Step::Applied(text) => {
            unsafe {
                ptr::copy_nonoverlapping(text.as_ptr(), buf.cast::<u8>(), text.len());
                *buf.add(text.len()) = 0;
            }
            STEP_OK
        }
        Step::NotApplicable => STEP_NOT_APPLICABLE,
        Step::NoStep => STEP_NO_STEP,
        Step::Unchanged => STEP_UNCHANGED,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_value_step_mismatch(
    input: *const NsNode,
    value: *const c_char,
) -> GBoolean {
    glib::boolean(
        unsafe { node(input) }.is_some_and(|i| controls::value_step_mismatch(i, c_str(value))),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_control_value_missing(
    control: *const NsNode,
    value: *const c_char,
    doc: *const NsNode,
) -> GBoolean {
    let doc = unsafe { node(doc) };
    glib::boolean(
        unsafe { node(control) }.is_some_and(|c| controls::value_missing(c, c_str(value), doc)),
    )
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_email_value_valid(
    input: *const NsNode,
    value: *const c_char,
) -> GBoolean {
    glib::boolean(controls::email_value_valid(
        unsafe { node(input) },
        c_str(value),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_ce_attr_enables(contenteditable: *const c_char) -> GBoolean {
    glib::boolean(controls::ce_attr_enables(c_str(contenteditable)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_spellcheck_used(n: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { node(n) }.is_none_or(controls::spellcheck_used))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_node_spellcheck_host(n: *const NsNode) -> *const NsNode {
    Node::ptr_or_null(unsafe { node(n) }.and_then(controls::spellcheck_host))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_value_is_dirty_mode(n: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { node(n) }.is_some_and(controls::value_is_dirty_mode))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_used_value(n: *const NsNode) -> *const c_char {
    opt_ptr(unsafe { node(n) }.and_then(controls::used_value))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_textarea_default_value_dup(n: *const NsNode) -> *mut c_char {
    glib::strdup(&controls::textarea_default_value(unsafe { node(n) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_textarea_value_dup(n: *const NsNode) -> *mut c_char {
    glib::strdup(&controls::textarea_value(unsafe { node(n) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_input_is_one_line_text(n: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { node(n) }.is_some_and(controls::is_one_line_text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_reset_owned_controls(
    form: *mut NsNode,
    root: *mut NsNode,
    _doc: *const NsNode,
) {
    if let Some(form) = unsafe { node(form) } {
        controls::reset_owned_controls(form, unsafe { node(root) });
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_element_supports_disabled(el: *const NsNode) -> GBoolean {
    glib::boolean(unsafe { node(el) }.is_some_and(controls::supports_disabled))
}
