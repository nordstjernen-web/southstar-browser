//! Southstar — gathering an element's matching rules: the ancestor Bloom filter that rejects selectors early, the per-rule record of the most specific matching selector and its scope proximity for each pseudo-element, and the class tokens an element is indexed by.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub(crate) const FILTER_SIZE: usize = 4096;
pub(crate) const DESTS: usize = 10;

pub(crate) struct AncestorFilter {
    pub(crate) counts: [u8; FILTER_SIZE],
    pub(crate) active: bool,
    pub(crate) attrs: bool,
    pub(crate) subject: usize,
}

impl AncestorFilter {
    pub(crate) const fn new() -> AncestorFilter {
        AncestorFilter {
            counts: [0; FILTER_SIZE],
            active: false,
            attrs: false,
            subject: 0,
        }
    }

    fn slots(hash: u32) -> [usize; 2] {
        [
            hash as usize % FILTER_SIZE,
            (hash >> 12) as usize % FILTER_SIZE,
        ]
    }

    pub(crate) fn count(&mut self, hash: u32, delta: i32) {
        for slot in Self::slots(hash) {
            let counter = &mut self.counts[slot];
            if *counter == u8::MAX {
                continue;
            }
            if delta > 0 {
                *counter += 1;
            } else if *counter > 0 {
                *counter -= 1;
            }
        }
    }

    pub(crate) fn rejects(&self, hashes: &[u32], attr_hashes: usize) -> bool {
        let first = if self.attrs { 0 } else { attr_hashes };
        hashes
            .iter()
            .skip(first)
            .any(|&hash| Self::slots(hash).iter().any(|&slot| self.counts[slot] == 0))
    }
}

pub(crate) fn is_class_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c)
}

pub(crate) fn class_tokens(value: &[u8]) -> impl Iterator<Item = &[u8]> {
    value
        .split(|&c| is_class_space(c))
        .filter(|t| !t.is_empty())
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Accum {
    pub(crate) epoch: u32,
    pub(crate) layer_order: Option<i32>,
    pub(crate) any: [bool; DESTS],
    pub(crate) specificity: [(i32, i32, i32); DESTS],
    pub(crate) scope_order: [i32; DESTS],
}

impl Accum {
    pub(crate) fn note(&mut self, dest: usize, specificity: (i32, i32, i32), scope_order: i32) {
        if !self.any[dest] || specificity > self.specificity[dest] {
            self.any[dest] = true;
            self.specificity[dest] = specificity;
            self.scope_order[dest] = scope_order;
        } else if specificity == self.specificity[dest] && scope_order > self.scope_order[dest] {
            self.scope_order[dest] = scope_order;
        }
    }
}
