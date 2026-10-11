//! Southstar — reading the shell's command line: which mode to run, the headless options and the settings flags.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

pub const DUMP_TEXT: u32 = 0;
pub const DUMP_DOM: u32 = 1;
pub const DUMP_LAYOUT: u32 = 2;
pub const DUMP_PNG: u32 = 3;
pub const DUMP_PDF: u32 = 4;
pub const DUMP_PRINT: u32 = 5;
pub const DUMP_NONE: u32 = 6;

pub const DLOG_NET: u32 = 4;

fn rest<'a>(arg: &'a CStr, prefix: &[u8]) -> Option<&'a CStr> {
    arg.to_bytes()
        .starts_with(prefix)
        .then(|| CStr::from_bytes_with_nul(&arg.to_bytes_with_nul()[prefix.len()..]).ok())
        .flatten()
}

fn is(arg: &CStr, flag: &[u8]) -> bool {
    arg.to_bytes() == flag
}

fn has_prefix(arg: &CStr, prefix: &[u8]) -> bool {
    arg.to_bytes().starts_with(prefix)
}

#[cfg(windows)]
pub fn need_console(args: &[&CStr]) -> bool {
    args.iter().skip(1).any(|a| {
        is(a, b"--headless")
            || is(a, b"--print-config")
            || is(a, b"--wpt")
            || [
                &b"--dump="[..],
                b"--url=",
                b"--viewport=",
                b"--eval=",
                b"--inspect=",
                b"--inspect-at=",
                b"--wpt-timeout-ms=",
                b"--settle-ms=",
            ]
            .iter()
            .any(|p| has_prefix(a, p))
    })
}

pub fn proc_mode_wanted(args: &[&CStr]) -> bool {
    !args.iter().skip(1).any(|a| {
        is(a, b"--headless")
            || is(a, b"--print-config")
            || is(a, b"--wpt")
            || [
                &b"--dump="[..],
                b"--eval=",
                b"--inspect=",
                b"--inspect-at=",
                b"--act=",
            ]
            .iter()
            .any(|p| has_prefix(a, p))
    })
}

pub fn flag_given(args: &[&CStr], flag: &[u8]) -> bool {
    args.iter().skip(1).any(|a| is(a, flag))
}

pub fn env_enabled(value: Option<&[u8]>) -> bool {
    value.is_some_and(|v| !v.is_empty() && v != b"0")
}

pub fn screenshot_paths<'a>(args: &[&'a CStr]) -> Vec<&'a CStr> {
    args.iter()
        .skip(1)
        .filter_map(|a| {
            if let Some(v) = rest(a, b"--dump=") {
                rest(v, b"png:")
                    .or_else(|| rest(v, b"pdf:"))
                    .or_else(|| rest(v, b"print:"))
            } else {
                rest(a, b"--screenshot=")
            }
        })
        .filter(|p| !p.is_empty() && p.to_bytes()[0] != b'-')
        .collect()
}

pub fn is_local_path_candidate(url: &[u8]) -> bool {
    !url.windows(3).any(|w| w == b"://")
        && !url.starts_with(b"about:")
        && !url.starts_with(b"data:")
}

pub enum Gsk {
    Auto,
    Set(&'static CStr),
    Unknown,
}

pub fn gsk_renderer(pref: Option<&[u8]>) -> Gsk {
    let pref = match pref {
        None => return Gsk::Auto,
        Some(p) => p,
    };
    if pref.is_empty()
        || [&b"auto"[..], b"default", b"system"]
            .iter()
            .any(|k| pref.eq_ignore_ascii_case(k))
    {
        return Gsk::Auto;
    }
    [c"gl", c"ngl", c"opengl", c"vulkan", c"cairo", c"help"]
        .into_iter()
        .find(|k| pref.eq_ignore_ascii_case(k.to_bytes()))
        .map_or(Gsk::Unknown, Gsk::Set)
}

pub fn ascii_strtoll(text: &[u8]) -> (i64, usize) {
    let mut i = 0;
    while i < text.len() && matches!(text[i], b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c) {
        i += 1;
    }
    let negative = text.get(i) == Some(&b'-');
    if matches!(text.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let start = i;
    let mut value: u64 = 0;
    let mut overflow = false;
    while i < text.len() && text[i].is_ascii_digit() {
        let digit = u64::from(text[i] - b'0');
        match value.checked_mul(10).and_then(|v| v.checked_add(digit)) {
            Some(v) => value = v,
            None => overflow = true,
        }
        i += 1;
    }
    if i == start {
        return (0, 0);
    }
    let limit = if negative {
        i64::MIN.unsigned_abs()
    } else {
        i64::MAX as u64
    };
    let value = if overflow || value > limit {
        if negative { i64::MIN } else { i64::MAX }
    } else if negative {
        (value as i64).wrapping_neg()
    } else {
        value as i64
    };
    (value, i)
}

fn whole_number(text: &[u8]) -> Option<i64> {
    let (n, end) = ascii_strtoll(text);
    (end != 0 && end == text.len()).then_some(n)
}

fn size_pair(spec: &[u8]) -> Option<(i64, i64)> {
    let (w, end) = ascii_strtoll(spec);
    if end == 0 || !matches!(spec.get(end), Some(b'x' | b'X')) || w <= 0 || w >= 100_000 {
        return None;
    }
    let h = whole_number(&spec[end + 1..])?;
    (h > 0 && h < 100_000).then_some((w, h))
}

fn viewport(spec: &[u8]) -> Option<(i64, i64)> {
    let (w, end) = ascii_strtoll(spec);
    if end == 0 || w <= 0 || w >= 100_000 {
        return None;
    }
    match spec.get(end) {
        Some(b'x' | b'X') => {
            let h = whole_number(&spec[end + 1..])?;
            (h > 0 && h < 100_000).then_some((w, h))
        }
        None => Some((w, 0)),
        Some(_) => None,
    }
}

fn bounded(text: &[u8], low: i64, high: i64) -> Option<i32> {
    whole_number(text)
        .filter(|&n| n >= low && n < high)
        .map(|n| n as i32)
}

pub enum Action<'a> {
    Proxy(&'a CStr),
    WindowSize(i32, i32),
    PrintConfig,
    BadViewport(&'a CStr),
}

pub struct Options<'a> {
    pub actions: Vec<Action<'a>>,
    pub gsk_renderer_override: Option<&'a CStr>,
    pub private_window: bool,
    pub url: Option<&'a CStr>,
    pub dump: u32,
    pub out_path: Option<&'a CStr>,
    pub viewport_width: i32,
    pub viewport_height: i32,
    pub settle_ms: i32,
    pub time_ms: i32,
    pub debug: Option<&'a CStr>,
    pub script: Option<&'a CStr>,
    pub eval: Option<&'a CStr>,
    pub inspect: Option<&'a CStr>,
    pub inspect_at: Option<&'a CStr>,
    pub wpt: bool,
    pub wpt_timeout_ms: i32,
}

fn dump_mode<'a>(v: &'a CStr, o: &mut Options<'a>) {
    let b = v.to_bytes();
    let plain = match b {
        b"text" => Some(DUMP_TEXT),
        b"dom" => Some(DUMP_DOM),
        b"layout" => Some(DUMP_LAYOUT),
        b"none" => Some(DUMP_NONE),
        _ => None,
    };
    if let Some(mode) = plain {
        o.dump = mode;
    } else if let Some(path) = rest(v, b"png:") {
        o.dump = DUMP_PNG;
        o.out_path = Some(path);
    } else if let Some(path) = rest(v, b"pdf:") {
        o.dump = DUMP_PDF;
        o.out_path = Some(path);
    } else if let Some(path) = rest(v, b"print:") {
        o.dump = DUMP_PRINT;
        o.out_path = Some(path);
    }
}

pub fn parse<'a>(args: &[&'a CStr]) -> Options<'a> {
    let mut o = Options {
        actions: Vec::new(),
        gsk_renderer_override: None,
        private_window: false,
        url: None,
        dump: DUMP_TEXT,
        out_path: None,
        viewport_width: 1000,
        viewport_height: 0,
        settle_ms: 200,
        time_ms: 1000,
        debug: None,
        script: None,
        eval: None,
        inspect: None,
        inspect_at: None,
        wpt: false,
        wpt_timeout_ms: 0,
    };
    let mut dump_set = false;
    for &a in args.iter().skip(1) {
        if let Some(v) = rest(a, b"--proxy=") {
            o.actions.push(Action::Proxy(v));
        } else if let Some(v) = rest(a, b"--gsk-renderer=") {
            o.gsk_renderer_override = Some(v);
        } else if is(a, b"--private") {
            o.private_window = true;
        } else if let Some(v) = rest(a, b"--window-size=") {
            if let Some((w, h)) = size_pair(v.to_bytes()) {
                o.actions.push(Action::WindowSize(w as i32, h as i32));
            }
        } else if is(a, b"--print-config") {
            o.actions.push(Action::PrintConfig);
            break;
        } else if is(a, b"--headless") {
        } else if let Some(v) = rest(a, b"--dump=") {
            dump_set = true;
            dump_mode(v, &mut o);
        } else if let Some(v) = rest(a, b"--viewport=") {
            match viewport(v.to_bytes()) {
                Some((w, h)) => {
                    o.viewport_width = w as i32;
                    if h > 0 {
                        o.viewport_height = h as i32;
                    }
                }
                None => {
                    o.actions.push(Action::BadViewport(v));
                    break;
                }
            }
        } else if let Some(v) = rest(a, b"--viewport-height=") {
            if let Some(n) = bounded(v.to_bytes(), 1, 100_000) {
                o.viewport_height = n;
            }
        } else if let Some(v) = rest(a, b"--settle-ms=") {
            if let Some(n) = bounded(v.to_bytes(), 0, 600_000) {
                o.settle_ms = n;
            }
        } else if let Some(v) = rest(a, b"--time-ms=") {
            if let Some(n) = bounded(v.to_bytes(), 0, 600_000) {
                o.time_ms = n;
            }
        } else if is(a, b"--wpt") {
            o.wpt = true;
        } else if let Some(v) = rest(a, b"--wpt-timeout-ms=") {
            if let Some(n) = bounded(v.to_bytes(), 1, 600_000) {
                o.wpt_timeout_ms = n;
            }
        } else if let Some(v) = rest(a, b"--act=") {
            o.script = Some(v);
        } else if let Some(v) = rest(a, b"--eval=") {
            o.eval = Some(v);
        } else if let Some(v) = rest(a, b"--inspect=") {
            o.inspect = Some(v);
        } else if let Some(v) = rest(a, b"--inspect-at=") {
            o.inspect_at = Some(v);
        } else if is(a, b"--debug") {
            o.debug = Some(c"all");
        } else if let Some(v) = rest(a, b"--debug=") {
            o.debug = Some(v);
        } else if let Some(v) = rest(a, b"--url=") {
            o.url = Some(v);
        } else if a.to_bytes().first() != Some(&b'-') && o.url.is_none() {
            o.url = Some(a);
        }
    }
    if !dump_set && (o.inspect.is_some() || o.inspect_at.is_some() || o.wpt) {
        o.dump = DUMP_NONE;
    }
    o
}
