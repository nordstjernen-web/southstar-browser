//! Southstar — which properties animate, transition, animate discretely or need a relayout when they change.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;
use std::sync::OnceLock;

use crate::ffi::css::{PROP_COUNT, prop_id};

const NOT_ANIMATABLE: [&CStr; 23] = [
    c"transition",
    c"transition-property",
    c"transition-duration",
    c"transition-delay",
    c"transition-timing-function",
    c"transition-behavior",
    c"animation",
    c"animation-name",
    c"animation-duration",
    c"animation-delay",
    c"animation-timing-function",
    c"animation-iteration-count",
    c"animation-direction",
    c"animation-fill-mode",
    c"animation-play-state",
    c"animation-composition",
    c"animation-timeline",
    c"animation-range-start",
    c"animation-range-end",
    c"container-type",
    c"container-name",
    c"direction",
    c"unicode-bidi",
];

const NOT_TRANSITIONABLE: [&CStr; 2] = [c"display", c"content"];

pub struct Known {
    animatable: [bool; PROP_COUNT],
    transitionable: [bool; PROP_COUNT],
    pub visibility: i32,
    pub opacity: i32,
    pub transform: i32,
    pub color: i32,
    pub background_color: i32,
    pub animation_timing_function: i32,
}

fn mark(table: &mut [bool; PROP_COUNT], names: &[&CStr], value: bool) {
    for name in names {
        if let Some(slot) = usize::try_from(prop_id(name))
            .ok()
            .and_then(|i| table.get_mut(i))
        {
            *slot = value;
        }
    }
}

pub fn known() -> &'static Known {
    static KNOWN: OnceLock<Known> = OnceLock::new();
    KNOWN.get_or_init(|| {
        let mut animatable = [true; PROP_COUNT];
        mark(&mut animatable, &NOT_ANIMATABLE, false);
        let mut transitionable = animatable;
        mark(&mut transitionable, &NOT_TRANSITIONABLE, false);
        Known {
            animatable,
            transitionable,
            visibility: prop_id(c"visibility"),
            opacity: prop_id(c"opacity"),
            transform: prop_id(c"transform"),
            color: prop_id(c"color"),
            background_color: prop_id(c"background-color"),
            animation_timing_function: prop_id(c"animation-timing-function"),
        }
    })
}

fn lookup(table: &[bool; PROP_COUNT], prop: i32) -> bool {
    usize::try_from(prop)
        .ok()
        .and_then(|i| table.get(i))
        .copied()
        .unwrap_or(false)
}

pub fn in_range(prop: i32) -> bool {
    usize::try_from(prop).is_ok_and(|i| i < PROP_COUNT)
}

pub fn animatable(prop: i32) -> bool {
    lookup(&known().animatable, prop)
}

pub fn transitionable(prop: i32) -> bool {
    lookup(&known().transitionable, prop)
}

pub fn discretely_animatable(prop: i32) -> bool {
    prop == known().visibility
}

pub fn needs_relayout(prop: i32) -> bool {
    let k = known();
    prop != k.opacity && prop != k.transform && prop != k.color && prop != k.background_color
}
