//! Southstar — the C ABI of element state: the pseudo-class test css.c's matcher delegates, an element's directionality, the target fragment, visited links and the document language, with the GLib Unicode, regex and URL calls they make.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::ptr;
use std::ffi::CString;

use southstar_dom::{Node, NsNode};
use southstar_glib::{self as glib, GBoolean};

use crate::element_state;

const RTL_SCRIPTS: [c_int; 7] = [19, 2, 34, 37, 66, 90, 95];

unsafe extern "C" {
    fn g_unichar_get_script(c: u32) -> c_int;
    fn g_unichar_isalpha(c: u32) -> GBoolean;
    fn g_regex_new(
        pattern: *const c_char,
        compile_options: c_uint,
        match_options: c_uint,
        error: *mut *mut c_void,
    ) -> *mut c_void;
    fn g_regex_match(
        regex: *const c_void,
        string: *const c_char,
        match_options: c_uint,
        match_info: *mut *mut c_void,
    ) -> GBoolean;
    fn g_regex_unref(regex: *mut c_void);
    fn g_clear_error(error: *mut *mut c_void);
    fn ns_url_resolve(base: *const c_char, href: *const c_char) -> *mut c_char;
    fn ns_url_is_valid_absolute(url: *const c_char) -> GBoolean;
}

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

pub(crate) fn unichar_is_rtl_script(c: u32) -> bool {
    RTL_SCRIPTS.contains(&unsafe { g_unichar_get_script(c) })
}

pub(crate) fn unichar_is_alpha(c: u32) -> bool {
    unsafe { g_unichar_isalpha(c) != 0 }
}

pub(crate) fn url_resolve(base: &[u8], href: &[u8]) -> Option<Vec<u8>> {
    let (base, href) = (c_string(base), c_string(href));
    let resolved = unsafe { ns_url_resolve(base.as_ptr(), href.as_ptr()) };
    unsafe { glib::GStr::take(resolved) }.map(|abs| abs.to_bytes().to_vec())
}

pub(crate) fn url_is_valid_absolute(url: &CStr) -> bool {
    unsafe { ns_url_is_valid_absolute(url.as_ptr()) != 0 }
}

pub(crate) fn regex_matches_whole(pattern: Option<&[u8]>, value: &[u8]) -> bool {
    let Some(pattern) = pattern.filter(|p| !p.is_empty()) else {
        return true;
    };
    let anchored = c_string(&[b"^(?:".as_slice(), pattern, b")$"].concat());
    let mut error = ptr::null_mut();
    let regex = unsafe { g_regex_new(anchored.as_ptr(), 0, 0, &mut error) };
    if regex.is_null() {
        unsafe { g_clear_error(&mut error) };
        return true;
    }
    let value = c_string(value);
    let matched = unsafe { g_regex_match(regex, value.as_ptr(), 0, ptr::null_mut()) != 0 };
    unsafe { g_regex_unref(regex) };
    matched
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_element_state_matches(
    el: *const NsNode,
    kind: c_uint,
    arg: *const c_char,
) -> GBoolean {
    let Some(el) = (unsafe { Node::from_ptr(el) }) else {
        return glib::FALSE;
    };
    glib::boolean(element_state::matches(el, kind, unsafe { bytes(arg) }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_node_dir(el: *const NsNode) -> *const c_char {
    unsafe { Node::from_ptr(el) }
        .map_or(c"ltr", element_state::node_dir)
        .as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_set_target_fragment(fragment: *const c_char) {
    element_state::set_target_fragment(unsafe { bytes(fragment) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_mark_visited(abs_url: *const c_char) {
    if let Some(url) = unsafe { bytes(abs_url) } {
        element_state::mark_visited(url);
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_set_doc_base(base_url: *const c_char) {
    element_state::set_doc_base(unsafe { bytes(base_url) });
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_set_doc_language(lang: *const c_char) {
    element_state::set_doc_language(unsafe { bytes(lang) });
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_css_language_cache_reset() {
    element_state::reset_language_cache();
}
