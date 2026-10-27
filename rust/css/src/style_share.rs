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
        let (words, tail) = bytes.as_chunks::<8>();
        for &word in words {
            self.0 = (self.0 ^ u64::from_ne_bytes(word)).wrapping_mul(FNV_PRIME);
        }
        for &b in tail {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(FNV_PRIME);
        }
    }
}

pub(crate) struct ShareTable<T> {
    styles: HashMap<Box<[u8]>, T, BuildHasherDefault<WordHasher>>,
    key: Vec<u8>,
    active: bool,
}

impl<T> Default for ShareTable<T> {
    fn default() -> Self {
        ShareTable {
            styles: HashMap::default(),
            key: Vec::new(),
            active: false,
        }
    }
}

impl<T> ShareTable<T> {
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

    pub(crate) fn find(&self) -> Option<&T> {
        self.styles.get(self.key.as_slice())
    }

    pub(crate) fn insert(&mut self, style: T) {
        if self.active {
            self.styles.insert(self.key.as_slice().into(), style);
        }
    }
}

pub(crate) fn uses_attr(text: &[u8]) -> bool {
    text.windows(5).any(|w| w == b"attr(")
}
