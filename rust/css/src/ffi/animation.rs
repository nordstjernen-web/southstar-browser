//! Southstar — the C ABI of time values, easing functions and the animation and transition lists css.h lays out.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_char, c_int, c_void};
use core::ptr;
use core::slice;
use std::sync::OnceLock;

use southstar_glib::{self as glib, GBoolean};

use super::value::{self, NsCssValue};
use crate::animation::{self, ENTRIES_MAX, Entry, Longhand};
use crate::time;
use crate::timing::{self, Timing};

unsafe extern "C" {
    fn ns_css_prop_id(name: *const c_char) -> c_int;
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawEntry {
    target: c_int,
    name: *mut c_char,
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
}

#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct RawList {
    n: c_int,
    entries: [RawEntry; ENTRIES_MAX],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(
    core::mem::size_of::<RawEntry>() == 120
        && core::mem::offset_of!(RawEntry, timing) == 32
        && core::mem::offset_of!(RawEntry, iterations) == 88
        && core::mem::offset_of!(RawEntry, allow_discrete) == 112
        && core::mem::size_of::<RawList>() == 968
);

const ZERO_ENTRY: RawEntry = RawEntry {
    target: 0,
    name: ptr::null_mut(),
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

    pub(super) unsafe fn entries(&self) -> Vec<Entry> {
        let n = usize::try_from(self.n).unwrap_or(0).min(ENTRIES_MAX);
        self.entries[..n]
            .iter()
            .map(|e| unsafe { e.entry() })
            .collect()
    }
}

impl RawEntry {
    fn from_entry(e: &Entry) -> RawEntry {
        RawEntry {
            target: e.target,
            name: e.name.as_deref().map_or(ptr::null_mut(), glib::strdup),
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
        }
    }

    unsafe fn entry(&self) -> Entry {
        Entry {
            target: self.target,
            name: unsafe { bytes(self.name) }.map(<[u8]>::to_vec),
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
        }
    }
}

unsafe fn bytes<'a>(s: *const c_char) -> Option<&'a [u8]> {
    (!s.is_null()).then(|| unsafe { CStr::from_ptr(s) }.to_bytes())
}

fn owned(text: Option<Vec<u8>>) -> *mut c_char {
    text.map_or(ptr::null_mut(), |text| glib::strdup(&text))
}

fn prop_ids() -> &'static [c_int; 17] {
    static IDS: OnceLock<[c_int; 17]> = OnceLock::new();
    IDS.get_or_init(|| Longhand::ALL.map(|lh| unsafe { ns_css_prop_id(lh.name().as_ptr()) }))
}

fn longhand_of(prop: c_int) -> Option<Longhand> {
    prop_ids()
        .iter()
        .position(|&id| id == prop)
        .map(|i| Longhand::ALL[i])
}

fn prop_of(lh: Longhand) -> c_int {
    let i = Longhand::ALL.iter().position(|&l| l == lh).unwrap_or(0);
    prop_ids()[i]
}

unsafe fn longhand_value<'a>(style: *const c_void, lh: Longhand) -> Option<&'a NsCssValue> {
    unsafe { value::style_value(style, prop_of(lh)) }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_timing_parse(text: *const c_char, out: *mut Timing) -> GBoolean {
    if out.is_null() {
        return 0;
    }
    let Some(t) = unsafe { bytes(text) }.and_then(timing::parse) else {
        return 0;
    };
    unsafe { *out = t };
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_timing_serialize(t: *const Timing) -> *mut c_char {
    glib::strdup(&timing::serialize(unsafe { t.as_ref() }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_time_property(t: *const c_char) -> *mut NsCssValue {
    match unsafe { bytes(t) } {
        Some(text) if time::property_valid(text) => value::new_keyword(text),
        _ => ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_animation_duration(t: *const c_char) -> *mut NsCssValue {
    unsafe { bytes(t) }
        .and_then(animation::duration_canonical)
        .map_or(ptr::null_mut(), |canon| value::new_keyword(&canon))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_anim_longhand(
    prop: c_int,
    t: *const c_char,
) -> *mut NsCssValue {
    let (Some(lh), Some(text)) = (longhand_of(prop), unsafe { bytes(t) }) else {
        return ptr::null_mut();
    };
    animation::longhand_canonical(lh, text)
        .map_or(ptr::null_mut(), |canon| value::new_keyword(&canon))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_parse_anim_value(
    t: *const c_char,
    is_animation: GBoolean,
) -> *mut NsCssValue {
    unsafe { bytes(t) }
        .and_then(|text| animation::shorthand_parse(text, is_animation != 0))
        .map_or(ptr::null_mut(), |list| {
            value::new_anim(&RawList::from_entries(&list))
        })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_anim_list_clear(list: *mut RawList) {
    let Some(list) = (unsafe { list.as_mut() }) else {
        return;
    };
    let n = usize::try_from(list.n).unwrap_or(0).min(ENTRIES_MAX);
    for e in &list.entries[..n] {
        unsafe { glib::g_free(e.name.cast()) };
    }
    list.n = 0;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_style_may_animate(style: *const c_void) -> GBoolean {
    if style.is_null() {
        return 0;
    }
    glib::boolean(animation::may_animate(|lh| {
        unsafe { longhand_value(style, lh) }.is_some()
    }))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_anim_effective(
    style: *const c_void,
    is_animation: GBoolean,
    out: *mut RawList,
) {
    if out.is_null() {
        return;
    }
    let entries = if style.is_null() {
        Vec::new()
    } else {
        animation::effective(
            |lh| unsafe { value::keyword_of(longhand_value(style, lh)) },
            |lh| unsafe { longhand_value(style, lh) }.is_some(),
            is_animation != 0,
        )
    };
    unsafe { *out = RawList::from_entries(&entries) };
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_anim_lists(
    style: *const c_void,
    is_animation: GBoolean,
    out: *mut RawList,
    out_mismatch: *mut GBoolean,
) {
    if out.is_null() {
        return;
    }
    let (entries, mismatch) = if style.is_null() {
        (Vec::new(), false)
    } else {
        animation::lists(
            |lh| unsafe { value::keyword_of(longhand_value(style, lh)) },
            is_animation != 0,
        )
    };
    unsafe { *out = RawList::from_entries(&entries) };
    if !out_mismatch.is_null() {
        unsafe { *out_mismatch = glib::boolean(mismatch) };
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_anim_shorthand_serialize(
    list: *const RawList,
    is_animation: GBoolean,
) -> *mut c_char {
    let entries = unsafe { list.as_ref() }
        .map(|l| unsafe { l.entries() })
        .unwrap_or_default();
    glib::strdup(&animation::shorthand_serialize(&entries, is_animation != 0))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_animation_shorthand_canonical(
    text: *const c_char,
    is_animation: GBoolean,
) -> *mut c_char {
    owned(unsafe { bytes(text) }.and_then(|t| animation::shorthand_canonical(t, is_animation != 0)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_time_specified(value: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(value) }.and_then(|v| time::list_serialize(v, false)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_time_computed(value: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(value) }.and_then(|v| time::list_serialize(v, true)))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_anim_range_shorthand_expand(
    text: *const c_char,
    out_start: *mut *mut c_char,
    out_end: *mut *mut c_char,
) -> GBoolean {
    let Some((start, end)) = unsafe { bytes(text) }.and_then(animation::range_shorthand_expand)
    else {
        return 0;
    };
    unsafe {
        *out_start = glib::strdup(&start);
        *out_end = glib::strdup(&end);
    }
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_animation_range_serialize(
    start_list: *const c_char,
    end_list: *const c_char,
) -> *mut c_char {
    let start = unsafe { bytes(start_list) }.unwrap_or(b"normal");
    let end = unsafe { bytes(end_list) }.unwrap_or(b"normal");
    glib::strdup(&animation::range_serialize(start, end))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_ident_decode(tok: *const c_char) -> *mut c_char {
    owned(unsafe { bytes(tok) }.and_then(animation::ident_decode))
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_css_starts_math_fn(s: *const c_char, e: *const c_char) -> GBoolean {
    if s.is_null() {
        return 0;
    }
    let len = usize::try_from(unsafe { e.offset_from(s) }).unwrap_or(0);
    glib::boolean(time::starts_math_fn(unsafe {
        slice::from_raw_parts(s.cast::<u8>(), len)
    }))
}
