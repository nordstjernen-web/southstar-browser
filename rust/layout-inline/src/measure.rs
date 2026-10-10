//! Southstar — measuring a laid-out ns-pango paragraph, with a per-thread cache keyed by the layout's settings, attributes, text and font.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use core::hash::{BuildHasherDefault, Hasher};
use std::collections::HashMap;

use southstar_paint::ffi::pango::{self, FontDescRef, FontDescription, Layout, Rectangle};

const CACHE_MAX: usize = 32768;

#[derive(Clone, Copy, Default)]
pub(crate) struct Measure {
    pub logical: Rectangle,
    pub lines: i32,
    pub baseline: i32,
}

impl Measure {
    pub fn pixel_logical(self) -> Rectangle {
        let mut r = self.logical;
        pango::extents_to_pixels(&mut r);
        r
    }
}

#[derive(Default)]
struct HashPassthrough(u64);

impl Hasher for HashPassthrough {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8) | u64::from(b);
        }
    }

    fn write_u32(&mut self, v: u32) {
        self.0 = u64::from(v);
    }
}

struct Entry {
    data: Box<[u8]>,
    font: Option<FontDescription>,
    measure: Measure,
}

#[derive(Default)]
struct Cache {
    buckets: HashMap<u32, Vec<Entry>, BuildHasherDefault<HashPassthrough>>,
    len: usize,
    scratch: Vec<u8>,
}

fn fonts_equal(a: Option<&FontDescription>, b: Option<FontDescRef>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.as_ref().equal(b),
        (None, None) => true,
        _ => false,
    }
}

impl Cache {
    fn lookup(&self, hash: u32, font: Option<FontDescRef>) -> Option<Measure> {
        self.buckets.get(&hash)?.iter().find_map(|e| {
            (*e.data == *self.scratch && fonts_equal(e.font.as_ref(), font)).then_some(e.measure)
        })
    }

    fn insert(&mut self, hash: u32, font: Option<FontDescRef>, measure: Measure) {
        if self.len >= CACHE_MAX {
            self.buckets.clear();
            self.len = 0;
        }
        let entry = Entry {
            data: self.scratch.as_slice().into(),
            font: font.map(FontDescRef::copy_owned),
            measure,
        };
        self.buckets.entry(hash).or_default().push(entry);
        self.len += 1;
    }
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache::default());
}

fn build_key(layout: &Layout, buf: &mut Vec<u8>) -> Option<(u32, Option<FontDescRef>)> {
    if layout.has_tabs() {
        return None;
    }
    let attr_str = layout.attributes_string();
    let attr_bytes = attr_str.as_deref().map_or(&[][..], |s| s.to_bytes());
    if attr_bytes.windows(6).any(|w| w == b" shape") {
        return None;
    }
    let flags = u32::from(layout.justify())
        | u32::from(layout.single_paragraph_mode()) << 2
        | u32::from(layout.auto_dir()) << 3
        | (layout.alignment() as u32) << 4
        | (layout.wrap() as u32) << 8
        | (layout.ellipsize() as u32) << 12;
    buf.clear();
    buf.extend_from_slice(&layout.context().serial().to_ne_bytes());
    buf.extend_from_slice(&layout.width().to_ne_bytes());
    buf.extend_from_slice(&layout.height().to_ne_bytes());
    buf.extend_from_slice(&layout.indent().to_ne_bytes());
    buf.extend_from_slice(&layout.spacing().to_ne_bytes());
    buf.extend_from_slice(&layout.line_spacing().to_ne_bytes());
    buf.extend_from_slice(&flags.to_ne_bytes());
    buf.extend_from_slice(&(attr_bytes.len() as u32).to_ne_bytes());
    buf.extend_from_slice(attr_bytes);
    if let Some(text) = layout.text() {
        buf.extend_from_slice(text.to_bytes());
    }
    let mut h: u32 = 2166136261;
    for &b in buf.iter() {
        h = (h ^ u32::from(b)).wrapping_mul(16777619);
    }
    let font = layout.font_description();
    if let Some(fd) = font {
        h ^= fd.hash_value().wrapping_mul(0x9e3779b1);
    }
    Some((h, font))
}

pub(crate) fn measure(layout: &Layout) -> Measure {
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let mut scratch = core::mem::take(&mut cache.scratch);
        let key = build_key(layout, &mut scratch);
        cache.scratch = scratch;
        if let Some((hash, font)) = key
            && let Some(hit) = cache.lookup(hash, font)
        {
            return hit;
        }
        let m = Measure {
            logical: layout.logical_extents(),
            lines: layout.line_count(),
            baseline: layout.baseline(),
        };
        if let Some((hash, font)) = key {
            cache.insert(hash, font, m);
        }
        m
    })
}
