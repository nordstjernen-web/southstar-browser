//! Southstar — the <area> of a client-side image map under a point in an image: the usemap's <map> in the image's tree scope, coordinates read as HTML's list of numbers, and circle, polygon, default and rectangle shapes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_dom::{Node, children};
use southstar_glib as glib;

const MAX_DEPTH: i32 = 512;
const SHADOW_ATTR: &CStr = c"data-nd-shadow-root";

fn min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn find_map<'a>(n: Node<'a>, name: &[u8], depth: i32) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if n.element_name() == Some(b"map") {
        let named = |attr: &CStr| n.attr(attr).is_some_and(|v| v.to_bytes() == name);
        if named(c"id") || named(c"name") {
            return Some(n);
        }
    }
    children(n)
        .filter(|c| c.is_element() && c.attr(SHADOW_ATTR).is_none())
        .find_map(|c| find_map(c, name, depth + 1))
}

fn map_for(img: Node<'_>) -> Option<Node<'_>> {
    let usemap = img.attr(c"usemap")?.to_bytes();
    let hash = usemap.iter().position(|&c| c == b'#')?;
    let name = &usemap[hash + 1..];
    if name.is_empty() {
        return None;
    }
    let mut scope = img;
    while let Some(parent) = scope.parent() {
        if scope.attr(SHADOW_ATTR).is_some() {
            break;
        }
        scope = parent;
    }
    find_map(scope, name, 0)
}

fn is_delimiter(c: u8) -> bool {
    matches!(c, b',' | b';' | b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn number(s: &[u8]) -> f64 {
    let mut p = 0;
    let mut sign = 1.0;
    if matches!(s.first(), Some(b'-' | b'+')) {
        if s[0] == b'-' {
            sign = -1.0;
        }
        p += 1;
    }
    let start = p;
    let digits = |from: usize| s[from..].iter().take_while(|c| c.is_ascii_digit()).count();
    p += digits(p);
    if s.get(p) == Some(&b'.') && s.get(p + 1).is_some_and(u8::is_ascii_digit) {
        p += 1;
        p += digits(p);
    }
    if p == start {
        return 0.0;
    }
    if matches!(s.get(p), Some(b'e' | b'E')) {
        let mut e = p + 1;
        if matches!(s.get(e), Some(b'-' | b'+')) {
            e += 1;
        }
        if s.get(e).is_some_and(u8::is_ascii_digit) {
            p = e + digits(e);
        }
    }
    let v = glib::ascii_strtod(&s[start..p]);
    if v.is_finite() { sign * v } else { 0.0 }
}

fn coords(s: &[u8]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut p = 0;
    let skip_delimiters = |p: &mut usize| {
        while *p < s.len() && is_delimiter(s[*p]) {
            *p += 1;
        }
    };
    skip_delimiters(&mut p);
    while p < s.len() {
        while p < s.len()
            && !is_delimiter(s[p])
            && !s[p].is_ascii_digit()
            && s[p] != b'.'
            && s[p] != b'-'
        {
            p += 1;
        }
        let start = p;
        while p < s.len() && !is_delimiter(s[p]) {
            p += 1;
        }
        out.push(number(&s[start..p]));
        skip_delimiters(&mut p);
    }
    out
}

fn polygon_contains(c: &[f64], x: f64, y: f64) -> bool {
    let points = c.len() / 2;
    let mut inside = false;
    let mut j = points - 1;
    for i in 0..points {
        let (xi, yi) = (c[2 * i], c[2 * i + 1]);
        let (xj, yj) = (c[2 * j], c[2 * j + 1]);
        let cross = (xj - xi) * (y - yi) - (yj - yi) * (x - xi);
        if cross.abs() < 1e-6
            && x >= min(xi, xj)
            && x <= max(xi, xj)
            && y >= min(yi, yj)
            && y <= max(yi, yj)
        {
            return true;
        }
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn area_contains(area: Node<'_>, x: f64, y: f64, width: f64, height: f64) -> bool {
    let shape = area.attr(c"shape").map(CStr::to_bytes);
    let c = coords(area.attr(c"coords").map_or(&[][..], CStr::to_bytes));
    let is =
        |names: &[&[u8]]| shape.is_some_and(|s| names.iter().any(|n| s.eq_ignore_ascii_case(n)));
    if is(&[b"circle", b"circ"]) {
        c.len() >= 3
            && c[2] > 0.0
            && (x - c[0]) * (x - c[0]) + (y - c[1]) * (y - c[1]) <= c[2] * c[2]
    } else if is(&[b"default"]) {
        x >= 0.0 && y >= 0.0 && x < width && y < height
    } else if is(&[b"poly", b"polygon"]) {
        c.len() >= 6 && polygon_contains(&c[..c.len() & !1], x, y)
    } else if c.len() >= 4 {
        let (x1, x2) = (min(c[0], c[2]), max(c[0], c[2]));
        let (y1, y2) = (min(c[1], c[3]), max(c[1], c[3]));
        x >= x1 && x < x2 && y >= y1 && y < y2
    } else {
        false
    }
}

fn area_in(n: Node<'_>, x: f64, y: f64, width: f64, height: f64, depth: i32) -> Option<Node<'_>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    children(n).filter(|c| c.is_element()).find_map(|c| {
        if c.element_name() == Some(b"area") && area_contains(c, x, y, width, height) {
            Some(c)
        } else {
            area_in(c, x, y, width, height, depth + 1)
        }
    })
}

pub fn area_at(img: Node<'_>, x: f64, y: f64, width: f64, height: f64) -> Option<Node<'_>> {
    if img.element_name() != Some(b"img") || img.attr(c"usemap").is_none() {
        return None;
    }
    area_in(map_for(img)?, x, y, width, height, 0)
}
