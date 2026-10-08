//! Southstar — the headless driver behind --headless: debug-log levels, the renderer-driven run with its scripted actions, and layout inspection reports.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod inspect;
mod renderer;
mod text;

use core::ffi::CStr;

pub use ffi::NsHeadlessOpts;

const DLOG_LEVELS: core::ops::RangeInclusive<u32> = 0..=5;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dump {
    Text,
    Dom,
    Layout,
    Png,
    Pdf,
    Print,
    None,
    Unknown,
}

pub struct Opts<'a> {
    pub url: Option<&'a CStr>,
    pub dump: Dump,
    pub viewport_width: i32,
    pub viewport_height: i32,
    pub settle_ms: i32,
    pub actions: Option<&'a CStr>,
    pub eval: Option<&'a CStr>,
    pub inspect: Option<&'a CStr>,
    pub inspect_at: Option<&'a CStr>,
}

pub fn nonempty(s: Option<&CStr>) -> Option<&CStr> {
    s.filter(|s| !s.is_empty())
}

pub fn debug_mask(spec: Option<&CStr>) -> u32 {
    let Some(spec) = nonempty(spec).map(CStr::to_bytes) else {
        return 0;
    };
    if spec.eq_ignore_ascii_case(b"all") {
        return 0xFFFF_FFFF;
    }
    let mut mask = 0;
    for token in spec.split(|&b| b == b',') {
        let token = text::strip(token);
        for level in DLOG_LEVELS {
            if ffi::dlog_level_name(level).is_some_and(|name| token.eq_ignore_ascii_case(name)) {
                mask |= 1 << level;
            }
        }
    }
    mask
}
