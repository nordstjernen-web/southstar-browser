//! Southstar — CSS scroll snapping: the snap positions a scroller's descendants offer through scroll-snap-align and scroll-margin, the snapport after scroll-padding, and the choice between them by travel direction and distance.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_layout::BoxRef;
use southstar_style::{PropId, StyleRef};

use crate::ffi;

#[derive(Default)]
struct Axis {
    best: f64,
    prev: f64,
    any: bool,
}

pub struct Port {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub cur_x: f64,
    pub cur_y: f64,
    pub prev_x: f64,
    pub prev_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

fn is_c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

fn scan_word(s: &[u8]) -> (&[u8], &[u8]) {
    let start = s.iter().position(|&b| !is_c_space(b)).unwrap_or(s.len());
    let s = &s[start..];
    let len = s.iter().take(15).take_while(|&&b| !is_c_space(b)).count();
    s.split_at(len)
}

fn scan_two_words(s: &[u8]) -> (&[u8], &[u8]) {
    let (first, rest) = scan_word(s);
    if first.is_empty() {
        return (first, &[]);
    }
    (first, scan_word(rest).0)
}

fn snap_length(s: Option<StyleRef<'_>>, prop: PropId, basis: f64) -> f64 {
    let Some(v) = s.and_then(|s| s.get(prop)) else {
        return 0.0;
    };
    if v.is_keyword(c"auto") {
        return 0.0;
    }
    let px = ffi::length_resolve(v, basis, 0.0);
    if px.is_finite() { px } else { 0.0 }
}

fn travel_ahead(travel: f64, candidate: f64, prev: f64) -> bool {
    if travel > 0.5 {
        candidate > prev + 0.5
    } else if travel < -0.5 {
        candidate < prev - 0.5
    } else {
        true
    }
}

fn better(axis: &Axis, candidate: f64, current: f64) -> bool {
    let travel = current - axis.prev;
    let ahead = travel_ahead(travel, candidate, axis.prev);
    let best_ahead = travel_ahead(travel, axis.best, axis.prev);
    if !axis.any {
        return true;
    }
    if ahead != best_ahead {
        return ahead;
    }
    (candidate - current).abs() < (axis.best - current).abs()
}

fn consider(axis: &mut Axis, candidate: f64, current: f64, max: f64) {
    let mut candidate = candidate;
    if candidate < 0.0 {
        candidate = 0.0;
    }
    if candidate > max {
        candidate = max;
    }
    if better(axis, candidate, current) {
        axis.best = candidate;
        axis.any = true;
    }
}

fn aligned(kw: &[u8], start: f64, end: f64, port_start: f64, port_end: f64) -> f64 {
    match kw {
        b"end" => end - port_end,
        b"center" => (start + end - port_start - port_end) / 2.0,
        _ => start - port_start,
    }
}

fn collect(b: BoxRef<'_>, port: &Port, ax: &mut Axis, ay: &mut Axis, want_x: bool, want_y: bool) {
    let port_w = port.right - port.left;
    let port_h = port.bottom - port.top;
    for c in southstar_layout::children(b) {
        let style = ffi::style_of(c);
        let align = style.and_then(|s| s.keyword_of(PropId::ScrollSnapAlign));
        if let Some(align) = align.map(|a| a.to_bytes()).filter(|a| *a != b"none none") {
            let (block, inline_kw) = scan_two_words(align);
            let (m, p, bd) = (c.margin(), c.padding(), c.border());
            let top = c.y() + m.top - snap_length(style, PropId::ScrollMarginTop, port_h);
            let bottom = c.y()
                + m.top
                + c.content_height()
                + p.top
                + p.bottom
                + bd.top
                + bd.bottom
                + snap_length(style, PropId::ScrollMarginBottom, port_h);
            let left = c.x() + m.left - snap_length(style, PropId::ScrollMarginLeft, port_w);
            let right = c.x()
                + m.left
                + c.content_width()
                + p.left
                + p.right
                + bd.left
                + bd.right
                + snap_length(style, PropId::ScrollMarginRight, port_w);
            if want_y && block != b"none" {
                consider(
                    ay,
                    aligned(block, top, bottom, port.top, port.bottom),
                    port.cur_y,
                    port.max_y,
                );
            }
            if want_x && inline_kw != b"none" {
                consider(
                    ax,
                    aligned(inline_kw, left, right, port.left, port.right),
                    port.cur_x,
                    port.max_x,
                );
            }
        }
        if c.scrolls() {
            continue;
        }
        collect(c, port, ax, ay, want_x, want_y);
    }
}

pub fn solve(root: BoxRef<'_>, kind: &[u8], port: &Port) -> Option<(Option<f64>, Option<f64>)> {
    let (axis_kw, strictness) = scan_two_words(kind);
    let want_x = matches!(axis_kw, b"x" | b"inline" | b"both");
    let want_y = matches!(axis_kw, b"y" | b"block" | b"both");
    if !want_x && !want_y {
        return None;
    }
    let mut ax = Axis {
        prev: port.prev_x,
        ..Axis::default()
    };
    let mut ay = Axis {
        prev: port.prev_y,
        ..Axis::default()
    };
    collect(root, port, &mut ax, &mut ay, want_x, want_y);
    let mandatory = strictness == b"mandatory";
    let port_w = port.right - port.left;
    let port_h = port.bottom - port.top;
    let y =
        (ay.any && (mandatory || (ay.best - port.cur_y).abs() <= port_h / 2.0)).then_some(ay.best);
    let x =
        (ax.any && (mandatory || (ax.best - port.cur_x).abs() <= port_w / 2.0)).then_some(ax.best);
    (x.is_some() || y.is_some()).then_some((x, y))
}

pub fn snap_type<'a>(s: StyleRef<'a>) -> Option<&'a [u8]> {
    s.keyword_of(PropId::ScrollSnapType)
        .map(|t| t.to_bytes())
        .filter(|t| *t != b"none")
}

pub fn scroller_port(scroller: BoxRef<'_>, s: StyleRef<'_>, prev_x: f64, prev_y: f64) -> Port {
    let (m, p, bd) = (scroller.margin(), scroller.padding(), scroller.border());
    let port_top = scroller.y() + m.top + bd.top;
    let port_left = scroller.x() + m.left + bd.left;
    let port_h = scroller.content_height() + p.top + p.bottom;
    let port_w = scroller.content_width() + p.left + p.right;
    let s = Some(s);
    Port {
        top: port_top + snap_length(s, PropId::ScrollPaddingTop, port_h),
        bottom: port_top + port_h - snap_length(s, PropId::ScrollPaddingBottom, port_h),
        left: port_left + snap_length(s, PropId::ScrollPaddingLeft, port_w),
        right: port_left + port_w - snap_length(s, PropId::ScrollPaddingRight, port_w),
        cur_x: scroller.scroll_x(),
        cur_y: scroller.scroll_y(),
        prev_x,
        prev_y,
        max_x: scroller.scroll_max_x(),
        max_y: scroller.scroll_max_y(),
    }
}

pub fn viewport_port(
    s: StyleRef<'_>,
    w: f64,
    h: f64,
    cur: (f64, f64),
    prev: (f64, f64),
    max: (f64, f64),
) -> Port {
    let s = Some(s);
    Port {
        top: snap_length(s, PropId::ScrollPaddingTop, h),
        bottom: h - snap_length(s, PropId::ScrollPaddingBottom, h),
        left: snap_length(s, PropId::ScrollPaddingLeft, w),
        right: w - snap_length(s, PropId::ScrollPaddingRight, w),
        cur_x: cur.0,
        cur_y: cur.1,
        prev_x: prev.0,
        prev_y: prev.1,
        max_x: max.0,
        max_y: max.1,
    }
}
