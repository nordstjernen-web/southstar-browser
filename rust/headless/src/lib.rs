//! Southstar — the headless driver behind --headless: debug-log levels, the renderer-driven run with its scripted actions, and layout inspection reports.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;
mod inproc;
mod input;
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

#[derive(Clone, Copy)]
pub struct Opts<'a> {
    pub url: Option<&'a CStr>,
    pub dump: Dump,
    pub out_path: Option<&'a CStr>,
    pub viewport_width: i32,
    pub viewport_height: i32,
    pub settle_ms: i32,
    pub time_ms: i32,
    pub debug_levels: u32,
    pub actions: Option<&'a CStr>,
    pub eval: Option<&'a CStr>,
    pub inspect: Option<&'a CStr>,
    pub inspect_at: Option<&'a CStr>,
    pub wpt: bool,
    pub wpt_timeout_ms: i32,
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

fn renderer_capable(o: &Opts) -> bool {
    if std::env::var_os("NS_HEADLESS_LEGACY").is_some() || o.wpt {
        return false;
    }
    if nonempty(o.inspect).is_some() || nonempty(o.inspect_at).is_some() {
        return false;
    }
    !matches!(o.dump, Dump::Png | Dump::Pdf | Dump::Print)
}

pub fn run(opts: Option<&Opts>) -> i32 {
    let Some((o, url)) = opts.and_then(|o| nonempty(o.url).map(|url| (o, url))) else {
        ffi::err(b"headless: --url is required\n");
        return 2;
    };
    ffi::console_setup();
    let subscription = ffi::subscribe_dlog(o.debug_levels);
    let rc = if renderer_capable(o) {
        renderer::run(o)
    } else {
        inproc::run_one(o, url, 0, None, None)
    };
    drop(subscription);
    rc
}
