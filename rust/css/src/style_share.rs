//! Southstar — sharing computed styles between elements: the per-pass table that maps the key an element's parent, root font size, container context and matched declarations make to the style first computed for it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::hash::{BuildHasherDefault, Hasher};
use std::collections::HashMap;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub(crate) struct WordHasher(u64);

impl Default for WordHasher {
    fn default() -> Self {
        WordHasher(FNV_OFFSET)
    }
}

impl Hasher for WordHasher {
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }

    fn write(&mut self, bytes: &[u8]) {
        let mut words = bytes.chunks_exact(8);
        for word in &mut words {
            let mut w = [0; 8];
            w.copy_from_slice(word);
            self.0 = (self.0 ^ u64::from_ne_bytes(w)).wrapping_mul(FNV_PRIME);
        }
        for &b in words.remainder() {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(FNV_PRIME);
        }
    }
}

#[derive(Default)]
pub(crate) struct ShareTable {
    styles: HashMap<Box<[u8]>, usize, BuildHasherDefault<WordHasher>>,
    key: Vec<u8>,
    active: bool,
}

impl ShareTable {
    pub(crate) fn begin(&mut self) {
        self.styles = HashMap::default();
        self.active = true;
    }

    pub(crate) fn end(&mut self) {
        self.styles = HashMap::default();
        self.active = false;
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    pub(crate) fn new_key(&mut self) -> &mut Vec<u8> {
        self.key.clear();
        &mut self.key
    }

    pub(crate) fn find(&self) -> Option<usize> {
        self.styles.get(self.key.as_slice()).copied()
    }

    pub(crate) fn insert(&mut self, style: usize) {
        if self.active {
            self.styles.insert(self.key.as_slice().into(), style);
        }
    }
}

pub(crate) fn uses_attr(text: &[u8]) -> bool {
    text.windows(5).any(|w| w == b"attr(")
}
