//! Southstar — the C ABI of the form helpers, as declared in src/forms.h, over the ns_node layout and the control functions of src/dom.h.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_long, c_uint, c_void};
use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr::{self, NonNull};

use southstar_glib::{self as glib, GBoolean, GError};

const NODE_ELEMENT: c_uint = 2;

#[repr(C)]
pub struct NsNode {
    kind: c_uint,
    name: *const c_char,
    _text: *const c_char,
    _text_len: u32,
    _attrs: *const c_void,
    parent: *const NsNode,
    first_child: *const NsNode,
    _last_child: *const NsNode,
    _prev_sibling: *const NsNode,
    next_sibling: *const NsNode,
}

unsafe extern "C" {
    fn ns_element_get_attr(el: *const NsNode, name: *const c_char) -> *const c_char;
    fn ns_form_owner(control: *const NsNode, doc: *const NsNode) -> *const NsNode;
    fn ns_element_effectively_disabled(el: *const NsNode) -> GBoolean;
    fn ns_select_chosen_option(select: *const NsNode) -> *const NsNode;
    fn ns_option_value_dup(option: *const NsNode) -> *mut c_char;
    fn ns_textarea_value_dup(n: *const NsNode) -> *mut c_char;
    fn ns_input_is_checked(n: *const NsNode) -> GBoolean;
    fn ns_input_used_value(n: *const NsNode) -> *const c_char;
    fn ns_input_email_value_valid(input: *const NsNode, value: *const c_char) -> GBoolean;
    fn ns_input_value_range_state(
        input: *const NsNode,
        value: *const c_char,
        under: *mut GBoolean,
        over: *mut GBoolean,
    ) -> GBoolean;
    fn ns_input_value_step_mismatch(input: *const NsNode, value: *const c_char) -> GBoolean;
    fn ns_input_type_has_number_value(ty: *const c_char) -> GBoolean;
    fn ns_input_type_supports_text_constraints(ty: *const c_char) -> GBoolean;
    fn ns_input_value_to_number(ty: *const c_char, value: *const c_char, out: *mut f64)
    -> GBoolean;
    fn ns_form_control_value_missing(
        control: *const NsNode,
        value: *const c_char,
        doc: *const NsNode,
    ) -> GBoolean;
    fn ns_form_control_readonly_bars_validation(control: *const NsNode) -> GBoolean;
    fn ns_form_control_length_limits_apply(control: *const NsNode) -> GBoolean;
    fn ns_form_control_supports_required(control: *const NsNode) -> GBoolean;
    fn ns_parse_int(s: *const c_char, dflt: c_int, min_v: c_int, max_v: c_int) -> c_int;
    fn ns_url_is_valid_absolute(url: *const c_char) -> GBoolean;
    fn ns_form_urlencoded_append_pair(
        out: *mut c_void,
        first: *mut GBoolean,
        name: *const c_char,
        value: *const c_char,
    );
    fn g_utf8_strlen(p: *const c_char, max: isize) -> c_long;
    fn g_regex_new(
        pattern: *const c_char,
        compile_options: c_uint,
        match_options: c_uint,
        error: *mut *mut GError,
    ) -> *mut c_void;
    fn g_regex_match(
        regex: *const c_void,
        string: *const c_char,
        match_options: c_uint,
        match_info: *mut *mut c_void,
    ) -> GBoolean;
    fn g_regex_unref(regex: *mut c_void);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Node<'a> {
    node: NonNull<NsNode>,
    tree: PhantomData<&'a NsNode>,
}

fn opt_ptr(s: Option<&CStr>) -> *const c_char {
    s.map_or(ptr::null(), CStr::as_ptr)
}

fn node_ptr(node: Option<Node>) -> *const NsNode {
    node.map_or(ptr::null(), |n| n.node.as_ptr())
}

impl<'a> Node<'a> {
    unsafe fn from_ptr(node: *const NsNode) -> Option<Self> {
        NonNull::new(node.cast_mut()).map(|node| Node {
            node,
            tree: PhantomData,
        })
    }

    fn get(self) -> &'a NsNode {
        unsafe { self.node.as_ref() }
    }

    fn link(link: *const NsNode) -> Option<Self> {
        unsafe { Self::from_ptr(link) }
    }

    fn ptr(self) -> *const NsNode {
        self.node.as_ptr()
    }

    pub fn element_name(self) -> Option<&'a [u8]> {
        let node = self.get();
        (node.kind == NODE_ELEMENT && !node.name.is_null())
            .then(|| unsafe { CStr::from_ptr(node.name) }.to_bytes())
    }

    pub fn parent(self) -> Option<Self> {
        Self::link(self.get().parent)
    }

    pub fn first_child(self) -> Option<Self> {
        Self::link(self.get().first_child)
    }

    pub fn next_sibling(self) -> Option<Self> {
        Self::link(self.get().next_sibling)
    }

    pub fn attr(self, name: &CStr) -> Option<&'a CStr> {
        let value = unsafe { ns_element_get_attr(self.ptr(), name.as_ptr()) };
        (!value.is_null()).then(|| unsafe { CStr::from_ptr(value) })
    }

    pub fn form_owner(self, doc: Option<Node>) -> Option<Self> {
        Self::link(unsafe { ns_form_owner(self.ptr(), node_ptr(doc)) })
    }

    pub fn effectively_disabled(self) -> bool {
        unsafe { ns_element_effectively_disabled(self.ptr()) != 0 }
    }

    pub fn chosen_option(self) -> Option<Self> {
        Self::link(unsafe { ns_select_chosen_option(self.ptr()) })
    }

    pub fn option_value(self) -> Option<GStr> {
        GStr::take(unsafe { ns_option_value_dup(self.ptr()) })
    }

    pub fn textarea_value(self) -> Option<GStr> {
        GStr::take(unsafe { ns_textarea_value_dup(self.ptr()) })
    }

    pub fn is_checked(self) -> bool {
        unsafe { ns_input_is_checked(self.ptr()) != 0 }
    }

    pub fn used_value(self) -> Option<&'a CStr> {
        let value = unsafe { ns_input_used_value(self.ptr()) };
        (!value.is_null()).then(|| unsafe { CStr::from_ptr(value) })
    }

    pub fn email_value_valid(self, value: &CStr) -> bool {
        unsafe { ns_input_email_value_valid(self.ptr(), value.as_ptr()) != 0 }
    }

    pub fn range_state(self, value: &CStr) -> Option<(bool, bool)> {
        let (mut under, mut over) = (0, 0);
        let known = unsafe {
            ns_input_value_range_state(self.ptr(), value.as_ptr(), &mut under, &mut over)
        };
        (known != 0).then_some((under != 0, over != 0))
    }

    pub fn step_mismatch(self, value: &CStr) -> bool {
        unsafe { ns_input_value_step_mismatch(self.ptr(), value.as_ptr()) != 0 }
    }

    pub fn value_missing(self, value: &CStr, doc: Option<Node>) -> bool {
        unsafe { ns_form_control_value_missing(self.ptr(), value.as_ptr(), node_ptr(doc)) != 0 }
    }

    pub fn readonly_bars_validation(self) -> bool {
        unsafe { ns_form_control_readonly_bars_validation(self.ptr()) != 0 }
    }

    pub fn length_limits_apply(self) -> bool {
        unsafe { ns_form_control_length_limits_apply(self.ptr()) != 0 }
    }

    pub fn supports_required(self) -> bool {
        unsafe { ns_form_control_supports_required(self.ptr()) != 0 }
    }
}

pub struct GStr(NonNull<c_char>);

impl GStr {
    fn take(s: *mut c_char) -> Option<Self> {
        NonNull::new(s).map(GStr)
    }
}

impl Deref for GStr {
    type Target = CStr;

    fn deref(&self) -> &CStr {
        unsafe { CStr::from_ptr(self.0.as_ptr()) }
    }
}

impl Drop for GStr {
    fn drop(&mut self) {
        unsafe { glib::g_free(self.0.as_ptr().cast()) };
    }
}

pub struct Query {
    out: *mut c_void,
    first: *mut GBoolean,
}

impl Query {
    pub fn append(&mut self, name: &CStr, value: Option<&CStr>) {
        unsafe {
            ns_form_urlencoded_append_pair(self.out, self.first, name.as_ptr(), opt_ptr(value))
        };
    }
}

pub fn type_has_number_value(ty: &CStr) -> bool {
    unsafe { ns_input_type_has_number_value(ty.as_ptr()) != 0 }
}

pub fn type_supports_text_constraints(ty: Option<&CStr>) -> bool {
    unsafe { ns_input_type_supports_text_constraints(opt_ptr(ty)) != 0 }
}

pub fn value_to_number(ty: &CStr, value: &CStr) -> bool {
    unsafe { ns_input_value_to_number(ty.as_ptr(), value.as_ptr(), ptr::null_mut()) != 0 }
}

pub fn parse_int(s: &CStr, default: c_int, min: c_int, max: c_int) -> c_int {
    unsafe { ns_parse_int(s.as_ptr(), default, min, max) }
}

pub fn url_is_valid_absolute(url: &CStr) -> bool {
    unsafe { ns_url_is_valid_absolute(url.as_ptr()) != 0 }
}

pub fn utf8_strlen(s: &CStr) -> c_long {
    unsafe { g_utf8_strlen(s.as_ptr(), -1) }
}

pub fn regex_matches(pattern: &CStr, value: &CStr) -> Option<bool> {
    let mut error = ptr::null_mut();
    let regex = unsafe { g_regex_new(pattern.as_ptr(), 0, 0, &mut error) };
    if regex.is_null() {
        if !error.is_null() {
            unsafe { glib::g_error_free(error) };
        }
        return None;
    }
    let matched = unsafe { g_regex_match(regex, value.as_ptr(), 0, ptr::null_mut()) != 0 };
    unsafe { g_regex_unref(regex) };
    Some(matched)
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_is_submit_trigger(n: *const NsNode) -> GBoolean {
    let node = unsafe { Node::from_ptr(n) };
    glib::boolean(node.is_some_and(crate::is_submit_trigger))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_is_reset_trigger(n: *const NsNode) -> GBoolean {
    let node = unsafe { Node::from_ptr(n) };
    glib::boolean(node.is_some_and(crate::is_reset_trigger))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_collect_inputs(
    form: *const NsNode,
    n: *const NsNode,
    doc: *const NsNode,
    query: *mut c_void,
    first: *mut GBoolean,
    submitter: *const NsNode,
) {
    let mut query = Query { out: query, first };
    unsafe {
        crate::collect_inputs(
            Node::from_ptr(form),
            Node::from_ptr(n),
            Node::from_ptr(doc),
            &mut query,
            Node::from_ptr(submitter),
            0,
        )
    };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_form_first_invalid(
    form: *const NsNode,
    n: *const NsNode,
    doc: *const NsNode,
) -> *const NsNode {
    let invalid = unsafe {
        crate::first_invalid(
            Node::from_ptr(form),
            Node::from_ptr(n),
            Node::from_ptr(doc),
            0,
        )
    };
    node_ptr(invalid)
}
