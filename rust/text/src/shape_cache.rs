//! Southstar text — the cross-layout cache of shaped runs, keyed on everything HarfBuzz reads, stored a word at a time where that is safe.
//! Copyright 2026 Northstar contributors; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::ffi::harfbuzz::Feature;
use crate::font::Font;
use crate::glyph::{GlyphInfo, GlyphString};
use crate::item::Analysis;
use crate::item_cache::{item_cache_clear, mix_hash, shard_of};
use crate::language::Language;
use crate::unicode::{UnicodeType, unichar_type};
use crate::utf8::{char_at, next_char, prev_char};
use crate::utils::caches_enabled;

const MAX_ENTRIES: usize = 20000;
const MAX_TEXT: usize = 4096;
const MAX_BYTES: usize = 48 * 1024 * 1024;
const SHARDS: usize = 16;
pub(crate) const MAX_KEY_FEATURES: usize = 16;
pub(crate) const MAX_SEGMENTS: usize = 512;

pub(crate) const HYPHEN_NONE: u32 = 0;
pub(crate) const HYPHEN_UNICODE: u32 = 1;
pub(crate) const HYPHEN_ASCII: u32 = 2;
pub(crate) const HYPHEN_MISSING: u32 = 3;
pub(crate) const HYPHEN_TRIMMED: u32 = 4;

pub(crate) const fn hyphen_kind(hyphen: u32) -> u32 {
    hyphen & 3
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct KeyFields {
    hash: u32,
    language: Option<Language>,
    level: u32,
    gravity: u32,
    script: u32,
    analysis_flags: u32,
    shape_flags: u32,
    show_flags: u32,
    transform: u32,
    hyphen: u32,
    n_features: usize,
    features: [Feature; MAX_KEY_FEATURES],
}

pub(crate) struct ShapeKey<'a> {
    fields: KeyFields,
    font: &'a Font,
    text: &'a [u8],
}

struct CachedRun {
    glyphs: Vec<GlyphInfo>,
    log_clusters: Vec<i32>,
    read: AtomicBool,
}

struct ShapeEntry {
    fields: KeyFields,
    font: Font,
    text: Box<[u8]>,
    run: CachedRun,
}

#[derive(Default)]
struct ShapeShard {
    table: HashMap<u32, Vec<ShapeEntry>>,
    entries: usize,
    bytes: usize,
}

static SHAPE_SHARDS: [RwLock<Option<ShapeShard>>; SHARDS] = [const { RwLock::new(None) }; SHARDS];
static CACHE_HITS: AtomicU32 = AtomicU32::new(0);
static CACHE_MISSES: AtomicU32 = AtomicU32::new(0);
static CACHE_SKIPS: AtomicU32 = AtomicU32::new(0);
static SKIP_FONT: AtomicU32 = AtomicU32::new(0);
static SKIP_LEN: AtomicU32 = AtomicU32::new(0);
static SKIP_FEAT: AtomicU32 = AtomicU32::new(0);
static SKIP_CTX: AtomicU32 = AtomicU32::new(0);

fn read_shard(index: usize) -> RwLockReadGuard<'static, Option<ShapeShard>> {
    SHAPE_SHARDS[index]
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_shard(index: usize) -> RwLockWriteGuard<'static, Option<ShapeShard>> {
    SHAPE_SHARDS[index]
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn isolated_ideograph(ch: u32) -> bool {
    (0x4E00..=0x9FFF).contains(&ch)
        || (0x3400..=0x4DBF).contains(&ch)
        || (0xF900..=0xFAFF).contains(&ch)
        || (0x20000..=0x2A6DF).contains(&ch)
        || (0x2A700..=0x2EBEF).contains(&ch)
        || (0x3041..=0x3096).contains(&ch)
        || (0x30A1..=0x30FA).contains(&ch)
        || (0xAC00..=0xD7A3).contains(&ch)
}

fn space_boundary_char(ch: u32) -> bool {
    if ch == 0x3000 || ch == 0x1680 {
        return false;
    }
    matches!(ch, 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x2028 | 0x2029)
        || unichar_type(ch) == UnicodeType::SpaceSeparator
}

fn boundary_char(ch: u32) -> bool {
    space_boundary_char(ch) || isolated_ideograph(ch)
}

fn attaches_to_neighbour(ch: u32) -> bool {
    matches!(
        unichar_type(ch),
        UnicodeType::NonSpacingMark
            | UnicodeType::SpacingMark
            | UnicodeType::EnclosingMark
            | UnicodeType::Format
    )
}

fn segment_starts_here(paragraph: &[u8], item_offset: usize, p: usize) -> bool {
    if p <= item_offset {
        return false;
    }
    let here = char_at(paragraph, p);
    if attaches_to_neighbour(here) || space_boundary_char(here) {
        return false;
    }
    let before = char_at(paragraph, prev_char(paragraph, p));
    space_boundary_char(before)
}

pub(crate) fn shape_cache_split(
    paragraph: &[u8],
    item_offset: usize,
    item_length: usize,
    max_segments: usize,
) -> Vec<u32> {
    let mut starts = Vec::new();
    if item_length == 0 {
        return starts;
    }
    let end = item_offset + item_length;
    starts.push(0);
    let mut p = next_char(paragraph, item_offset);
    while p < end {
        if segment_starts_here(paragraph, item_offset, p) {
            if starts.len() == max_segments {
                return Vec::new();
            }
            starts.push((p - item_offset) as u32);
        }
        p = next_char(paragraph, p);
    }
    starts
}

fn context_independent(
    paragraph: &[u8],
    item_offset: usize,
    item_length: usize,
    trailing_edge_is_final: bool,
) -> bool {
    let item_end = item_offset + item_length;

    if item_offset > 0 {
        let first = char_at(paragraph, item_offset);
        if attaches_to_neighbour(first) {
            return false;
        }
        if !boundary_char(first)
            && !boundary_char(char_at(paragraph, prev_char(paragraph, item_offset)))
        {
            return false;
        }
    }

    if !trailing_edge_is_final && item_end < paragraph.len() {
        let last = char_at(paragraph, prev_char(paragraph, item_end));
        if attaches_to_neighbour(last) {
            return false;
        }
        if !boundary_char(last) && !boundary_char(char_at(paragraph, item_end)) {
            return false;
        }
    }

    true
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn shape_cache_key_init<'a>(
    analysis: &'a Analysis,
    paragraph: &'a [u8],
    item_offset: usize,
    item_length: usize,
    shape_flags: u32,
    show_flags: u32,
    transform: u32,
    hyphen: u32,
    features: &[Feature],
) -> Option<ShapeKey<'a>> {
    if !caches_enabled() {
        return None;
    }

    let Some(font) = analysis.font.as_ref() else {
        SKIP_FONT.fetch_add(1, Ordering::Relaxed);
        CACHE_SKIPS.fetch_add(1, Ordering::Relaxed);
        return None;
    };
    if item_length == 0 || item_length > MAX_TEXT {
        SKIP_LEN.fetch_add(1, Ordering::Relaxed);
        CACHE_SKIPS.fetch_add(1, Ordering::Relaxed);
        return None;
    }
    if features.len() > MAX_KEY_FEATURES {
        SKIP_FEAT.fetch_add(1, Ordering::Relaxed);
        CACHE_SKIPS.fetch_add(1, Ordering::Relaxed);
        return None;
    }

    let kind = hyphen_kind(hyphen);
    if !context_independent(
        paragraph,
        item_offset,
        item_length,
        kind == HYPHEN_UNICODE || kind == HYPHEN_ASCII,
    ) {
        SKIP_CTX.fetch_add(1, Ordering::Relaxed);
        CACHE_SKIPS.fetch_add(1, Ordering::Relaxed);
        return None;
    }

    let mut fields = KeyFields {
        hash: 0,
        language: analysis.language,
        level: u32::from(analysis.level),
        gravity: analysis.gravity as u32,
        script: analysis.script.0 as u8 as u32,
        analysis_flags: u32::from(analysis.flags),
        shape_flags,
        show_flags,
        transform,
        hyphen,
        n_features: 0,
        features: [Feature::default(); MAX_KEY_FEATURES],
    };

    let offset = item_offset as u32;
    let length = item_length as u32;
    for feature in features {
        let start = if feature.start <= offset {
            0
        } else {
            (feature.start - offset).min(length)
        };
        let end = if feature.end <= offset {
            0
        } else {
            (feature.end - offset).min(length)
        };
        if start >= end {
            continue;
        }
        fields.features[fields.n_features] = Feature {
            start,
            end,
            ..*feature
        };
        fields.n_features += 1;
    }

    let text = &paragraph[item_offset..item_offset + item_length];
    let mut hash: u32 = 5381;
    for &byte in text {
        hash = hash.wrapping_mul(33).wrapping_add(u32::from(byte));
    }
    let language_id = analysis
        .language
        .map_or(0, |language| language.as_str().as_ptr() as usize as u32);
    hash = hash.wrapping_mul(33).wrapping_add(font.id() as u32);
    hash = hash.wrapping_mul(33).wrapping_add(language_id);
    hash = hash.wrapping_mul(33).wrapping_add(fields.level);
    hash = hash.wrapping_mul(33).wrapping_add(fields.gravity);
    hash = hash.wrapping_mul(33).wrapping_add(fields.script);
    hash = hash.wrapping_mul(33).wrapping_add(fields.analysis_flags);
    hash = hash.wrapping_mul(33).wrapping_add(fields.shape_flags);
    hash = hash.wrapping_mul(33).wrapping_add(fields.show_flags);
    hash = hash.wrapping_mul(33).wrapping_add(fields.transform);
    hash = hash.wrapping_mul(33).wrapping_add(fields.hyphen);
    for feature in &fields.features[..fields.n_features] {
        hash = hash
            .wrapping_mul(33)
            .wrapping_add(feature.tag.wrapping_add(feature.value));
        hash = hash.wrapping_mul(33).wrapping_add(feature.start);
        hash = hash.wrapping_mul(33).wrapping_add(feature.end);
    }
    fields.hash = mix_hash(hash);

    Some(ShapeKey { fields, font, text })
}

impl ShapeEntry {
    fn matches(&self, key: &ShapeKey<'_>) -> bool {
        self.fields == key.fields && self.font.ptr_eq(key.font) && *self.text == *key.text
    }

    fn size(&self) -> usize {
        run_size(&self.run, self.text.len())
    }
}

fn run_size(run: &CachedRun, text_length: usize) -> usize {
    run.glyphs.len() * (core::mem::size_of::<GlyphInfo>() + core::mem::size_of::<i32>())
        + core::mem::size_of::<ShapeEntry>()
        + text_length
}

fn with_entry<R>(key: &ShapeKey<'_>, f: impl FnOnce(Option<&ShapeEntry>) -> R) -> R {
    let guard = read_shard(shard_of(key.fields.hash));
    let entry = guard
        .as_ref()
        .and_then(|shard| shard.table.get(&key.fields.hash))
        .and_then(|bucket| bucket.iter().find(|entry| entry.matches(key)));
    f(entry)
}

pub(crate) fn shape_cache_lookup_at(
    key: &ShapeKey<'_>,
    glyphs: &mut GlyphString,
    at_glyph: usize,
    byte_offset: i32,
) -> bool {
    let found = with_entry(key, |entry| {
        let Some(entry) = entry else {
            return false;
        };
        entry.run.read.store(true, Ordering::Relaxed);
        glyphs.set_size(at_glyph);
        glyphs.glyphs.extend_from_slice(&entry.run.glyphs);
        glyphs.log_clusters.extend(
            entry
                .run
                .log_clusters
                .iter()
                .map(|&cluster| cluster + byte_offset),
        );
        true
    });
    if found {
        CACHE_HITS.fetch_add(1, Ordering::Relaxed);
    } else {
        CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    }
    found
}

pub(crate) fn shape_cache_lookup(key: &ShapeKey<'_>, glyphs: &mut GlyphString) -> bool {
    let found = with_entry(key, |entry| {
        let Some(entry) = entry else {
            return false;
        };
        entry.run.read.store(true, Ordering::Relaxed);
        glyphs.glyphs.clear();
        glyphs.glyphs.extend_from_slice(&entry.run.glyphs);
        glyphs.log_clusters.clear();
        glyphs
            .log_clusters
            .extend_from_slice(&entry.run.log_clusters);
        true
    });
    if found {
        CACHE_HITS.fetch_add(1, Ordering::Relaxed);
    } else {
        CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
    }
    found
}

pub(crate) fn shape_cache_matches(key: &ShapeKey<'_>, glyphs: &GlyphString) -> bool {
    with_entry(key, |entry| {
        entry.is_none_or(|entry| {
            entry.run.glyphs.len() == glyphs.glyphs.len()
                && entry
                    .run
                    .glyphs
                    .iter()
                    .zip(&glyphs.glyphs)
                    .all(|(a, b)| a == b)
                && entry.run.log_clusters == glyphs.log_clusters
        })
    })
}

fn drop_unread_entries(shard: &mut ShapeShard) {
    let mut freed = 0usize;
    let mut removed = 0usize;
    for bucket in shard.table.values_mut() {
        bucket.retain(|entry| {
            if entry.run.read.swap(false, Ordering::Relaxed) {
                true
            } else {
                freed += entry.size();
                removed += 1;
                false
            }
        });
    }
    shard.table.retain(|_, bucket| !bucket.is_empty());
    shard.entries -= removed;
    shard.bytes -= freed.min(shard.bytes);
}

fn make_room(shard: &mut ShapeShard) {
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

fn insert_run(key: &ShapeKey<'_>, run: CachedRun) {
    let entry = ShapeEntry {
        fields: key.fields,
        font: key.font.clone(),
        text: key.text.into(),
        run,
    };
    let entry_size = entry.size();

    let mut guard = write_shard(shard_of(key.fields.hash));
    let shard = match guard.as_mut() {
        Some(shard) => {
            make_room(shard);
            shard
        }
        None => guard.insert(ShapeShard::default()),
    };

    let bucket = shard.table.entry(key.fields.hash).or_default();
    if let Some(position) = bucket.iter().position(|old| old.matches(key)) {
        let old = bucket.remove(position);
        shard.entries -= 1;
        shard.bytes -= old.size().min(shard.bytes);
    }
    shard.bytes += entry_size;
    shard.entries += 1;
    bucket.push(entry);
}

pub(crate) fn shape_cache_insert_range(
    key: &ShapeKey<'_>,
    glyphs: &GlyphString,
    first_glyph: usize,
    n_glyphs: usize,
    byte_offset: i32,
) {
    if n_glyphs == 0 {
        return;
    }
    let range = first_glyph..first_glyph + n_glyphs;
    insert_run(
        key,
        CachedRun {
            glyphs: glyphs.glyphs[range.clone()].to_vec(),
            log_clusters: glyphs.log_clusters[range]
                .iter()
                .map(|&cluster| cluster - byte_offset)
                .collect(),
            read: AtomicBool::new(false),
        },
    );
}

pub(crate) fn shape_cache_insert(key: &ShapeKey<'_>, glyphs: &GlyphString) {
    if glyphs.glyphs.is_empty() {
        return;
    }
    insert_run(
        key,
        CachedRun {
            glyphs: glyphs.glyphs.clone(),
            log_clusters: glyphs.log_clusters.clone(),
            read: AtomicBool::new(false),
        },
    );
}

fn drop_shaped_runs() {
    for index in 0..SHARDS {
        let mut guard = write_shard(index);
        if let Some(shard) = guard.as_mut() {
            shard.table.clear();
            shard.entries = 0;
            shard.bytes = 0;
        }
    }
}

pub fn shape_cache_font_map_changed() {
    drop_shaped_runs();
    item_cache_clear();
}

pub fn shape_cache_stats() -> (u64, u64, u64, u64) {
    let entries = (0..SHARDS)
        .map(|index| read_shard(index).as_ref().map_or(0, |shard| shard.entries))
        .sum::<usize>();
    (
        u64::from(CACHE_HITS.load(Ordering::Relaxed)),
        u64::from(CACHE_MISSES.load(Ordering::Relaxed)),
        u64::from(CACHE_SKIPS.load(Ordering::Relaxed)),
        entries as u64,
    )
}

pub fn shape_cache_skip_reasons() -> (u64, u64, u64, u64) {
    (
        u64::from(SKIP_FONT.load(Ordering::Relaxed)),
        u64::from(SKIP_LEN.load(Ordering::Relaxed)),
        u64::from(SKIP_FEAT.load(Ordering::Relaxed)),
        u64::from(SKIP_CTX.load(Ordering::Relaxed)),
    )
}
