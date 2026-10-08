//! Southstar — sharing one fetch among identical requests: the preloads a page announced, the responses kept for them, and the requests waiting on a fetch already in flight.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::{BTreeMap, BTreeSet};

const MAX_EXPECTED: usize = 64;
const MAX_STORED_BYTES: usize = 16 * 1024 * 1024;

pub trait Shareable: Sized {
    fn preloadable_len(&self) -> Option<usize>;
    fn duplicate(&self) -> Self;
}

pub enum Claim<R> {
    Lead,
    Joined,
    Preloaded(R),
}

pub struct Coalescer<R, W> {
    expected: BTreeSet<Vec<u8>>,
    stored: BTreeMap<Vec<u8>, (R, usize)>,
    stored_bytes: usize,
    groups: BTreeMap<Vec<u8>, Vec<W>>,
}

impl<R: Shareable, W> Coalescer<R, W> {
    pub const fn new() -> Self {
        Coalescer {
            expected: BTreeSet::new(),
            stored: BTreeMap::new(),
            stored_bytes: 0,
            groups: BTreeMap::new(),
        }
    }

    pub fn expect(&mut self, key: &[u8]) {
        if self.expected.len() < MAX_EXPECTED {
            self.expected.insert(key.to_vec());
        }
    }

    pub fn clear(&mut self) {
        self.expected.clear();
        self.stored.clear();
        self.stored_bytes = 0;
    }

    fn take_stored(&mut self, key: &[u8]) -> Option<R> {
        let (response, len) = self.stored.remove(key)?;
        self.stored_bytes = self.stored_bytes.saturating_sub(len);
        Some(response)
    }

    fn store(&mut self, key: &[u8], response: &R) {
        if !self.expected.remove(key) {
            return;
        }
        let Some(len) = response.preloadable_len() else {
            return;
        };
        if len > MAX_STORED_BYTES
            || self.stored_bytes + len > MAX_STORED_BYTES
            || self.stored.contains_key(key)
        {
            return;
        }
        self.stored_bytes += len;
        self.stored
            .insert(key.to_vec(), (response.duplicate(), len));
    }

    pub fn claim(&mut self, key: &[u8], joiner: impl FnOnce() -> W) -> Claim<R> {
        if let Some(response) = self.take_stored(key) {
            return Claim::Preloaded(response);
        }
        if let Some(group) = self.groups.get_mut(key) {
            group.push(joiner());
            return Claim::Joined;
        }
        self.groups.insert(key.to_vec(), Vec::new());
        Claim::Lead
    }

    pub fn leave(&mut self, key: &[u8], is_waiter: impl Fn(&W) -> bool) {
        if let Some(group) = self.groups.get_mut(key) {
            if let Some(at) = group.iter().position(is_waiter) {
                group.swap_remove(at);
            }
        }
    }

    pub fn deliver(&mut self, key: &[u8], response: Option<&R>) -> Option<Vec<W>> {
        match response {
            Some(response) => self.store(key, response),
            None => {
                self.expected.remove(key);
            }
        }
        self.groups.remove(key)
    }
}
