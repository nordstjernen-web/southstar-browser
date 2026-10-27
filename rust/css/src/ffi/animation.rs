//! Southstar — the animation and transition lists a value keeps, and the ones a computed style's longhands give.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use std::ffi::CString;
use std::sync::OnceLock;

use southstar_glib::{self as glib, GBoolean};

use super::computed_units::RawStyle;
use super::value::{NsCssValue, c_dup};
use crate::animation::{self, ENTRIES_MAX, Entry, Longhand};
use crate::prop::Prop;
use crate::timing::Timing;

#[derive(Clone)]
pub(crate) struct RawEntry {
    target: c_int,
    name: Option<CString>,
    duration_ms: f64,
    delay_ms: f64,
    timing: Timing,
    iter_count: c_int,
    iterations: f64,
    direction: c_int,
    fill: c_int,
    paused: GBoolean,
    duration_auto: GBoolean,
    allow_discrete: GBoolean,
    document_timeline: GBoolean,
}

#[derive(Clone)]
pub(crate) struct RawList {
    n: c_int,
    entries: [RawEntry; ENTRIES_MAX],
}

const ZERO_ENTRY: RawEntry = RawEntry {
    target: 0,
    name: None,
    duration_ms: 0.0,
    delay_ms: 0.0,
    timing: Timing {
        kind: 0,
        steps: 0,
        step_pos: 0,
        jump_keyword: 0,
        cb: [0.0; 4],
    },
    iter_count: 0,
    iterations: 0.0,
    direction: 0,
    fill: 0,
    paused: 0,
    duration_auto: 0,
    allow_discrete: 0,
    document_timeline: 1,
};

impl RawList {
    pub(crate) fn from_entries(entries: &[Entry]) -> RawList {
        let mut list = RawList {
            n: entries.len() as c_int,
            entries: [ZERO_ENTRY; ENTRIES_MAX],
        };
        for (slot, e) in list.entries.iter_mut().zip(entries) {
            *slot = RawEntry::from_entry(e);
        }
        list
    }

    pub(super) fn entries(&self) -> Vec<Entry> {
        let n = usize::try_from(self.n).unwrap_or(0).min(ENTRIES_MAX);
        self.entries[..n].iter().map(RawEntry::entry).collect()
    }
}

impl RawEntry {
    fn from_entry(e: &Entry) -> RawEntry {
        RawEntry {
            target: e.target,
            name: e.name.as_deref().map(c_dup),
            duration_ms: e.duration_ms,
            delay_ms: e.delay_ms,
            timing: e.timing,
            iter_count: e.iter_count,
            iterations: e.iterations,
            direction: e.direction,
            fill: e.fill,
            paused: glib::boolean(e.paused),
            duration_auto: glib::boolean(e.duration_auto),
            allow_discrete: glib::boolean(e.allow_discrete),
            document_timeline: glib::boolean(e.document_timeline),
        }
    }

    fn entry(&self) -> Entry {
        Entry {
            target: self.target,
            name: self.name.as_deref().map(|name| name.to_bytes().to_vec()),
            duration_ms: self.duration_ms,
            delay_ms: self.delay_ms,
            timing: self.timing,
            iter_count: self.iter_count,
            iterations: self.iterations,
            direction: self.direction,
            fill: self.fill,
            paused: self.paused != 0,
            duration_auto: self.duration_auto != 0,
            allow_discrete: self.allow_discrete != 0,
            document_timeline: self.document_timeline != 0,
        }
    }
}

fn prop_ids() -> &'static [c_int; 17] {
    static IDS: OnceLock<[c_int; 17]> = OnceLock::new();
    IDS.get_or_init(|| {
        Longhand::ALL
            .map(|lh| Prop::from_name(lh.name().to_bytes()).map_or(-1, |p| p.id() as c_int))
    })
}

fn prop_of(lh: Longhand) -> c_int {
    let i = Longhand::ALL.iter().position(|&l| l == lh).unwrap_or(0);
    prop_ids()[i]
}

fn longhand_value(style: &RawStyle, lh: Longhand) -> Option<&NsCssValue> {
    style.value_at(usize::try_from(prop_of(lh)).ok()?)
}

fn longhand_keyword(style: &RawStyle, lh: Longhand) -> Option<&[u8]> {
    longhand_value(style, lh)?.keyword().map(CStr::to_bytes)
}

fn c_names(mut entries: Vec<Entry>) -> Vec<Entry> {
    for entry in &mut entries {
        if let Some(name) = &mut entry.name
            && let Some(nul) = name.iter().position(|&c| c == 0)
        {
            name.truncate(nul);
        }
    }
    entries
}

pub fn style_may_animate(style: &RawStyle) -> bool {
    animation::may_animate(|lh| longhand_value(style, lh).is_some())
}

pub fn animation_effective(style: &RawStyle, is_animation: bool) -> Vec<Entry> {
    c_names(animation::effective(
        |lh| longhand_keyword(style, lh),
        |lh| longhand_value(style, lh).is_some(),
        is_animation,
    ))
}

pub fn animation_lists(style: Option<&RawStyle>, is_animation: bool) -> (Vec<Entry>, bool) {
    let Some(style) = style else {
        return (Vec::new(), false);
    };
    let (entries, mismatch) = animation::lists(|lh| longhand_keyword(style, lh), is_animation);
    (c_names(entries), mismatch)
}
