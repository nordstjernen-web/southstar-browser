//! Southstar — the C ABI of declaration blocks: declarations appended to css.c's arrays, custom properties and waiting values captured into its ns_css_rule, and the validity checks and sizes evaluation css.c and the bindings call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_double, c_int, c_uint, c_void};
use core::mem::size_of;
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray};

use super::property::prop_of;
use super::shorthand::{RawDecl, append_expanded};
use super::value::ns_css_value_free;
use crate::declarations::{self, Sink};

#[repr(C)]
pub(super) struct RawRule {
    pub(super) selectors: *mut GPtrArray,
    pub(super) decls: *mut GArray,
    pub(super) vars: *mut GHashTable,
    pub(super) var_important: *mut GHashTable,
    pub(super) pending: *mut GArray,
    pub(super) layer_name: *mut c_char,
    pub(super) container_condition: *mut c_char,
    pub(super) container_query: *mut c_void,
    pub(super) scopes: *mut GPtrArray,
    pub(super) source_order: c_int,
    pub(super) pe_mask: c_uint,
}

#[repr(C)]
pub(super) struct RawPending {
    pname: *mut c_char,
    pub(super) raw_vtext: *mut c_char,
    important: GBoolean,
    decl_index: c_int,
    decl_rank: c_int,
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    size_of::<RawPending>() == 32
        && size_of::<RawRule>() == 80
        && core::mem::offset_of!(RawRule, scopes) == 64
);

unsafe extern "C" {
    fn g_array_set_clear_func(array: *mut GArray, clear_func: glib::GDestroyNotify);
    fn g_array_free(array: *mut GArray, free_segment: GBoolean) -> *mut c_char;
    fn ns_css_media_query_matches(query: *const c_char) -> GBoolean;
    fn ns_css_syntax_def_parse(text: *const c_char) -> *mut c_void;
    fn ns_css_syntax_def_free(syntax: *mut c_void);
}

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

pub(crate) fn media_query_matches(query: &[u8]) -> bool {
    let query = c_string(query);
    unsafe { ns_css_media_query_matches(query.as_ptr()) != 0 }
}

pub(crate) fn syntax_def_valid(text: &[u8]) -> bool {
    let text = c_string(text);
    let syntax = unsafe { ns_css_syntax_def_parse(text.as_ptr()) };
    if syntax.is_null() {
        return false;
    }
    unsafe { ns_css_syntax_def_free(syntax) };
    true
}

unsafe extern "C" fn pending_clear(data: *mut c_void) {
    let pending = data.cast::<RawPending>();
    unsafe {
        glib::g_free((*pending).pname.cast());
        glib::g_free((*pending).raw_vtext.cast());
    }
}

impl RawPending {
    pub(super) fn name(&self) -> Option<&[u8]> {
        (!self.pname.is_null()).then(|| unsafe { CStr::from_ptr(self.pname) }.to_bytes())
    }

    pub(super) fn raw_text(&self) -> Option<&[u8]> {
        (!self.raw_vtext.is_null()).then(|| unsafe { CStr::from_ptr(self.raw_vtext) }.to_bytes())
    }

    pub(super) fn important(&self) -> bool {
        self.important != 0
    }

    pub(super) fn decl_slot(&self) -> c_int {
        let rank = if self.decl_rank < DECL_SLOT_SPAN - 1 {
            self.decl_rank
        } else {
            DECL_SLOT_SPAN - 2
        };
        self.decl_index * DECL_SLOT_SPAN - DECL_SLOT_SPAN + 1 + rank
    }
}

pub(super) const DECL_SLOT_SPAN: c_int = 64;

pub(super) struct RuleSink {
    decls: *mut GArray,
    rule: *mut RawRule,
}

pub(super) fn parse_decls_into(text: &[u8], decls: *mut GArray) {
    let mut sink = RuleSink {
        decls,
        rule: ptr::null_mut(),
    };
    declarations::parse_block(text, 0, &mut sink);
}

impl RuleSink {
    pub(super) fn new(decls: *mut GArray, rule: *mut RawRule) -> RuleSink {
        RuleSink { decls, rule }
    }

    fn rule(&mut self) -> Option<&mut RawRule> {
        unsafe { self.rule.as_mut() }
    }
}

fn new_str_table(value_destroy: glib::GDestroyNotify) -> *mut GHashTable {
    unsafe {
        glib::g_hash_table_new_full(
            Some(glib::g_str_hash),
            Some(glib::g_str_equal),
            Some(glib::g_free),
            value_destroy,
        )
    }
}

impl Sink for RuleSink {
    fn capturing(&self) -> bool {
        !self.rule.is_null()
    }

    fn custom(&mut self, name: &[u8], value: &[u8], important: bool) {
        let Some(rule) = self.rule() else {
            return;
        };
        unsafe {
            if rule.vars.is_null() {
                rule.vars = new_str_table(Some(glib::g_free));
            }
            glib::g_hash_table_replace(
                rule.vars,
                glib::strdup(name).cast(),
                glib::strdup(value).cast(),
            );
            if important {
                if rule.var_important.is_null() {
                    rule.var_important = new_str_table(None);
                }
                glib::g_hash_table_add(rule.var_important, glib::strdup(name).cast());
            } else if !rule.var_important.is_null() {
                let key = c_string(name);
                glib::g_hash_table_remove(rule.var_important, key.as_ptr().cast());
            }
        }
    }

    fn pending(&mut self, name: &[u8], raw: &[u8], important: bool) {
        let Some(rule) = self.rule() else {
            return;
        };
        unsafe {
            if rule.pending.is_null() {
                rule.pending =
                    glib::g_array_new(glib::FALSE, glib::FALSE, size_of::<RawPending>() as c_uint);
                g_array_set_clear_func(rule.pending, Some(pending_clear));
            }
            let decl_index = rule.decls.as_ref().map_or(0, |decls| decls.len as c_int);
            let pending = &*rule.pending;
            let last = (pending.len > 0).then(|| {
                &*pending
                    .data
                    .cast::<RawPending>()
                    .add(pending.len as usize - 1)
            });
            let decl_rank = match last {
                Some(prev) if prev.decl_index == decl_index => prev.decl_rank + 1,
                _ => 0,
            };
            let entry = RawPending {
                pname: glib::strdup(name),
                raw_vtext: glib::strdup(raw),
                important: glib::boolean(important),
                decl_index,
                decl_rank,
            };
            glib::g_array_append_vals(rule.pending, (&raw const entry).cast(), 1);
        }
    }

    fn declaration(&mut self, name: &[u8], text: &[u8], important: bool) {
        unsafe { append_expanded(name, text, important, self.decls) };
    }
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_declaration_block(
    p: *const c_char,
    end: *const c_char,
    decls_out: *mut GArray,
    capture: *mut c_void,
) -> *const c_char {
    if p.is_null() || end <= p {
        return p;
    }
    let len = unsafe { end.offset_from(p) } as usize;
    let s = unsafe { core::slice::from_raw_parts(p.cast::<u8>(), len) };
    let mut sink = RuleSink {
        decls: decls_out,
        rule: capture.cast(),
    };
    let stop = declarations::parse_block(s, 0, &mut sink);
    unsafe { p.add(stop) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_declarations(text: *const c_char) -> *mut GArray {
    let decls =
        unsafe { glib::g_array_new(glib::FALSE, glib::FALSE, size_of::<RawDecl>() as c_uint) };
    if let Some(text) = unsafe { bytes(text) } {
        let mut sink = RuleSink {
            decls,
            rule: ptr::null_mut(),
        };
        declarations::parse_block(text, 0, &mut sink);
    }
    decls
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_declarations_free(decls: *mut GArray) {
    let Some(array) = (unsafe { decls.as_ref() }) else {
        return;
    };
    for i in 0..array.len as usize {
        unsafe { ns_css_value_free((*array.data.cast::<RawDecl>().add(i)).value) };
    }
    unsafe { g_array_free(decls, glib::TRUE) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_named_property_supported(name: *const c_char) -> GBoolean {
    glib::boolean(unsafe { bytes(name) }.is_some_and(declarations::named_property_supported))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_named_declaration_valid(
    name: *const c_char,
    text: *const c_char,
) -> GBoolean {
    let (Some(name), Some(text)) = (unsafe { bytes(name) }, unsafe { bytes(text) }) else {
        return glib::TRUE;
    };
    glib::boolean(declarations::named_declaration_valid(name, text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_declaration_valid(prop: c_int, text: *const c_char) -> GBoolean {
    let Some(text) = (unsafe { bytes(text) }) else {
        return glib::TRUE;
    };
    if prop < 0 {
        return glib::TRUE;
    }
    glib::boolean(declarations::declaration_valid(prop_of(prop), text))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_declaration_value_syntax_valid(text: *const c_char) -> GBoolean {
    glib::boolean(declarations::value_syntax_valid(
        unsafe { bytes(text) }.unwrap_or_default(),
    ))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_attr_unit_ident_valid(unit: *const c_char) -> GBoolean {
    glib::boolean(unsafe { bytes(unit) }.is_some_and(declarations::attr_unit_ident_valid))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_sizes_resolve(sizes: *const c_char) -> c_double {
    declarations::sizes_resolve(unsafe { bytes(sizes) }.unwrap_or_default())
}
