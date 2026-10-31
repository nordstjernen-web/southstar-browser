//! Southstar — the rule index: a sheet's candidate rules and selectors bucketed by id, class, tag and attribute for its gather loop, with the pseudo-element masks recorded on the sheet and its rules on the way.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_uint;
use std::collections::HashMap;

use super::declarations::lookup_key;
use super::selector_view::SelectorRef;
use super::sheet::Sheet;
use crate::rule_index::{self, Key, Table};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Candidate {
    pub(super) rule_idx: c_uint,
    pub(super) selector_idx: c_uint,
}

pub(super) type Buckets = HashMap<Vec<u8>, Vec<Candidate>>;

#[derive(Default)]
pub(super) struct RuleIndex {
    pub(super) by_id: Buckets,
    pub(super) by_class: Buckets,
    pub(super) by_tag: Buckets,
    pub(super) by_attr: Buckets,
    pub(super) universal: Vec<Candidate>,
}

fn push(bucket: &mut Vec<Candidate>, cand: Candidate) {
    if bucket.last() != Some(&cand) {
        bucket.push(cand);
    }
}

pub(super) fn rule_index_build(sheet: &Sheet) -> RuleIndex {
    let mut rules = Vec::new();
    for rule in &sheet.rules {
        let mut selectors = Vec::new();
        for view in rule.selectors.refs() {
            if let Some(pe) = view.map(SelectorRef::pseudo_element).filter(|&pe| pe != 0) {
                sheet.pseudo_mask.set(sheet.pseudo_mask.get() | 1 << pe);
                rule.pe_mask.set(rule.pe_mask.get() | 1 << pe);
            }
            selectors.push(view);
        }
        rules.push(selectors);
    }
    let mut index = RuleIndex::default();
    for (key, rule_idx, selector_idx) in rule_index::plan(&rules) {
        let bucket = match &key {
            Key::Universal => &mut index.universal,
            Key::In(table, name) => {
                let table = match table {
                    Table::Id => &mut index.by_id,
                    Table::Class => &mut index.by_class,
                    Table::Tag => &mut index.by_tag,
                    Table::Attr => &mut index.by_attr,
                };
                table.entry(lookup_key(name).to_vec()).or_default()
            }
        };
        push(
            bucket,
            Candidate {
                rule_idx,
                selector_idx,
            },
        );
    }
    index
}
