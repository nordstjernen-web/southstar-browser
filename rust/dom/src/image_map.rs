//! Southstar — client-side image maps: the area of a usemap under a point and its link.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use crate::ffi::Node;
use crate::{MAX_DEPTH, children};

const MAX_COORDS: usize = 64;

fn parse_coords(s: &[u8]) -> Vec<f64> {
    let mut out = Vec::new();
    let mut p = 0;
    while p < s.len() && out.len() < MAX_COORDS {
        p += s[p..]
            .iter()
            .take_while(|&&c| matches!(c, b',' | b' ' | b'\t' | b'\n' | b'\r'))
            .count();
        if p >= s.len() {
            break;
        }
        let (value, consumed) = southstar_glib::ascii_strtod_prefix(&s[p..]);
        if consumed == 0 {
            break;
        }
        out.push(value);
        p += consumed;
    }
    out
}

fn point_in_poly(pts: &[f64], pairs: usize, x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = pairs.wrapping_sub(1);
    for i in 0..pairs {
        let (xi, yi) = (pts[2 * i], pts[2 * i + 1]);
        let (xj, yj) = (pts[2 * j], pts[2 * j + 1]);
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn c_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn c_max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn shape_prefix(shape: &[u8], prefix: &[u8]) -> bool {
    shape.len() >= prefix.len() && shape[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn area_hit(
    shape: Option<&CStr>,
    coords: Option<&CStr>,
    lx: f64,
    ly: f64,
    iw: f64,
    ih: f64,
) -> bool {
    let shape = shape.map(CStr::to_bytes);
    if shape.is_some_and(|s| s.eq_ignore_ascii_case(b"default")) {
        return lx >= 0.0 && ly >= 0.0 && lx <= iw && ly <= ih;
    }
    let c = coords.map_or_else(Vec::new, |coords| parse_coords(coords.to_bytes()));
    if shape.is_some_and(|s| shape_prefix(s, b"circ")) {
        if c.len() < 3 {
            return false;
        }
        let (dx, dy) = (lx - c[0], ly - c[1]);
        return dx * dx + dy * dy <= c[2] * c[2];
    }
    if shape.is_some_and(|s| shape_prefix(s, b"poly")) {
        return c.len() >= 6 && point_in_poly(&c, c.len() / 2, lx, ly);
    }
    if c.len() < 4 {
        return false;
    }
    let (x1, x2) = (c_min(c[0], c[2]), c_max(c[0], c[2]));
    let (y1, y2) = (c_min(c[1], c[3]), c_max(c[1], c[3]));
    lx >= x1 && lx <= x2 && ly >= y1 && ly <= y2
}

fn find_map<'a>(node: Node<'a>, name: &[u8], depth: i32) -> Option<Node<'a>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    if node.element_name() == Some(b"map") {
        let named = |attr: &CStr| node.attr(attr).is_some_and(|v| v.to_bytes() == name);
        if named(c"name") || named(c"id") {
            return Some(node);
        }
    }
    children(node).find_map(|child| find_map(child, name, depth + 1))
}

fn first_area(node: Node<'_>, point: (f64, f64), size: (f64, f64), depth: i32) -> Option<Node<'_>> {
    if depth >= MAX_DEPTH {
        return None;
    }
    children(node).find_map(|child| {
        let hit = child.element_name() == Some(b"area")
            && area_hit(
                child.attr(c"shape"),
                child.attr(c"coords"),
                point.0,
                point.1,
                size.0,
                size.1,
            );
        if hit {
            Some(child)
        } else {
            first_area(child, point, size, depth + 1)
        }
    })
}

pub fn resolve<'a>(
    doc: Node<'a>,
    usemap: &CStr,
    point: (f64, f64),
    size: (f64, f64),
) -> Option<(&'a CStr, Option<&'a CStr>)> {
    if point.0 < 0.0 || point.1 < 0.0 {
        return None;
    }
    let usemap = usemap.to_bytes();
    let name = usemap.strip_prefix(b"#").unwrap_or(usemap);
    if name.is_empty() {
        return None;
    }
    let map = find_map(doc, name, 0)?;
    let area = first_area(map, point, size, 0)?;
    let href = area.attr(c"href").filter(|href| !href.is_empty())?;
    Some((href, area.attr(c"target")))
}
