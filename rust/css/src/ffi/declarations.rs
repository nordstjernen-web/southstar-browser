//! Southstar — declaration blocks: a style rule's declarations, custom properties and waiting values captured as the parser reads it, its lazily compiled @container condition, blocks parsed into owned declarations, and the validity checks the parser and the bindings call.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::{Cell, OnceCell};
use core::ffi::{c_char, c_int, c_uint};
use core::ops::Deref;
use std::collections::{HashMap, HashSet};
use std::ffi::CString;

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
    entries: Vec<(CString, CString)>,
    index: HashMap<Vec<u8>, usize>,
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

fn c_text(text: &[u8]) -> CString {
    CString::new(until_nul(text)).unwrap_or_default()
}

impl VarTable {
    fn new() -> VarTable {
        VarTable {
            entries: Vec::new(),
            index: HashMap::new(),
            important: None,
        }
    }

    fn set(&mut self, name: &[u8], value: &[u8], important: bool) {
        let key = until_nul(name);
        let entry = (c_text(key), c_text(value));
        match self.index.get(key) {
            Some(&at) => self.entries[at] = entry,
            None => {
                self.index.insert(key.to_vec(), self.entries.len());
                self.entries.push(entry);
            }
        }
        if important {
            self.important
                .get_or_insert_with(HashSet::new)
                .insert(until_nul(name).to_vec());
        } else if let Some(set) = &mut self.important {
            set.remove(lookup_key(name));
        }
    }

    pub(super) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(super) fn entries(
        &self,
    ) -> impl Iterator<Item = (*const c_char, *const c_char, bool)> + '_ {
        self.entries.iter().map(|(name, text)| {
            let important = self
                .important
                .as_ref()
                .is_some_and(|set| set.contains(name.to_bytes()));
            (name.as_ptr(), text.as_ptr(), important)
        })
    }

    pub(super) fn replace_texts(&mut self, mut replace: impl FnMut(&[u8]) -> Option<Vec<u8>>) {
        for (_, text) in &mut self.entries {
            if let Some(replaced) = replace(text.to_bytes()) {
                *text = c_text(&replaced);
            }
        }
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
