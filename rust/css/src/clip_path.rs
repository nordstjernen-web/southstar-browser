//! Southstar — validating clip-path values: none, url(), and a basic shape with or without a geometry box.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::property::{eq, length};
use crate::units::NUMBER;

const GEOMETRY_BOXES: [&[u8]; 7] = [
    b"border-box",
    b"padding-box",
    b"content-box",
    b"margin-box",
    b"fill-box",
    b"stroke-box",
    b"view-box",
];
const MATH_FUNCTIONS: [&[u8]; 10] = [
    b"calc(", b"min(", b"max(", b"clamp(", b"abs(", b"sign(", b"round(", b"mod(", b"rem(", b"var(",
];

fn split_top(s: &[u8], at: impl Fn(u8) -> bool) -> Vec<&[u8]> {
    let mut parts = Vec::new();
    let (mut depth, mut quote, mut start) = (0i32, 0u8, 0usize);
    for (i, &c) in s.iter().enumerate() {
        if quote != 0 {
            if c == quote {
                quote = 0;
            }
            continue;
        }
        match c {
            b'"' | b'\'' => quote = c,
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

fn function<'a>(token: &'a [u8], name: &[u8]) -> Option<&'a [u8]> {
    let open = token.iter().position(|&c| c == b'(')?;
    (eq(&token[..open], name) && token.ends_with(b")")).then(|| &token[open + 1..token.len() - 1])
}

fn is_math(w: &[u8]) -> bool {
    MATH_FUNCTIONS
        .iter()
        .any(|f| w.len() > f.len() && eq(&w[..f.len()], f))
}

fn length_percentage(w: &[u8], non_negative: bool, percent_ok: bool) -> bool {
    if is_math(w) {
        return true;
    }
    match length(w) {
        Some((v, unit)) if unit == NUMBER => v == 0.0,
        Some((v, unit)) => {
            (!non_negative || v >= 0.0) && (percent_ok || unit != crate::units::PERCENT)
        }
        None => false,
    }
}

fn position(ws: &[&[u8]]) -> bool {
    (1..=4).contains(&ws.len())
        && ws.iter().all(|w| {
            length_percentage(w, false, true)
                || [&b"left"[..], b"right", b"top", b"bottom", b"center"]
                    .iter()
                    .any(|k| eq(k, w))
        })
}

fn radius_list(ws: &[&[u8]]) -> bool {
    let halves = split_top_words(ws);
    halves.len() <= 2
        && halves.iter().all(|h| {
            (1..=4).contains(&h.len()) && h.iter().all(|w| length_percentage(w, true, true))
        })
}

fn split_top_words<'a>(ws: &[&'a [u8]]) -> Vec<Vec<&'a [u8]>> {
    let mut out = vec![Vec::new()];
    for w in ws {
        if *w == b"/" {
            out.push(Vec::new());
        } else if let Some(last) = out.last_mut() {
            last.push(*w);
        }
    }
    out
}

fn with_round<'a>(ws: &'a [&'a [u8]]) -> Option<&'a [&'a [u8]]> {
    match ws.iter().position(|w| eq(w, b"round")) {
        Some(i) => radius_list(&ws[i + 1..]).then_some(&ws[..i]),
        None => Some(ws),
    }
}

fn inset(args: &[u8]) -> bool {
    let ws = words(args);
    with_round(&ws).is_some_and(|edges| {
        (1..=4).contains(&edges.len()) && edges.iter().all(|w| length_percentage(w, false, true))
    })
}

fn shape_radius(w: &[u8]) -> bool {
    eq(w, b"closest-side") || eq(w, b"farthest-side") || length_percentage(w, true, true)
}

fn circle_or_ellipse(args: &[u8], radii: usize) -> bool {
    let ws = words(args);
    let (sizes, at) = match ws.iter().position(|w| eq(w, b"at")) {
        Some(i) => (&ws[..i], Some(&ws[i + 1..])),
        None => (&ws[..], None),
    };
    (sizes.is_empty() || sizes.len() == radii)
        && sizes.iter().all(|w| shape_radius(w))
        && at.is_none_or(position)
}

fn fill_rule(item: &[u8]) -> bool {
    eq(item.trim_ascii(), b"nonzero") || eq(item.trim_ascii(), b"evenodd")
}

fn polygon_prelude(item: &[u8]) -> Option<bool> {
    let ws = words(item);
    let rest = match ws.split_first() {
        Some((rule, rest)) if fill_rule(rule) => rest,
        _ => &ws[..],
    };
    match rest {
        [] => Some(true),
        [r, len] if eq(r, b"round") => Some(length_percentage(len, true, false)),
        _ if rest.len() == ws.len() && rest.iter().any(|w| eq(w, b"round")) => Some(false),
        _ if rest.len() == ws.len() => None,
        _ => Some(false),
    }
}

fn polygon(args: &[u8]) -> bool {
    let mut items = split_top(args, |c| c == b',');
    match items.first().map(|i| polygon_prelude(i)) {
        Some(Some(false)) => return false,
        Some(Some(true)) => {
            items.remove(0);
        }
        _ => {}
    }
    !items.is_empty()
        && items.iter().all(|i| {
            let ws = words(i);
            ws.len() == 2 && ws.iter().all(|w| length_percentage(w, false, true))
        })
}

fn path(args: &[u8]) -> bool {
    let items = split_top(args, |c| c == b',');
    let data = match items.as_slice() {
        [data] => data,
        [rule, data] if fill_rule(rule) => data,
        _ => return false,
    };
    let data = data.trim_ascii();
    data.len() >= 2
        && matches!(data[0], b'"' | b'\'')
        && data[data.len() - 1] == data[0]
        && data[1..data.len() - 1]
            .iter()
            .any(|c| !c.is_ascii_whitespace())
}

fn xywh(args: &[u8]) -> bool {
    let ws = words(args);
    with_round(&ws).is_some_and(|v| {
        v.len() == 4
            && v.iter()
                .enumerate()
                .all(|(i, w)| length_percentage(w, i >= 2, true))
    })
}

fn rect(args: &[u8]) -> bool {
    let ws = words(args);
    with_round(&ws).is_some_and(|v| {
        v.len() == 4
            && v.iter()
                .all(|w| eq(w, b"auto") || length_percentage(w, false, true))
    })
}

fn basic_shape(token: &[u8]) -> bool {
    if let Some(a) = function(token, b"inset") {
        return inset(a);
    }
    if let Some(a) = function(token, b"circle") {
        return circle_or_ellipse(a, 1);
    }
    if let Some(a) = function(token, b"ellipse") {
        return circle_or_ellipse(a, 2);
    }
    if let Some(a) = function(token, b"polygon") {
        return polygon(a);
    }
    if let Some(a) = function(token, b"path") {
        return path(a);
    }
    if let Some(a) = function(token, b"xywh") {
        return xywh(a);
    }
    if let Some(a) = function(token, b"rect") {
        return rect(a);
    }
    function(token, b"shape").is_some_and(|a| words(a).iter().any(|w| eq(w, b"from")))
}

pub(crate) fn valid(t: &[u8]) -> bool {
    let ws = words(t);
    if let [w] = ws.as_slice()
        && (eq(w, b"none")
            || function(w, b"url").is_some()
            || (w.len() > 4 && eq(&w[..4], b"url(")))
    {
        return true;
    }
    let is_box = |w: &[u8]| GEOMETRY_BOXES.iter().any(|b| eq(b, w));
    match ws.as_slice() {
        [w] => is_box(w) || basic_shape(w),
        [a, b] => (is_box(a) && basic_shape(b)) || (basic_shape(a) && is_box(b)),
        _ => false,
    }
}
