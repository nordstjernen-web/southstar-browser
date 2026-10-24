//! Southstar text — font coverage: which characters a font renders, with decomposable characters resolved and remembered.
//! Copyright 2000 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::collections::HashMap;

use crate::types::CoverageLevel;

const PAGE_BITS: u32 = 512;
const PAGE_WORDS: usize = (PAGE_BITS / 64) as usize;
const INVALID_CODEPOINT: u32 = u32::MAX;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CharSet {
    pages: HashMap<u32, [u64; PAGE_WORDS]>,
}

impl CharSet {
    pub fn new() -> CharSet {
        CharSet::default()
    }

    fn locate(ch: u32) -> (u32, usize, u64) {
        let offset = ch % PAGE_BITS;
        (
            ch / PAGE_BITS,
            (offset / 64) as usize,
            1u64 << (offset % 64),
        )
    }

    pub fn contains(&self, ch: u32) -> bool {
        let (page, word, bit) = CharSet::locate(ch);
        self.pages
            .get(&page)
            .is_some_and(|words| words[word] & bit != 0)
    }

    pub fn insert(&mut self, ch: u32) {
        if ch == INVALID_CODEPOINT {
            return;
        }
        let (page, word, bit) = CharSet::locate(ch);
        self.pages.entry(page).or_insert([0; PAGE_WORDS])[word] |= bit;
    }

    pub fn insert_range(&mut self, first: u32, last: u32) {
        for ch in first..=last {
            self.insert(ch);
        }
    }

    pub fn remove(&mut self, ch: u32) {
        let (page, word, bit) = CharSet::locate(ch);
        if let Some(words) = self.pages.get_mut(&page) {
            words[word] &= !bit;
            if words.iter().all(|&w| w == 0) {
                self.pages.remove(&page);
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.pages
            .values()
            .map(|words| words.iter().map(|w| w.count_ones() as usize).sum::<usize>())
            .sum()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Coverage {
    covered: Option<CharSet>,
    not_covered: CharSet,
    remembers_misses: bool,
}

impl Coverage {
    pub fn new() -> Coverage {
        Coverage::default()
    }

    pub fn from_charset(covered: CharSet) -> Coverage {
        Coverage {
            covered: Some(covered),
            not_covered: CharSet::new(),
            remembers_misses: true,
        }
    }

    fn is_covered(&self, ch: u32) -> bool {
        self.covered.as_ref().is_some_and(|set| set.contains(ch))
    }

    fn decomposition_covered(&mut self, ch: u32) -> bool {
        let Some((first, second)) = southstar_unicode::decompose(ch) else {
            return false;
        };
        self.get(first as i32) == CoverageLevel::Exact
            && (second == 0 || self.get(second as i32) == CoverageLevel::Exact)
    }

    pub fn get(&mut self, index: i32) -> CoverageLevel {
        let ch = index as u32;
        if self.remembers_misses {
            if self.is_covered(ch) {
                return CoverageLevel::Exact;
            }
            if self.not_covered.contains(ch) {
                return CoverageLevel::None;
            }
            if self.decomposition_covered(ch) {
                self.covered.get_or_insert_with(CharSet::new).insert(ch);
                return CoverageLevel::Exact;
            }
            self.not_covered.insert(ch);
            return CoverageLevel::None;
        }

        if self.covered.is_none() {
            return CoverageLevel::None;
        }
        if self.is_covered(ch) {
            return CoverageLevel::Exact;
        }
        if self.decomposition_covered(ch) {
            self.set(index, CoverageLevel::Exact);
            return CoverageLevel::Exact;
        }
        self.set(index, CoverageLevel::None);
        CoverageLevel::None
    }

    pub fn set(&mut self, index: i32, level: CoverageLevel) {
        let ch = index as u32;
        if level != CoverageLevel::None {
            self.covered.get_or_insert_with(CharSet::new).insert(ch);
            self.not_covered.remove(ch);
        } else {
            self.not_covered.insert(ch);
            if let Some(covered) = self.covered.as_mut() {
                covered.remove(ch);
            }
        }
    }
}
