//! Southstar text — the cross-layout cache of itemised paragraphs, keyed on the text, its attributes, its offset and the context's state.
//! Copyright 2026 Northstar contributors; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::attrs::{AttrList, Attribute};
use crate::item::Item;
use crate::types::Direction;
use crate::utf8::g_utf8_strlen;
use crate::utils::caches_enabled;

const MAX_ENTRIES: usize = 4000;
const MAX_TEXT: usize = 64 * 1024;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const SHARDS: usize = 16;
const ITEM_OVERHEAD_ESTIMATE: usize = core::mem::size_of::<Item>() + 2 * 24 + 64;
const ATTR_OVERHEAD_ESTIMATE: usize =
    core::mem::size_of::<Attribute>() + core::mem::size_of::<usize>() + 64;

pub(crate) fn mix_hash(mut h: u32) -> u32 {
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h
}

pub(crate) const fn shard_of(hash: u32) -> usize {
    ((hash >> 27) as usize) & (SHARDS - 1)
}

struct ItemProbe<'a> {
    hash: u32,
    state_id: u64,
    base_dir: Direction,
    start_index: usize,
    text: &'a [u8],
    attrs: Option<&'a [Attribute]>,
}

struct ItemEntry {
    hash: u32,
    state_id: u64,
    base_dir: Direction,
    start_index: usize,
    text: Box<[u8]>,
    attrs: Option<Vec<Attribute>>,
    items: Vec<Item>,
    bytes: usize,
    read: AtomicBool,
}

#[derive(Default)]
struct ItemShard {
    table: HashMap<u32, Vec<ItemEntry>>,
    entries: usize,
    bytes: usize,
}

static ITEM_SHARDS: [RwLock<Option<ItemShard>>; SHARDS] = [const { RwLock::new(None) }; SHARDS];
static ITEM_HITS: AtomicU32 = AtomicU32::new(0);
static ITEM_MISSES: AtomicU32 = AtomicU32::new(0);
static ITEM_SKIPS: AtomicU32 = AtomicU32::new(0);

fn read_shard(index: usize) -> RwLockReadGuard<'static, Option<ItemShard>> {
    ITEM_SHARDS[index]
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_shard(index: usize) -> RwLockWriteGuard<'static, Option<ItemShard>> {
    ITEM_SHARDS[index]
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn attr_lists_equal_in_order(a: Option<&[Attribute]>, b: Option<&[Attribute]>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => {
            a.len() == b.len()
                && a.iter().zip(b).all(|(x, y)| {
                    x.start_index == y.start_index && x.end_index == y.end_index && x.equal(y)
                })
        }
        _ => false,
    }
}

fn hash_attrs(mut hash: u32, attrs: Option<&[Attribute]>) -> u32 {
    let Some(attrs) = attrs else {
        return hash;
    };
    hash = hash.wrapping_mul(33).wrapping_add(attrs.len() as u32);
    for attr in attrs {
        hash = hash.wrapping_mul(33).wrapping_add(attr.kind() as u32);
        hash = hash.wrapping_mul(33).wrapping_add(attr.start_index);
        hash = hash.wrapping_mul(33).wrapping_add(attr.end_index);
    }
    hash
}

impl ItemEntry {
    fn matches(&self, probe: &ItemProbe<'_>) -> bool {
        self.hash == probe.hash
            && self.state_id == probe.state_id
            && self.base_dir == probe.base_dir
            && self.start_index == probe.start_index
            && *self.text == *probe.text
            && attr_lists_equal_in_order(self.attrs.as_deref(), probe.attrs)
    }

    fn size(&self) -> usize {
        core::mem::size_of::<ItemEntry>()
            + self.text.len()
            + self.items.len() * ITEM_OVERHEAD_ESTIMATE
            + self
                .attrs
                .as_ref()
                .map_or(0, |attrs| attrs.len() * ATTR_OVERHEAD_ESTIMATE)
    }
}

fn probe_init<'a>(
    state_id: u64,
    base_dir: Direction,
    text: &'a str,
    start_index: usize,
    length: usize,
    attrs: Option<&'a AttrList>,
) -> Option<ItemProbe<'a>> {
    if !caches_enabled() || length == 0 || length > MAX_TEXT {
        return None;
    }
    let slice = text.as_bytes().get(start_index..start_index + length)?;
    let mut hash: u32 = 5381;
    for &byte in slice {
        hash = hash.wrapping_mul(33).wrapping_add(u32::from(byte));
    }
    hash = hash.wrapping_mul(33).wrapping_add(state_id as u32);
    hash = hash.wrapping_mul(33).wrapping_add(base_dir as u32);
    hash = hash.wrapping_mul(33).wrapping_add(start_index as u32);
    hash = hash.wrapping_mul(33).wrapping_add(length as u32);
    let attrs = attrs.map(AttrList::as_slice);
    Some(ItemProbe {
        hash: mix_hash(hash_attrs(hash, attrs)),
        state_id,
        base_dir,
        start_index,
        text: slice,
        attrs,
    })
}

fn copy_items(items: &[Item], byte_shift: i32, char_shift: i32) -> Vec<Item> {
    items
        .iter()
        .map(|item| {
            let mut copy = item.clone();
            copy.offset += byte_shift;
            copy.char_offset += char_shift;
            copy
        })
        .collect()
}

fn char_offset_of(text: &str, start_index: usize) -> i32 {
    g_utf8_strlen(text.as_bytes(), start_index)
}

pub(crate) fn lookup(
    state_id: u64,
    base_dir: Direction,
    text: &str,
    start_index: usize,
    length: usize,
    attrs: Option<&AttrList>,
) -> Option<Vec<Item>> {
    let Some(probe) = probe_init(state_id, base_dir, text, start_index, length, attrs) else {
        ITEM_SKIPS.fetch_add(1, Ordering::Relaxed);
        return None;
    };

    let guard = read_shard(shard_of(probe.hash));
    let entry = guard
        .as_ref()
        .and_then(|shard| shard.table.get(&probe.hash))
        .and_then(|bucket| bucket.iter().find(|entry| entry.matches(&probe)));
    let Some(entry) = entry else {
        drop(guard);
        ITEM_MISSES.fetch_add(1, Ordering::Relaxed);
        return None;
    };

    entry.read.store(true, Ordering::Relaxed);
    let items = copy_items(
        &entry.items,
        start_index as i32,
        char_offset_of(text, start_index),
    );
    drop(guard);

    ITEM_HITS.fetch_add(1, Ordering::Relaxed);
    Some(items)
}

fn drop_unread_entries(shard: &mut ItemShard) {
    let mut freed = 0usize;
    let mut removed = 0usize;
    for bucket in shard.table.values_mut() {
        bucket.retain(|entry| {
            if entry.read.swap(false, Ordering::Relaxed) {
                true
            } else {
                freed += entry.bytes;
                removed += 1;
                false
            }
        });
    }
    shard.table.retain(|_, bucket| !bucket.is_empty());
    shard.entries -= removed;
    shard.bytes -= freed.min(shard.bytes);
}

fn make_room(shard: &mut ItemShard) {
    if shard.entries < MAX_ENTRIES / SHARDS && shard.bytes < MAX_BYTES / SHARDS {
        return;
    }

    drop_unread_entries(shard);

    if shard.entries >= MAX_ENTRIES / SHARDS / 2 || shard.bytes >= MAX_BYTES / SHARDS / 2 {
        shard.table.clear();
        shard.entries = 0;
        shard.bytes = 0;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn insert(
    state_id: u64,
    base_dir: Direction,
    text: &str,
    start_index: usize,
    length: usize,
    attrs: Option<&AttrList>,
    items: &[Item],
) {
    if items.is_empty() {
        return;
    }
    let Some(probe) = probe_init(state_id, base_dir, text, start_index, length, attrs) else {
        return;
    };

    let mut entry = ItemEntry {
        hash: probe.hash,
        state_id,
        base_dir,
        start_index,
        text: probe.text.into(),
        attrs: probe.attrs.map(<[Attribute]>::to_vec),
        items: copy_items(
            items,
            -(start_index as i32),
            -char_offset_of(text, start_index),
        ),
        bytes: 0,
        read: AtomicBool::new(false),
    };
    entry.bytes = entry.size();

    let mut guard = write_shard(shard_of(probe.hash));
    let shard = match guard.as_mut() {
        Some(shard) => {
            make_room(shard);
            shard
        }
        None => guard.insert(ItemShard::default()),
    };

    let bucket = shard.table.entry(probe.hash).or_default();
    if let Some(position) = bucket.iter().position(|old| old.matches(&probe)) {
        let old = bucket.remove(position);
        shard.entries -= 1;
        shard.bytes -= old.bytes.min(shard.bytes);
    }
    shard.bytes += entry.bytes;
    shard.entries += 1;
    bucket.push(entry);
}

pub fn item_cache_clear() {
    for index in 0..SHARDS {
        let mut guard = write_shard(index);
        if let Some(shard) = guard.as_mut() {
            shard.table.clear();
            shard.entries = 0;
        }
        if let Some(shard) = guard.as_mut() {
            shard.bytes = 0;
        }
    }
}

pub fn item_cache_stats() -> (u64, u64, u64, u64) {
    let entries = (0..SHARDS)
        .map(|index| read_shard(index).as_ref().map_or(0, |shard| shard.entries))
        .sum::<usize>();
    (
        u64::from(ITEM_HITS.load(Ordering::Relaxed)),
        u64::from(ITEM_MISSES.load(Ordering::Relaxed)),
        u64::from(ITEM_SKIPS.load(Ordering::Relaxed)),
        entries as u64,
    )
}
