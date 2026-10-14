//! Southstar — building the clip-path basic shapes (inset, circle, ellipse, polygon, xywh, rect) against a box's border box.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::f64::consts::{PI, SQRT_2};

use southstar_layout::BoxRef;

use crate::ffi::cairo::{self, Cr};
use crate::radii::{CornerRadii, rounded_rect_path};

const REM_PX: f64 = 16.0;

#[derive(Clone, Copy)]
struct Rect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn split_top(s: &[u8], at: impl Fn(u8) -> bool) -> Vec<&[u8]> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0usize);
    for (i, &c) in s.iter().enumerate() {
        match c {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ if depth == 0 && at(c) => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

fn words(s: &[u8]) -> Vec<&[u8]> {
    split_top(s, |c| c.is_ascii_whitespace())
        .into_iter()
        .filter(|w| !w.is_empty())
        .collect()
}

fn length(token: &[u8], basis: f64, font_size: f64) -> Option<f64> {
    let digits = token
        .iter()
        .position(|c| !(c.is_ascii_digit() || matches!(c, b'.' | b'-' | b'+' | b'e' | b'E')))
        .unwrap_or(token.len());
    let digits = if digits > 0 && matches!(token[digits - 1], b'e' | b'E') {
        digits - 1
    } else {
        digits
    };
    let v: f64 = core::str::from_utf8(&token[..digits]).ok()?.parse().ok()?;
    let unit = &token[digits..];
    Some(match unit {
        b"" | b"px" => v,
        b"%" => v / 100.0 * basis,
        b"em" => v * font_size,
        b"rem" => v * REM_PX,
        b"pt" => v * 4.0 / 3.0,
        b"in" => v * 96.0,
        b"cm" => v * 96.0 / 2.54,
        b"mm" => v * 96.0 / 25.4,
        _ => return None,
    })
}

fn position(ws: &[&[u8]], r: Rect, font_size: f64) -> (f64, f64) {
    let (mut x, mut y) = (r.w / 2.0, r.h / 2.0);
    let keyword = |w: &[u8]| match w {
        b"left" => Some((true, 0.0)),
        b"right" => Some((true, 1.0)),
        b"top" => Some((false, 0.0)),
        b"bottom" => Some((false, 1.0)),
        b"center" => None,
        _ => None,
    };
    let mut horizontal_next = true;
    for w in ws.iter().take(2) {
        if let Some((horizontal, f)) = keyword(w) {
            if horizontal {
                x = f * r.w;
            } else {
                y = f * r.h;
            }
            horizontal_next = !horizontal;
        } else if *w == b"center" {
            horizontal_next = !horizontal_next;
        } else if horizontal_next {
            x = length(w, r.w, font_size).unwrap_or(x);
            horizontal_next = false;
        } else {
            y = length(w, r.h, font_size).unwrap_or(y);
        }
    }
    (r.x + x, r.y + y)
}

fn split_at_keyword<'a>(ws: &'a [&'a [u8]], kw: &[u8]) -> (&'a [&'a [u8]], &'a [&'a [u8]]) {
    match ws.iter().position(|w| *w == kw) {
        Some(i) => (&ws[..i], &ws[i + 1..]),
        None => (ws, &[]),
    }
}

fn shape_radius(w: &[u8], closest: f64, farthest: f64, basis: f64, font_size: f64) -> f64 {
    match w {
        b"closest-side" => closest,
        b"farthest-side" => farthest,
        _ => length(w, basis, font_size).unwrap_or(closest),
    }
}

fn circle(cr: Cr, args: &[u8], r: Rect, font_size: f64) {
    let ws = words(args);
    let (radius, at) = split_at_keyword(&ws, b"at");
    let (cx, cy) = position(at, r, font_size);
    let (dx0, dx1, dy0, dy1) = (cx - r.x, r.x + r.w - cx, cy - r.y, r.y + r.h - cy);
    let closest = dx0.min(dx1).min(dy0).min(dy1).max(0.0);
    let farthest = dx0.max(dx1).max(dy0).max(dy1);
    let basis = (r.w * r.w + r.h * r.h).sqrt() / SQRT_2;
    let rad = radius.first().map_or(closest, |w| {
        shape_radius(w, closest, farthest, basis, font_size)
    });
    cr.new_sub_path();
    cr.arc(cx, cy, rad.max(0.0), 0.0, 2.0 * PI);
}

fn ellipse(cr: Cr, args: &[u8], r: Rect, font_size: f64) {
    let ws = words(args);
    let (radii, at) = split_at_keyword(&ws, b"at");
    let (cx, cy) = position(at, r, font_size);
    let (dx0, dx1, dy0, dy1) = (cx - r.x, r.x + r.w - cx, cy - r.y, r.y + r.h - cy);
    let rx = radii.first().map_or(dx0.min(dx1), |w| {
        shape_radius(w, dx0.min(dx1), dx0.max(dx1), r.w, font_size)
    });
    let ry = radii.get(1).map_or(dy0.min(dy1), |w| {
        shape_radius(w, dy0.min(dy1), dy0.max(dy1), r.h, font_size)
    });
    if rx <= 0.0 || ry <= 0.0 {
        cr.rectangle(cx, cy, 0.0, 0.0);
        return;
    }
    cr.save();
    cr.translate(cx, cy);
    cr.scale(rx, ry);
    cr.new_sub_path();
    cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
    cr.restore();
}

fn round_radii(ws: &[&[u8]], r: Rect, font_size: f64) -> CornerRadii {
    let first: Vec<&[u8]> = ws.iter().take_while(|w| **w != b"/").copied().collect();
    let v = |i: usize, basis: f64| {
        let n = first.len();
        let pick = match (n, i) {
            (0, _) => return 0.0,
            (1, _) => 0,
            (2, 0 | 2) | (3, 0) => 0,
            (2, _) | (3, 1 | 3) => 1,
            (3, 2) => 2,
            _ => i,
        };
        length(first[pick], basis, font_size).unwrap_or(0.0)
    };
    let mut c = CornerRadii::uniform(0.0);
    (c.tl, c.tr, c.br, c.bl) = (v(0, r.w), v(1, r.w), v(2, r.w), v(3, r.w));
    (c.tlv, c.trv, c.brv, c.blv) = (v(0, r.h), v(1, r.h), v(2, r.h), v(3, r.h));
    c
}

fn inset(cr: Cr, args: &[u8], r: Rect, font_size: f64) {
    let ws = words(args);
    let (edges, round) = split_at_keyword(&ws, b"round");
    let e = |i: usize, basis: f64| {
        let pick = match (edges.len(), i) {
            (1, _) => 0,
            (2, 0 | 2) | (3, 0) => 0,
            (2, _) | (3, 1 | 3) => 1,
            (3, 2) => 2,
            _ => i,
        };
        edges
            .get(pick)
            .and_then(|w| length(w, basis, font_size))
            .unwrap_or(0.0)
    };
    let (top, right, bottom, left) = (e(0, r.h), e(1, r.w), e(2, r.h), e(3, r.w));
    let inner = Rect {
        x: r.x + left,
        y: r.y + top,
        w: (r.w - left - right).max(0.0),
        h: (r.h - top - bottom).max(0.0),
    };
    rounded_rect_path(
        cr,
        inner.x,
        inner.y,
        inner.w,
        inner.h,
        round_radii(round, inner, font_size),
    );
}

fn xywh(cr: Cr, args: &[u8], r: Rect, font_size: f64) {
    let ws = words(args);
    let (v, round) = split_at_keyword(&ws, b"round");
    let get = |i: usize, basis: f64| {
        v.get(i)
            .and_then(|w| length(w, basis, font_size))
            .unwrap_or(0.0)
    };
    let inner = Rect {
        x: r.x + get(0, r.w),
        y: r.y + get(1, r.h),
        w: get(2, r.w).max(0.0),
        h: get(3, r.h).max(0.0),
    };
    rounded_rect_path(
        cr,
        inner.x,
        inner.y,
        inner.w,
        inner.h,
        round_radii(round, inner, font_size),
    );
}

fn rect(cr: Cr, args: &[u8], r: Rect, font_size: f64) {
    let ws = words(args);
    let (v, round) = split_at_keyword(&ws, b"round");
    let get = |i: usize, basis: f64, auto: f64| {
        v.get(i)
            .filter(|w| **w != b"auto")
            .and_then(|w| length(w, basis, font_size))
            .unwrap_or(auto)
    };
    let (top, right, bottom, left) = (
        get(0, r.h, 0.0),
        get(1, r.w, r.w),
        get(2, r.h, r.h),
        get(3, r.w, 0.0),
    );
    let inner = Rect {
        x: r.x + left,
        y: r.y + top,
        w: (right - left).max(0.0),
        h: (bottom - top).max(0.0),
    };
    rounded_rect_path(
        cr,
        inner.x,
        inner.y,
        inner.w,
        inner.h,
        round_radii(round, inner, font_size),
    );
}

fn polygon(cr: Cr, args: &[u8], r: Rect, font_size: f64) -> bool {
    let mut items = split_top(args, |c| c == b',');
    let mut even_odd = false;
    if let Some(first) = items.first() {
        let ws = words(first);
        let prelude = ws.iter().all(|w| {
            matches!(*w, b"nonzero" | b"evenodd" | b"round") || ws.contains(&&b"round"[..])
        });
        if prelude
            && ws
                .iter()
                .any(|w| matches!(*w, b"nonzero" | b"evenodd" | b"round"))
        {
            even_odd = ws.contains(&&b"evenodd"[..]);
            items.remove(0);
        }
    }
    let mut first = true;
    cr.new_sub_path();
    for item in items {
        let ws = words(item);
        let (Some(x), Some(y)) = (
            ws.first().and_then(|w| length(w, r.w, font_size)),
            ws.get(1).and_then(|w| length(w, r.h, font_size)),
        ) else {
            continue;
        };
        if first {
            cr.move_to(r.x + x, r.y + y);
            first = false;
        } else {
            cr.line_to(r.x + x, r.y + y);
        }
    }
    if first {
        return false;
    }
    cr.close_path();
    if even_odd {
        cr.set_fill_rule(cairo::FILL_RULE_EVEN_ODD);
    }
    true
}

fn reference_box(b: BoxRef<'_>, geometry_box: Option<&[u8]>) -> Rect {
    let (m, bd, p) = (b.margin(), b.border(), b.padding());
    let (cw, ch) = (b.content_width(), b.content_height());
    let border = Rect {
        x: b.x() + m.left,
        y: b.y() + m.top,
        w: cw + p.left + p.right + bd.left + bd.right,
        h: ch + p.top + p.bottom + bd.top + bd.bottom,
    };
    match geometry_box {
        Some(b"padding-box") => Rect {
            x: border.x + bd.left,
            y: border.y + bd.top,
            w: cw + p.left + p.right,
            h: ch + p.top + p.bottom,
        },
        Some(b"content-box") => Rect {
            x: border.x + bd.left + p.left,
            y: border.y + bd.top + p.top,
            w: cw,
            h: ch,
        },
        Some(b"margin-box") => Rect {
            x: b.x(),
            y: b.y(),
            w: border.w + m.left + m.right,
            h: border.h + m.top + m.bottom,
        },
        _ => border,
    }
}

pub fn clip_to_shape(cr: Cr, b: BoxRef<'_>, value: &[u8], font_size: f64) -> bool {
    let ws = words(value);
    let is_box = |w: &&[u8]| w.ends_with(b"-box");
    let geometry_box = ws.iter().find(|w| is_box(w)).copied();
    let r = reference_box(b, geometry_box);
    if r.w.is_nan() || r.h.is_nan() {
        return false;
    }
    let Some(shape) = ws.iter().find(|w| !is_box(w)) else {
        cr.rectangle(r.x, r.y, r.w, r.h);
        cr.clip();
        return true;
    };
    let Some(open) = shape.iter().position(|&c| c == b'(') else {
        return false;
    };
    let Some(args) = shape[open + 1..].strip_suffix(b")") else {
        return false;
    };
    match &shape[..open] {
        b"circle" => circle(cr, args, r, font_size),
        b"ellipse" => ellipse(cr, args, r, font_size),
        b"inset" => inset(cr, args, r, font_size),
        b"xywh" => xywh(cr, args, r, font_size),
        b"rect" => rect(cr, args, r, font_size),
        b"polygon" => {
            if !polygon(cr, args, r, font_size) {
                return false;
            }
        }
        _ => return false,
    }
    cr.clip();
    cr.set_fill_rule(cairo::FILL_RULE_WINDING);
    true
}
