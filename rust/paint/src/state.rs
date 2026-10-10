//! Southstar — the state a paint pass reads besides the box tree: the page's animations and script context, the find-in-page highlight, the caret blink and the NS_DBG_PAINT_AT and NS_DBG_COMPOSITE debug switches.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cell::Cell;
use core::ffi::c_void;
use core::ptr;

use southstar_layout::NsBox;

struct PaintState {
    anim: Cell<*mut c_void>,
    js: Cell<*mut c_void>,
    search_case_sensitive: Cell<bool>,
    search_active: Cell<*const NsBox>,
    caret_visible: Cell<bool>,
    debug_point: Cell<Option<(i32, i32)>>,
    last_composite_log_us: Cell<i64>,
}

impl PaintState {
    fn new() -> PaintState {
        PaintState {
            anim: Cell::new(ptr::null_mut()),
            js: Cell::new(ptr::null_mut()),
            search_case_sensitive: Cell::new(false),
            search_active: Cell::new(ptr::null()),
            caret_visible: Cell::new(true),
            debug_point: Cell::new(None),
            last_composite_log_us: Cell::new(0),
        }
    }
}

thread_local! {
    static STATE: PaintState = PaintState::new();
}

pub fn anim() -> *mut c_void {
    STATE.with(|s| s.anim.get())
}

pub fn set_anim(anim: *mut c_void) {
    STATE.with(|s| s.anim.set(anim));
}

pub fn js() -> *mut c_void {
    STATE.with(|s| s.js.get())
}

pub fn set_js(js: *mut c_void) {
    STATE.with(|s| s.js.set(js));
}

pub fn set_search(case_sensitive: bool, active: *const NsBox) {
    STATE.with(|s| {
        s.search_case_sensitive.set(case_sensitive);
        s.search_active.set(active);
    });
}

pub fn search_case_sensitive() -> bool {
    STATE.with(|s| s.search_case_sensitive.get())
}

pub fn search_active() -> *const NsBox {
    STATE.with(|s| s.search_active.get())
}

pub fn caret_visible() -> bool {
    STATE.with(|s| s.caret_visible.get())
}

pub fn set_caret_visible(visible: bool) {
    STATE.with(|s| s.caret_visible.set(visible));
}

pub fn composite_log_due(now_us: i64) -> bool {
    STATE.with(|s| {
        if now_us - s.last_composite_log_us.get() > 1_000_000 {
            s.last_composite_log_us.set(now_us);
            true
        } else {
            false
        }
    })
}

fn parse_number(s: &[u8]) -> Option<(i32, usize)> {
    let mut p = 0;
    while p < s.len() && crate::util::is_space(s[p]) {
        p += 1;
    }
    let start = p;
    if p < s.len() && (s[p] == b'-' || s[p] == b'+') {
        p += 1;
    }
    let digits = p;
    while p < s.len() && s[p].is_ascii_digit() {
        p += 1;
    }
    if p == digits {
        return None;
    }
    let v = core::str::from_utf8(&s[start..p])
        .ok()?
        .parse::<i64>()
        .ok()?;
    Some((v as i32, p))
}

fn parse_point(text: &[u8]) -> (i32, i32) {
    let mut x = -1;
    let mut y = -1;
    if let Some((vx, n)) = parse_number(text) {
        x = vx;
        if text.get(n) == Some(&b',')
            && let Some((vy, _)) = parse_number(&text[n + 1..])
        {
            y = vy;
        }
    }
    (x, y)
}

pub fn debug_point() -> Option<(i32, i32)> {
    let point = STATE.with(|s| {
        if let Some(p) = s.debug_point.get() {
            return p;
        }
        let parsed = std::env::var_os("NS_DBG_PAINT_AT")
            .map_or((-1, -1), |v| parse_point(v.as_encoded_bytes()));
        s.debug_point.set(Some(parsed));
        parsed
    });
    (point.0 >= 0).then_some(point)
}
