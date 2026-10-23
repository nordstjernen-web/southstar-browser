//! Southstar text — the process-wide cache of default break results, keyed on the paragraph's bytes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::types::LogAttr;
use crate::utils::caches_enabled;
use std::collections::HashMap;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, RwLock};

const MAX_ENTRIES: usize = 8000;
const MAX_TEXT: usize = 64 * 1024;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const SHARDS: usize = 16;
const ENTRY_OVERHEAD: usize = 48;

struct Entry {
    attrs: Box<[LogAttr]>,
    read: AtomicBool,
}

impl Entry {
    fn size(&self, text_length: usize) -> usize {
        ENTRY_OVERHEAD + text_length + self.attrs.len() * size_of::<LogAttr>()
    }
}

#[derive(Default)]
struct Shard {
    table: HashMap<Box<[u8]>, Entry>,
    bytes: usize,
}

static SHARD_TABLE: LazyLock<[RwLock<Shard>; SHARDS]> =
    LazyLock::new(|| std::array::from_fn(|_| RwLock::new(Shard::default())));

static HITS: AtomicU64 = AtomicU64::new(0);
static MISSES: AtomicU64 = AtomicU64::new(0);
static SKIPS: AtomicU64 = AtomicU64::new(0);

fn mix_hash(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h
}

fn hash_text(text: &[u8]) -> Option<u32> {
    let mut hash: u32 = 5381;
    for &byte in text {
        if byte == 0 {
            return None;
        }
        hash = hash.wrapping_mul(33).wrapping_add(u32::from(byte));
    }
    let length = u32::try_from(text.len()).ok()?;
    Some(mix_hash(hash.wrapping_mul(33).wrapping_add(length)))
}

fn shard_for(text: &str) -> Option<&'static RwLock<Shard>> {
    if !caches_enabled() || text.is_empty() || text.len() > MAX_TEXT {
        return None;
    }
    let hash = hash_text(text.as_bytes())?;
    let index = ((hash >> 27) as usize) & (SHARDS - 1);
    Some(&SHARD_TABLE[index])
}

pub(super) fn fill(text: &str, attrs: &mut [LogAttr]) -> bool {
    let Some(shard) = shard_for(text) else {
        SKIPS.fetch_add(1, Ordering::Relaxed);
        return false;
    };

    let guard = shard
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(entry) = guard.table.get(text.as_bytes()) else {
        drop(guard);
        MISSES.fetch_add(1, Ordering::Relaxed);
        return false;
    };
    if entry.attrs.len() > attrs.len() {
        drop(guard);
        MISSES.fetch_add(1, Ordering::Relaxed);
        return false;
    }

    entry.read.store(true, Ordering::Relaxed);
    attrs[..entry.attrs.len()].copy_from_slice(&entry.attrs);
    drop(guard);

    HITS.fetch_add(1, Ordering::Relaxed);
    true
}

fn drop_unread_entries(shard: &mut Shard) {
    let mut freed = 0;
    shard.table.retain(|text, entry| {
        if entry.read.swap(false, Ordering::Relaxed) {
            true
        } else {
            freed += entry.size(text.len());
            false
        }
    });
    shard.bytes -= freed.min(shard.bytes);
}

fn make_room(shard: &mut Shard) {
    if shard.table.len() < MAX_ENTRIES / SHARDS && shard.bytes < MAX_BYTES / SHARDS {
        return;
    }

    drop_unread_entries(shard);

    if shard.table.len() >= MAX_ENTRIES / SHARDS / 2 || shard.bytes >= MAX_BYTES / SHARDS / 2 {
        shard.table.clear();
        shard.bytes = 0;
    }
}

pub(super) fn store(text: &str, attrs: &[LogAttr]) {
    let Some(shard) = shard_for(text) else {
        return;
    };

    let n_attrs = (text.chars().count() + 1).min(attrs.len());
    let entry = Entry {
        attrs: attrs[..n_attrs].into(),
        read: AtomicBool::new(false),
    };
    let size = entry.size(text.len());

    let mut guard = shard
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    make_room(&mut guard);

    if let Some(old) = guard.table.get(text.as_bytes()) {
        let old_size = old.size(text.len());
        guard.bytes -= old_size.min(guard.bytes);
    }
    guard.bytes += size;
    guard.table.insert(text.as_bytes().into(), entry);
}

pub fn break_cache_stats() -> (u64, u64, u64, u64) {
    let entries = SHARD_TABLE
        .iter()
        .map(|shard| {
            shard
                .read()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .table
                .len() as u64
        })
        .sum();
    (
        HITS.load(Ordering::Relaxed),
        MISSES.load(Ordering::Relaxed),
        SKIPS.load(Ordering::Relaxed),
        entries,
    )
}
