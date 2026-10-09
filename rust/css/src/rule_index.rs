//! Southstar — the rule index of a style sheet: each selector filed under the rarest id, class, tag or attribute name its subject compound requires, so matching an element only tries the rules that could apply.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use crate::ffi::{CompoundRef, SelectorRef};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Table {
    Id,
    Class,
    Tag,
    Attr,
}

pub(crate) enum Key {
    In(Table, Vec<u8>),
    Universal,
}

#[derive(Default)]
struct Counts {
    by_id: HashMap<Vec<u8>, u32>,
    by_class: HashMap<Vec<u8>, u32>,
    by_tag: HashMap<Vec<u8>, u32>,
    by_attr: HashMap<Vec<u8>, u32>,
}

fn until_nul(text: &[u8]) -> &[u8] {
    &text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())]
}

fn subject_type(subject: CompoundRef<'_>) -> Option<&[u8]> {
    subject.type_name().filter(|t| !t.is_empty() && *t != b"*")
}

fn classes(subject: CompoundRef<'_>) -> impl Iterator<Item = &[u8]> {
    subject.classes().map(until_nul)
}

fn attr_names(subject: CompoundRef<'_>) -> impl Iterator<Item = Option<&[u8]>> {
    subject
        .attrs()
        .iter()
        .map(|a| a.name().map(|n| n.to_bytes()).filter(|n| !n.is_empty()))
}

fn bump(table: &mut HashMap<Vec<u8>, u32>, key: Vec<u8>) {
    *table.entry(key).or_insert(0) += 1;
}

impl Counts {
    fn add(&mut self, subject: CompoundRef<'_>) {
        if subject.never_match() {
            return;
        }
        if let Some(id) = subject.id().filter(|id| !id.is_empty()) {
            bump(&mut self.by_id, id.to_vec());
        }
        for class in classes(subject).filter(|c| !c.is_empty()) {
            bump(&mut self.by_class, class.to_vec());
        }
        if let Some(ty) = subject_type(subject) {
            bump(&mut self.by_tag, ty.to_ascii_lowercase());
        }
        for name in attr_names(subject).flatten() {
            bump(&mut self.by_attr, name.to_ascii_lowercase());
        }
    }

    fn key_for(&self, subject: CompoundRef<'_>) -> Key {
        let mut best = u32::MAX;
        let mut chosen = Key::Universal;
        let mut take = |count: Option<&u32>, key: &dyn Fn() -> Key| {
            let count = count.copied().unwrap_or(0);
            if count != 0 && count < best {
                best = count;
                chosen = key();
            }
        };
        if let Some(id) = subject.id().filter(|id| !id.is_empty()) {
            take(self.by_id.get(id), &|| Key::In(Table::Id, id.to_vec()));
        }
        for class in classes(subject).filter(|c| !c.is_empty()) {
            take(self.by_class.get(class), &|| {
                Key::In(Table::Class, class.to_vec())
            });
        }
        if let Some(ty) = subject_type(subject) {
            let lower = ty.to_ascii_lowercase();
            take(self.by_tag.get(&lower), &|| {
                Key::In(Table::Tag, lower.clone())
            });
        }
        for name in attr_names(subject).flatten() {
            let lower = name.to_ascii_lowercase();
            take(self.by_attr.get(&lower), &|| {
                Key::In(Table::Attr, lower.clone())
            });
        }
        chosen
    }
}

fn subject(sel: SelectorRef<'_>) -> Option<CompoundRef<'_>> {
    sel.len().checked_sub(1).and_then(|i| sel.compound(i))
}

pub(crate) fn plan(rules: &[Vec<Option<SelectorRef<'_>>>]) -> Vec<(Key, u32, u32)> {
    let mut counts = Counts::default();
    for sel in rules.iter().flatten().flatten() {
        if let Some(subject) = subject(*sel) {
            counts.add(subject);
        }
    }
    let mut out = Vec::new();
    for (ri, selectors) in rules.iter().enumerate() {
        for (si, sel) in selectors.iter().enumerate() {
            let at = (ri as u32, si as u32);
            let Some(sel) = sel.filter(|s| s.len() > 0) else {
                out.push((Key::Universal, at.0, at.1));
                continue;
            };
            let Some(subject) = subject(sel).filter(|s| !s.never_match()) else {
                continue;
            };
            out.push((counts.key_for(subject), at.0, at.1));
        }
    }
    out
}
