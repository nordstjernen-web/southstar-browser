//! Southstar — declaration blocks: a style rule's declarations, custom properties and waiting values captured as the parser reads it, its lazily compiled @container condition, blocks parsed into owned declarations, and the validity checks the parser and the bindings call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, OnceCell};
use core::ffi::{CStr, c_char, c_int, c_uint};
use core::ops::Deref;
use core::ptr::NonNull;
use std::collections::HashSet;

use southstar_glib::{self as glib, GHashTable};

use super::selector::SelectorGroup;
use super::sheet::Scope;
use super::shorthand::expanded;
use super::value::{NsCssValue, OwnedValue};
use crate::container::Query;
use crate::declarations::{self, Sink};

pub(super) struct Rule {
    pub(super) selectors: SelectorGroup,
    pub(super) decls: Vec<Declaration>,
    pub(super) vars: Option<VarTable>,
    pub(super) pending: Vec<PendingDecl>,
    pub(super) layer_name: Option<Vec<u8>>,
    pub(super) container_condition: Option<Vec<u8>>,
    container_query: OnceCell<Query>,
    pub(super) scopes: Vec<Scope>,
    pub(super) source_order: c_int,
    pub(super) pe_mask: Cell<c_uint>,
}

impl Rule {
    pub(super) fn new(layer_name: Option<&[u8]>, source_order: c_int) -> Rule {
        Rule {
            selectors: SelectorGroup::new(&[]),
            decls: Vec::new(),
            vars: None,
            pending: Vec::new(),
            layer_name: layer_name.map(|name| until_nul(name).to_vec()),
            container_condition: None,
            container_query: OnceCell::new(),
            scopes: Vec::new(),
            source_order,
            pe_mask: Cell::new(0),
        }
    }

    pub(super) fn container_matches(&self) -> bool {
        let condition = self.container_condition.as_deref().unwrap_or_default();
        self.container_query
            .get_or_init(|| Query::compile(condition))
            .matches()
    }
}

pub(super) struct PendingDecl {
    name: Vec<u8>,
    pub(super) raw_text: Vec<u8>,
    important: bool,
    decl_index: c_int,
    decl_rank: c_int,
}

pub(super) struct VarTable {
    table: NonNull<GHashTable>,
    important: Option<HashSet<Vec<u8>>>,
}

pub(super) fn until_nul(text: &[u8]) -> &[u8] {
    text.iter()
        .position(|&c| c == 0)
        .map_or(text, |nul| &text[..nul])
}

pub(super) fn lookup_key(text: &[u8]) -> &[u8] {
    if text.contains(&0) { &[] } else { text }
}

unsafe extern "C" {
    fn g_hash_table_iter_replace(iter: *mut glib::GHashTableIter, value: *mut core::ffi::c_void);
}

impl VarTable {
    fn new() -> VarTable {
        // SAFETY: the table owns its keys and values, which are GLib strings freed with g_free.
        let table = unsafe {
            glib::g_hash_table_new_full(
                Some(glib::g_str_hash),
                Some(glib::g_str_equal),
                Some(glib::g_free),
                Some(glib::g_free),
            )
        };
        VarTable {
            table: NonNull::new(table).expect("g_hash_table_new_full"),
            important: None,
        }
    }

    fn set(&mut self, name: &[u8], value: &[u8], important: bool) {
        // SAFETY: the table takes the two new GLib strings it is handed.
        unsafe {
            glib::g_hash_table_replace(
                self.table.as_ptr(),
                glib::strdup(name).cast(),
                glib::strdup(value).cast(),
            )
        };
        if important {
            self.important
                .get_or_insert_with(HashSet::new)
                .insert(until_nul(name).to_vec());
        } else if let Some(set) = &mut self.important {
            set.remove(lookup_key(name));
        }
    }

    pub(super) fn len(&self) -> usize {
        // SAFETY: the table is live for as long as the VarTable.
        unsafe { glib::g_hash_table_size(self.table.as_ptr()) as usize }
    }

    pub(super) fn entries(
        &self,
    ) -> impl Iterator<Item = (*const c_char, *const c_char, bool)> + '_ {
        // SAFETY: the table is live, and the iterator borrows the VarTable, which keeps it unchanged.
        let entries = unsafe { glib::hash_table_entries(self.table.as_ptr()) };
        entries.map(|(name, text)| {
            let name = name.cast_const().cast::<c_char>();
            let important = self.important.as_ref().is_some_and(|set| {
                // SAFETY: name is a key of the live table, a NUL-terminated string it owns.
                set.contains(unsafe { CStr::from_ptr(name) }.to_bytes())
            });
            (name, text.cast_const().cast(), important)
        })
    }

    pub(super) fn replace_texts(&mut self, mut replace: impl FnMut(&[u8]) -> Option<Vec<u8>>) {
        let mut iter = glib::GHashTableIter::new();
        // SAFETY: the table is live, and the VarTable is borrowed mutably while the iterator runs.
        unsafe { glib::g_hash_table_iter_init(&mut iter, self.table.as_ptr()) };
        let (mut key, mut value) = (core::ptr::null_mut(), core::ptr::null_mut());
        // SAFETY: iter was initialized over the table, which is not changed but through the iterator.
        while unsafe { glib::g_hash_table_iter_next(&mut iter, &mut key, &mut value) } != 0 {
            // SAFETY: the table's values are NUL-terminated strings it owns.
            let text = unsafe { CStr::from_ptr(value.cast()) }.to_bytes();
            if let Some(replaced) = replace(text) {
                // SAFETY: iter points at a live entry, and the table frees the value it replaces.
                unsafe { g_hash_table_iter_replace(&mut iter, glib::strdup(&replaced).cast()) };
            }
        }
    }
}

impl Drop for VarTable {
    fn drop(&mut self) {
        // SAFETY: the VarTable owns the table, and no entry outlives it.
        unsafe { glib::g_hash_table_destroy(self.table.as_ptr()) };
    }
}

pub(crate) fn media_query_matches(query: &[u8]) -> bool {
    southstar_css_media::query_matches(query)
}

pub(crate) fn syntax_def_valid(text: &[u8]) -> bool {
    southstar_css_prop_syntax::parse(text).is_some()
}

impl PendingDecl {
    pub(super) fn name(&self) -> Option<&[u8]> {
        Some(&self.name)
    }

    pub(super) fn raw_text(&self) -> Option<&[u8]> {
        Some(&self.raw_text)
    }

    pub(super) fn important(&self) -> bool {
        self.important
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

pub(super) struct RuleSink<'a> {
    rule: &'a mut Rule,
}

impl<'a> RuleSink<'a> {
    pub(super) fn new(rule: &'a mut Rule) -> RuleSink<'a> {
        RuleSink { rule }
    }
}

impl Sink for RuleSink<'_> {
    fn capturing(&self) -> bool {
        true
    }

    fn custom(&mut self, name: &[u8], value: &[u8], important: bool) {
        self.rule
            .vars
            .get_or_insert_with(VarTable::new)
            .set(name, value, important);
    }

    fn pending(&mut self, name: &[u8], raw: &[u8], important: bool) {
        let decl_index = self.rule.decls.len() as c_int;
        let decl_rank = match self.rule.pending.last() {
            Some(prev) if prev.decl_index == decl_index => prev.decl_rank + 1,
            _ => 0,
        };
        self.rule.pending.push(PendingDecl {
            name: until_nul(name).to_vec(),
            raw_text: until_nul(raw).to_vec(),
            important,
            decl_index,
            decl_rank,
        });
    }

    fn declaration(&mut self, name: &[u8], text: &[u8], important: bool) {
        let expanded = expanded(name, text).into_iter();
        self.rule
            .decls
            .extend(expanded.map(|(prop, value)| Declaration {
                prop,
                value,
                important,
            }));
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

    pub(super) fn value_ptr(&self) -> *mut NsCssValue {
        self.value.0
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
    pub(super) fn into_vec(self) -> Vec<Declaration> {
        self.0
    }

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
