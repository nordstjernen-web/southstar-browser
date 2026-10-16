//! Southstar — declaration blocks: declarations appended to css.c's arrays or parsed into an owned block, custom properties and waiting values captured into its ns_css_rule, and the validity checks css.c and the bindings call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::mem::size_of;
use core::ops::Deref;
use core::ptr;
use std::ffi::CString;

use southstar_glib::{self as glib, GArray, GBoolean, GHashTable, GPtrArray};

use super::shorthand::{append_expanded, expanded};
use super::value::{NsCssValue, OwnedValue};
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
}

fn c_string(text: &[u8]) -> CString {
    CString::new(text).unwrap_or_default()
}

pub(crate) fn media_query_matches(query: &[u8]) -> bool {
    southstar_css_media::query_matches(query)
}

pub(crate) fn syntax_def_valid(text: &[u8]) -> bool {
    southstar_css_prop_syntax::parse(text).is_some()
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

#[derive(Clone)]
pub struct Declaration {
    prop: c_int,
    value: OwnedValue,
    important: bool,
}

impl Declaration {
    pub fn prop(&self) -> i32 {
        self.prop
    }

    pub fn value(&self) -> &NsCssValue {
        &self.value
    }

    pub fn important(&self) -> bool {
        self.important
    }

    pub fn into_value(self) -> OwnedValue {
        self.value
    }
}

#[derive(Clone, Default)]
pub struct Declarations(Vec<Declaration>);

struct BlockSink(Vec<Declaration>);

impl Sink for BlockSink {
    fn capturing(&self) -> bool {
        false
    }

    fn custom(&mut self, _name: &[u8], _value: &[u8], _important: bool) {}

    fn pending(&mut self, _name: &[u8], _raw: &[u8], _important: bool) {}

    fn declaration(&mut self, name: &[u8], text: &[u8], important: bool) {
        let expanded = expanded(name, text).into_iter();
        self.0.extend(expanded.map(|(prop, value)| Declaration {
            prop,
            value,
            important,
        }));
    }
}

impl Declarations {
    pub fn parse(text: &[u8]) -> Declarations {
        let mut sink = BlockSink(Vec::new());
        declarations::parse_block(text, 0, &mut sink);
        Declarations(sink.0)
    }

    pub fn remove_important(&mut self) {
        self.0.retain(|decl| !decl.important);
    }

    pub fn into_first(self) -> Option<Declaration> {
        self.0.into_iter().next()
    }
}

impl Deref for Declarations {
    type Target = [Declaration];

    fn deref(&self) -> &[Declaration] {
        &self.0
    }
}
