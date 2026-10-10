//! Southstar — read-only views of css.c's ns_css_selector, ns_css_simple, predicate, group, rule and scope structs, so the matcher reads them without touching pointers.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_uint, c_void};
use core::slice;

use southstar_glib::{GArray, GPtrArray};

use super::declarations::RawRule;
use super::selector::{RawAttrPred, RawPseudoPred, RawSelector, RawSimple};
use super::sheet::{RawScope, RawSheet};

unsafe fn pointers<'a, T>(array: *const GPtrArray) -> &'a [*mut T] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.pdata.is_null() && a.len > 0 => unsafe {
            slice::from_raw_parts(a.pdata.cast::<*mut T>(), a.len as usize)
        },
        _ => &[],
    }
}

unsafe fn elements<'a, T>(array: *const GArray) -> &'a [T] {
    match unsafe { array.as_ref() } {
        Some(a) if !a.data.is_null() && a.len > 0 => unsafe {
            slice::from_raw_parts(a.data.cast::<T>(), a.len as usize)
        },
        _ => &[],
    }
}

unsafe fn text<'a>(s: *const c_char) -> Option<&'a CStr> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) })
}

#[derive(Clone, Copy)]
pub(crate) struct SelectorRef<'a>(&'a RawSelector);

#[derive(Clone, Copy)]
pub(crate) struct CompoundRef<'a>(&'a RawSimple);

#[derive(Clone, Copy)]
pub(crate) struct GroupRef<'a>(&'a GPtrArray);

#[derive(Clone, Copy)]
pub(crate) struct RuleRef<'a>(&'a RawRule);

#[derive(Clone, Copy)]
pub(crate) struct ScopeRef<'a>(&'a RawScope);

#[derive(Clone, Copy)]
pub(crate) struct SheetRef<'a>(&'a RawSheet);

#[repr(transparent)]
pub(crate) struct AttrRef(RawAttrPred);

#[repr(transparent)]
pub(crate) struct PseudoRef(RawPseudoPred);

impl<'a> SelectorRef<'a> {
    pub(super) unsafe fn from_ptr(sel: *const c_void) -> Option<Self> {
        unsafe { sel.cast::<RawSelector>().as_ref() }.map(SelectorRef)
    }

    pub(crate) fn pseudo_element(self) -> u32 {
        self.0.pseudo_element
    }

    pub(crate) fn len(self) -> usize {
        unsafe { pointers::<RawSimple>(self.0.compounds) }.len()
    }

    pub(crate) fn compound(self, i: usize) -> Option<CompoundRef<'a>> {
        let compounds = unsafe { pointers::<RawSimple>(self.0.compounds) };
        compounds
            .get(i)
            .and_then(|&c| unsafe { c.as_ref() })
            .map(CompoundRef)
    }

    pub(crate) fn combinator(self, i: usize) -> u32 {
        unsafe { elements::<c_uint>(self.0.combinators) }
            .get(i)
            .copied()
            .unwrap_or(0)
    }
}

impl<'a> CompoundRef<'a> {
    pub(crate) fn never_match(self) -> bool {
        self.0.never_match != 0
    }

    pub(crate) fn ns_none(self) -> bool {
        self.0.ns_none != 0
    }

    pub(crate) fn type_name(self) -> Option<&'a [u8]> {
        unsafe { text(self.0.type_) }.map(CStr::to_bytes)
    }

    pub(crate) fn id(self) -> Option<&'a [u8]> {
        unsafe { text(self.0.id) }.map(CStr::to_bytes)
    }

    pub(crate) fn id_text(self) -> Option<&'a CStr> {
        unsafe { text(self.0.id) }
    }

    pub(crate) fn type_text(self) -> Option<&'a CStr> {
        unsafe { text(self.0.type_) }
    }

    pub(crate) fn first_class_text(self) -> Option<&'a CStr> {
        unsafe { pointers::<c_char>(self.0.classes) }
            .first()
            .and_then(|&name| unsafe { text(name) })
    }

    pub(crate) fn classes(self) -> impl Iterator<Item = &'a [u8]> {
        let names = unsafe { pointers::<c_char>(self.0.classes) };
        let lens = unsafe { elements::<usize>(self.0.class_lens) };
        names
            .iter()
            .enumerate()
            .map(move |(i, &name)| match lens.get(i) {
                Some(&len) => unsafe { slice::from_raw_parts(name.cast::<u8>(), len) },
                None => unsafe { CStr::from_ptr(name) }.to_bytes(),
            })
    }

    pub(crate) fn attrs(self) -> &'a [AttrRef] {
        unsafe { elements::<AttrRef>(self.0.attrs) }
    }

    pub(crate) fn pseudos(self) -> &'a [PseudoRef] {
        unsafe { elements::<PseudoRef>(self.0.pseudos) }
    }

    fn groups(array: *const GPtrArray) -> impl Iterator<Item = GroupRef<'a>> {
        unsafe { pointers::<GPtrArray>(array) }
            .iter()
            .filter_map(|&g| unsafe { g.as_ref() }.map(GroupRef))
    }

    pub(crate) fn matches_any(self) -> impl Iterator<Item = GroupRef<'a>> {
        Self::groups(self.0.matches_any)
    }

    pub(crate) fn matches_none(self) -> impl Iterator<Item = GroupRef<'a>> {
        Self::groups(self.0.matches_none)
    }

    pub(crate) fn has_groups(self) -> impl Iterator<Item = GroupRef<'a>> {
        Self::groups(self.0.has_groups)
    }

    pub(crate) fn has_group_count(self) -> usize {
        unsafe { pointers::<GPtrArray>(self.0.has_groups) }.len()
    }

    pub(crate) fn has_group_arrays(self) -> bool {
        !self.0.matches_any.is_null()
            || !self.0.matches_none.is_null()
            || !self.0.has_groups.is_null()
    }
}

impl<'a> GroupRef<'a> {
    pub(crate) fn addr(self) -> usize {
        core::ptr::from_ref(self.0) as usize
    }

    pub(crate) fn selectors(self) -> impl Iterator<Item = Option<SelectorRef<'a>>> {
        unsafe { pointers::<RawSelector>(self.0) }
            .iter()
            .map(|&s| unsafe { s.as_ref() }.map(SelectorRef))
    }
}

impl AttrRef {
    pub(crate) fn name(&self) -> Option<&CStr> {
        unsafe { text(self.0.name) }
    }

    pub(crate) fn op(&self) -> u32 {
        self.0.op
    }

    pub(crate) fn value(&self) -> Option<&[u8]> {
        unsafe { text(self.0.value) }.map(CStr::to_bytes)
    }

    pub(crate) fn case_insensitive(&self) -> bool {
        self.0.case_insensitive != 0
    }

    pub(crate) fn case_sensitive(&self) -> bool {
        self.0.case_sensitive != 0
    }

    pub(crate) fn html_ci(&self) -> bool {
        self.0.html_ci != 0
    }

    pub(crate) fn name_bit(&self) -> u64 {
        self.0.name_bit
    }
}

impl PseudoRef {
    pub(crate) fn kind(&self) -> u32 {
        self.0.kind
    }

    pub(crate) fn a(&self) -> i32 {
        self.0.a
    }

    pub(crate) fn b(&self) -> i32 {
        self.0.b
    }

    pub(crate) fn arg(&self) -> Option<&[u8]> {
        unsafe { text(self.0.arg) }.map(CStr::to_bytes)
    }

    pub(crate) fn of_group(&self) -> Option<GroupRef<'_>> {
        unsafe { self.0.of_group.as_ref() }.map(GroupRef)
    }
}

impl<'a> RuleRef<'a> {
    pub(super) unsafe fn from_ptr(rule: *const c_void) -> Option<Self> {
        unsafe { rule.cast::<RawRule>().as_ref() }.map(RuleRef)
    }

    pub(crate) fn selector_group(self) -> Option<GroupRef<'a>> {
        unsafe { self.0.selectors.as_ref() }.map(GroupRef)
    }

    pub(crate) fn selectors(self) -> impl Iterator<Item = Option<SelectorRef<'a>>> {
        self.selector_group()
            .into_iter()
            .flat_map(GroupRef::selectors)
    }

    pub(crate) fn scopes(self) -> impl Iterator<Item = ScopeRef<'a>> {
        unsafe { pointers::<RawScope>(self.0.scopes) }
            .iter()
            .filter_map(|&s| unsafe { s.as_ref() }.map(ScopeRef))
    }
}

impl<'a> ScopeRef<'a> {
    pub(crate) fn roots(self) -> Option<GroupRef<'a>> {
        unsafe { self.0.roots.as_ref() }.map(GroupRef)
    }

    pub(crate) fn limits(self) -> Option<GroupRef<'a>> {
        unsafe { self.0.limits.as_ref() }.map(GroupRef)
    }
}

impl<'a> SheetRef<'a> {
    pub(super) unsafe fn from_ptr(sheet: *const c_void) -> Option<Self> {
        unsafe { sheet.cast::<RawSheet>().as_ref() }.map(SheetRef)
    }

    pub(super) unsafe fn list(
        ua: *const c_void,
        author: *const *const c_void,
        n_author: usize,
    ) -> Vec<SheetRef<'a>> {
        let author: &[*const c_void] = if author.is_null() || n_author == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(author, n_author) }
        };
        core::iter::once(ua)
            .chain(author.iter().copied())
            .filter_map(|sheet| unsafe { SheetRef::from_ptr(sheet) })
            .collect()
    }

    pub(crate) fn layer_names(self) -> impl Iterator<Item = &'a [u8]> {
        unsafe { pointers::<c_char>(self.0.layer_names) }
            .iter()
            .filter_map(|&name| unsafe { text(name) }.map(CStr::to_bytes))
    }

    pub(crate) fn rules(self) -> impl Iterator<Item = RuleRef<'a>> {
        unsafe { pointers::<RawRule>(self.0.rules) }
            .iter()
            .filter_map(|&r| unsafe { r.as_ref() }.map(RuleRef))
    }
}
