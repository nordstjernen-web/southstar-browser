//! Southstar — parsed selectors laid out for the matcher: each selector's compounds, combinators, specificity and ancestor hashes with the strings it reads cut where css.c's copies ended, the owning group a rule, scope or query holds, and the parser state css.c still asks for.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_int, c_uint};
use std::ffi::CString;

use super::declarations::until_nul;
use super::selector_view::{GroupRef, SelectorRef};
use crate::selector::{self, AttrPred, Compound, FLAGS, PseudoPred, Selector};

pub(super) struct SelectorData {
    pub(super) compounds: Vec<CompoundData>,
    pub(super) combinators: Vec<c_uint>,
    pub(super) pseudo_element: c_uint,
    pub(super) spec_a: c_int,
    pub(super) spec_b: c_int,
    pub(super) spec_c: c_int,
    pub(super) ancestor_hashes: [u32; 4],
    pub(super) n_ancestor_hashes: c_uint,
    pub(super) n_ancestor_attr_hashes: c_uint,
}

pub(super) struct CompoundData {
    pub(super) type_: Option<CString>,
    pub(super) id: Option<CString>,
    pub(super) classes: Vec<Vec<u8>>,
    pub(super) first_class: Option<CString>,
    pub(super) attrs: Vec<AttrData>,
    pub(super) pseudos: Vec<PseudoData>,
    pub(super) matches_any: Option<Vec<Vec<SelectorData>>>,
    pub(super) matches_none: Option<Vec<Vec<SelectorData>>>,
    pub(super) has_groups: Option<Vec<Vec<SelectorData>>>,
    pub(super) never_match: bool,
    pub(super) ns_none: bool,
}

pub(crate) struct AttrData {
    pub(super) name: CString,
    pub(super) op: c_uint,
    pub(super) value: Option<Vec<u8>>,
    pub(super) case_insensitive: bool,
    pub(super) case_sensitive: bool,
    pub(super) html_ci: bool,
    pub(super) name_bit: u64,
}

pub(crate) struct PseudoData {
    pub(super) kind: c_uint,
    pub(super) a: c_int,
    pub(super) b: c_int,
    pub(super) arg: Option<Vec<u8>>,
    pub(super) of_group: Option<Vec<SelectorData>>,
}

fn c_text(text: &[u8]) -> CString {
    CString::new(until_nul(text)).unwrap_or_default()
}

fn text(text: Option<&[u8]>) -> Option<Vec<u8>> {
    text.map(|text| until_nul(text).to_vec())
}

fn group(group: &[Selector]) -> Vec<SelectorData> {
    group.iter().map(selector_data).collect()
}

fn groups(groups: Option<&Vec<Vec<Selector>>>) -> Option<Vec<Vec<SelectorData>>> {
    groups.map(|groups| groups.iter().map(|g| group(g)).collect())
}

fn attr_data(pred: &AttrPred) -> AttrData {
    AttrData {
        name: c_text(&pred.name),
        op: pred.op,
        value: text(pred.value.as_deref()),
        case_insensitive: pred.case_insensitive,
        case_sensitive: pred.case_sensitive,
        html_ci: pred.html_ci,
        name_bit: pred.name_bit,
    }
}

fn pseudo_data(pred: &PseudoPred) -> PseudoData {
    PseudoData {
        kind: pred.kind,
        a: pred.a,
        b: pred.b,
        arg: text(pred.arg.as_deref()),
        of_group: pred.of_group.as_deref().map(group),
    }
}

fn compound_data(c: &Compound) -> CompoundData {
    CompoundData {
        type_: c.type_.as_deref().map(c_text),
        id: c.id.as_deref().map(c_text),
        classes: c.classes.clone(),
        first_class: c.classes.first().map(|class| c_text(class)),
        attrs: c.attrs.iter().map(attr_data).collect(),
        pseudos: c.pseudos.iter().map(pseudo_data).collect(),
        matches_any: groups(c.matches_any.as_ref()),
        matches_none: groups(c.matches_none.as_ref()),
        has_groups: groups(c.has_groups.as_ref()),
        never_match: c.never_match || c.inert,
        ns_none: c.ns_none,
    }
}

fn selector_data(sel: &Selector) -> SelectorData {
    let mut ancestor_hashes = [0u32; 4];
    ancestor_hashes[..sel.ancestor_hashes.len()].copy_from_slice(&sel.ancestor_hashes);
    SelectorData {
        compounds: sel.compounds.iter().map(compound_data).collect(),
        combinators: sel.combinators.clone(),
        pseudo_element: sel.pseudo_element,
        spec_a: sel.spec[0],
        spec_b: sel.spec[1],
        spec_c: sel.spec[2],
        ancestor_hashes,
        n_ancestor_hashes: sel.ancestor_hashes.len() as c_uint,
        n_ancestor_attr_hashes: sel.n_ancestor_attr_hashes,
    }
}

pub(super) struct SelectorGroup(Vec<SelectorData>);

impl SelectorGroup {
    pub(super) fn new(selectors: &[Selector]) -> SelectorGroup {
        SelectorGroup(group(selectors))
    }

    pub(super) fn push(&mut self, sel: &Selector) {
        self.0.push(selector_data(sel));
    }

    pub(super) fn group(&self) -> &Vec<SelectorData> {
        &self.0
    }

    pub(super) fn selectors(&self) -> &[SelectorData] {
        &self.0
    }

    pub(super) fn refs(&self) -> impl Iterator<Item = Option<SelectorRef<'_>>> {
        GroupRef::new(&self.0).selectors()
    }
}

pub(super) fn attr_ancestor_hashes() -> bool {
    selector::get(&FLAGS.attr_ancestor_hashes)
}
