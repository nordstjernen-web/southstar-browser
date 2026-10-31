//! Southstar — the style sheet caches: parsed sheets kept per <style> element, per merged style text, per linked URL and per imported URL and layer, so a style pass reparses only what changed.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::RefCell;
use std::collections::HashMap;

use southstar_dom::Node;

use super::sheet::OwnedSheet;
use crate::style_text;

const STYLE_ELEMENTS_MAX: usize = 2048;
const MERGED_TRIM_ABOVE: usize = 64;
const MERGED_TRIM_TO: usize = 48;
const URL_SHEETS_MAX: usize = 256;

fn viewport() -> (f64, f64) {
    (
        southstar_css_media::viewport_w(),
        southstar_css_media::viewport_h(),
    )
}

fn viewport_rounded() -> (u64, u64) {
    let (w, h) = viewport();
    (w.round_ties_even().to_bits(), h.round_ties_even().to_bits())
}

struct StyleElementSheet {
    css: Vec<u8>,
    viewport: (f64, f64),
    sheet: OwnedSheet,
}

#[derive(PartialEq, Eq, Hash)]
struct MergedKey {
    viewport: (u64, u64),
    base: Option<Vec<u8>>,
    css: Vec<u8>,
}

struct MergedSheet {
    sheet: OwnedSheet,
    stamp: u64,
}

#[derive(PartialEq, Eq, Hash)]
struct UrlKey {
    viewport: (u64, u64),
    layer: Option<Vec<u8>>,
    url: Vec<u8>,
}

struct ImportSheet {
    css: Vec<u8>,
    sheet: OwnedSheet,
}

#[derive(Default)]
struct Caches {
    style_elements: HashMap<usize, StyleElementSheet>,
    merged: HashMap<MergedKey, MergedSheet>,
    clock: u64,
    pass_start: u64,
    links: HashMap<UrlKey, OwnedSheet>,
    imports: HashMap<UrlKey, ImportSheet>,
    relayout_depth: u32,
    media_generation: u64,
}

type Dropped = (
    HashMap<usize, StyleElementSheet>,
    HashMap<MergedKey, MergedSheet>,
    HashMap<UrlKey, OwnedSheet>,
    HashMap<UrlKey, ImportSheet>,
);

impl Caches {
    fn take_sheets(&mut self) -> Dropped {
        (
            core::mem::take(&mut self.style_elements),
            core::mem::take(&mut self.merged),
            core::mem::take(&mut self.links),
            core::mem::take(&mut self.imports),
        )
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn trim_merged(&mut self, keep_after: u64) {
        if self.merged.len() <= MERGED_TRIM_ABOVE {
            return;
        }
        let mut evictable: Vec<u64> = self
            .merged
            .values()
            .map(|entry| entry.stamp)
            .filter(|&stamp| stamp <= keep_after)
            .collect();
        let excess = (self.merged.len() - MERGED_TRIM_TO).min(evictable.len());
        if excess == 0 {
            return;
        }
        evictable.sort_unstable();
        let newest_evicted = evictable[excess - 1];
        self.merged.retain(|_, entry| entry.stamp > newest_evicted);
    }

    fn nested_relayout(&self) -> bool {
        self.relayout_depth > 1
    }
}

thread_local! {
    static CACHES: RefCell<Caches> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut Caches) -> R) -> R {
    let generation = southstar_css_media::generation();
    let stale = CACHES.with_borrow_mut(|caches| {
        (caches.media_generation != generation).then(|| {
            caches.media_generation = generation;
            caches.take_sheets()
        })
    });
    drop(stale);
    CACHES.with_borrow_mut(f)
}

fn url_key(url: &[u8], layer: Option<&[u8]>) -> Option<UrlKey> {
    (!url.is_empty()).then(|| UrlKey {
        viewport: viewport_rounded(),
        layer: layer.map(<[u8]>::to_vec),
        url: url.to_vec(),
    })
}

pub fn stylesheet_from_style_element_cached(
    style: Node<'_>,
    base: Option<&[u8]>,
) -> Option<OwnedSheet> {
    let css = style_text::style_element_text(style)?;
    let key = style.as_ptr() as usize;
    let viewport = viewport();
    let hit = with(|caches| {
        caches
            .style_elements
            .get(&key)
            .filter(|entry| entry.css == css && entry.viewport == viewport)
            .map(|entry| entry.sheet.clone())
    });
    if hit.is_some() {
        return hit;
    }
    let sheet = OwnedSheet::parse_resolved(&css, base, None);
    let out = sheet.clone();
    let entry = StyleElementSheet {
        css,
        viewport,
        sheet,
    };
    drop(with(|caches| caches.style_elements.insert(key, entry)));
    Some(out)
}

pub fn merged_styles_cached(css: &[u8], base: Option<&[u8]>) -> Option<OwnedSheet> {
    if css.is_empty() {
        return None;
    }
    let key = MergedKey {
        viewport: viewport_rounded(),
        base: base.map(<[u8]>::to_vec),
        css: css.to_vec(),
    };
    let hit = with(|caches| {
        let Caches { merged, clock, .. } = caches;
        merged.get_mut(&key).map(|entry| {
            *clock += 1;
            entry.stamp = *clock;
            entry.sheet.clone()
        })
    });
    if hit.is_some() {
        return hit;
    }
    let sheet = OwnedSheet::parse_resolved(&key.css, base, None);
    let out = sheet.clone();
    drop(with(|caches| {
        let stamp = caches.tick();
        caches.merged.insert(key, MergedSheet { sheet, stamp })
    }));
    Some(out)
}

pub fn parse_url_cached(url: &[u8], css: &[u8]) -> OwnedSheet {
    let Some(key) = url_key(url, None) else {
        return OwnedSheet::parse_resolved(css, Some(url), None);
    };
    if let Some(sheet) = with(|caches| caches.links.get(&key).cloned()) {
        return sheet;
    }
    let sheet = OwnedSheet::parse_resolved(css, Some(url), None);
    let out = sheet.clone();
    drop(with(|caches| caches.links.insert(key, sheet)));
    out
}

pub fn parse_import_cached(url: &[u8], layer: Option<&[u8]>, css: &[u8]) -> OwnedSheet {
    let Some(key) = url_key(url, layer) else {
        return OwnedSheet::parse_resolved(css, Some(url), layer);
    };
    let hit = with(|caches| {
        caches
            .imports
            .get(&key)
            .filter(|entry| entry.css == css)
            .map(|entry| entry.sheet.clone())
    });
    if let Some(sheet) = hit {
        return sheet;
    }
    let sheet = OwnedSheet::parse_resolved(css, Some(url), layer);
    let out = sheet.clone();
    let entry = ImportSheet {
        css: css.to_vec(),
        sheet,
    };
    drop(with(|caches| caches.imports.insert(key, entry)));
    out
}

pub fn relayout_enter() {
    with(|caches| caches.relayout_depth += 1);
}

pub fn relayout_leave() {
    with(|caches| caches.relayout_depth = caches.relayout_depth.saturating_sub(1));
}

pub fn style_element_cache_begin() {
    with(|caches| {
        if caches.nested_relayout() {
            return;
        }
        if caches.style_elements.len() > STYLE_ELEMENTS_MAX {
            caches.style_elements.clear();
        }
        caches.trim_merged(u64::MAX);
        caches.pass_start = caches.clock;
        if caches.links.len() > URL_SHEETS_MAX {
            caches.links.clear();
        }
        if caches.imports.len() > URL_SHEETS_MAX {
            caches.imports.clear();
        }
    });
}

pub fn style_element_cache_end() {
    with(|caches| {
        if !caches.nested_relayout() {
            caches.trim_merged(caches.pass_start);
        }
    });
}
