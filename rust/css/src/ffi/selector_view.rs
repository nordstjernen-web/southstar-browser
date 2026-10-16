//! Southstar — read-only views of parsed selectors, their compounds, predicates and groups, and of a sheet's rules and scopes, the shapes the matcher, the rule index and restyle invalidation read.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use super::declarations::Rule;
pub(crate) use super::selector::{AttrData as AttrRef, PseudoData as PseudoRef};
use super::selector::{CompoundData, SelectorData};
use super::sheet::{Scope, SheetData};

#[derive(Clone, Copy)]
pub(crate) struct SelectorRef<'a>(&'a SelectorData);

#[derive(Clone, Copy)]
pub(crate) struct CompoundRef<'a>(&'a CompoundData);

#[derive(Clone, Copy)]
pub(crate) struct GroupRef<'a>(&'a Vec<SelectorData>);

#[derive(Clone, Copy)]
pub(crate) struct RuleRef<'a>(&'a Rule);

#[derive(Clone, Copy)]
pub(crate) struct ScopeRef<'a>(&'a Scope);

#[derive(Clone, Copy)]
pub(crate) struct SheetRef<'a>(&'a SheetData);

impl<'a> SelectorRef<'a> {
    pub(super) fn new(sel: &'a SelectorData) -> Self {
        SelectorRef(sel)
    }

    pub(crate) fn pseudo_element(self) -> u32 {
        self.0.pseudo_element
    }

    pub(crate) fn len(self) -> usize {
        self.0.compounds.len()
    }

    pub(crate) fn compound(self, i: usize) -> Option<CompoundRef<'a>> {
        self.0.compounds.get(i).map(CompoundRef)
    }

    pub(crate) fn combinator(self, i: usize) -> u32 {
        self.0.combinators.get(i).copied().unwrap_or(0)
    }
}

impl<'a> CompoundRef<'a> {
    pub(crate) fn never_match(self) -> bool {
        self.0.never_match
    }

    pub(crate) fn ns_none(self) -> bool {
        self.0.ns_none
    }

    pub(crate) fn type_name(self) -> Option<&'a [u8]> {
        self.0.type_.as_deref().map(CStr::to_bytes)
    }

    pub(crate) fn id(self) -> Option<&'a [u8]> {
        self.0.id.as_deref().map(CStr::to_bytes)
    }

    pub(crate) fn id_text(self) -> Option<&'a CStr> {
        self.0.id.as_deref()
    }

    pub(crate) fn type_text(self) -> Option<&'a CStr> {
        self.0.type_.as_deref()
    }

    pub(crate) fn first_class_text(self) -> Option<&'a CStr> {
        self.0.first_class.as_deref()
    }

    pub(crate) fn classes(self) -> impl Iterator<Item = &'a [u8]> {
        self.0.classes.iter().map(Vec::as_slice)
    }

    pub(crate) fn attrs(self) -> &'a [AttrRef] {
        &self.0.attrs
    }

    pub(crate) fn pseudos(self) -> &'a [PseudoRef] {
        &self.0.pseudos
    }

    fn groups(groups: Option<&'a Vec<Vec<SelectorData>>>) -> impl Iterator<Item = GroupRef<'a>> {
        groups.into_iter().flatten().map(GroupRef)
    }

    pub(crate) fn matches_any(self) -> impl Iterator<Item = GroupRef<'a>> {
        Self::groups(self.0.matches_any.as_ref())
    }

    pub(crate) fn matches_none(self) -> impl Iterator<Item = GroupRef<'a>> {
        Self::groups(self.0.matches_none.as_ref())
    }

    pub(crate) fn has_groups(self) -> impl Iterator<Item = GroupRef<'a>> {
        Self::groups(self.0.has_groups.as_ref())
    }

    pub(crate) fn has_group_count(self) -> usize {
        self.0.has_groups.as_ref().map_or(0, Vec::len)
    }

    pub(crate) fn has_group_arrays(self) -> bool {
        self.0.matches_any.is_some() || self.0.matches_none.is_some() || self.0.has_groups.is_some()
    }
}

impl<'a> GroupRef<'a> {
    pub(super) fn new(group: &'a Vec<SelectorData>) -> Self {
        GroupRef(group)
    }

    pub(crate) fn addr(self) -> usize {
        core::ptr::from_ref(self.0) as usize
    }

    pub(crate) fn selectors(self) -> impl Iterator<Item = Option<SelectorRef<'a>>> {
        self.0.iter().map(|sel| Some(SelectorRef(sel)))
    }
}

impl AttrRef {
    pub(crate) fn name(&self) -> Option<&CStr> {
        Some(&self.name)
    }

    pub(crate) fn op(&self) -> u32 {
        self.op
    }

    pub(crate) fn value(&self) -> Option<&[u8]> {
        self.value.as_deref()
    }

    pub(crate) fn case_insensitive(&self) -> bool {
        self.case_insensitive
    }

    pub(crate) fn case_sensitive(&self) -> bool {
        self.case_sensitive
    }

    pub(crate) fn html_ci(&self) -> bool {
        self.html_ci
    }

    pub(crate) fn name_bit(&self) -> u64 {
        self.name_bit
    }
}

impl PseudoRef {
    pub(crate) fn kind(&self) -> u32 {
        self.kind
    }

    pub(crate) fn a(&self) -> i32 {
        self.a
    }

    pub(crate) fn b(&self) -> i32 {
        self.b
    }

    pub(crate) fn arg(&self) -> Option<&[u8]> {
        self.arg.as_deref()
    }

    pub(crate) fn of_group(&self) -> Option<GroupRef<'_>> {
        self.of_group.as_ref().map(GroupRef)
    }
}

impl<'a> RuleRef<'a> {
    pub(super) fn new(rule: &'a Rule) -> Self {
        RuleRef(rule)
    }

    pub(crate) fn selector_group(self) -> Option<GroupRef<'a>> {
        Some(GroupRef(self.0.selectors.group()))
    }

    pub(crate) fn selectors(self) -> impl Iterator<Item = Option<SelectorRef<'a>>> {
        self.selector_group()
            .into_iter()
            .flat_map(GroupRef::selectors)
    }

    pub(crate) fn scopes(self) -> impl Iterator<Item = ScopeRef<'a>> {
        self.0.scopes.iter().map(ScopeRef)
    }
}

impl<'a> ScopeRef<'a> {
    pub(crate) fn roots(self) -> Option<GroupRef<'a>> {
        Some(GroupRef(self.0.roots.group()))
    }

    pub(crate) fn limits(self) -> Option<GroupRef<'a>> {
        self.0
            .limits
            .as_ref()
            .map(|limits| GroupRef(limits.group()))
    }
}

impl<'a> SheetRef<'a> {
    pub(super) fn new(sheet: &'a SheetData) -> Self {
        SheetRef(sheet)
    }

    pub(super) fn raw(self) -> &'a SheetData {
        self.0
    }

    pub(crate) fn layer_names(self) -> impl Iterator<Item = &'a [u8]> {
        self.0.layer_names.iter().map(Vec::as_slice)
    }

    pub(crate) fn rules(self) -> impl Iterator<Item = RuleRef<'a>> {
        self.0.rules.iter().map(RuleRef)
    }
}
