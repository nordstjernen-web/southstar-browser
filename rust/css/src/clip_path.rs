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

fn len_text(w: &[u8]) -> Option<String> {
    if is_math(w) {
        return Some(String::from_utf8_lossy(&w.to_ascii_lowercase()).into_owned());
    }
    let (v, unit) = length(w)?;
    if unit == NUMBER {
        return (v == 0.0).then(|| "0px".to_string());
    }
    Some(String::from_utf8_lossy(&w.to_ascii_lowercase()).into_owned())
}

fn is_zero(text: &str) -> bool {
    let digits = text.trim_end_matches(|c: char| c.is_ascii_alphabetic() || c == '%');
    !text.contains('(') && digits.parse::<f64>().is_ok_and(|v| v == 0.0)
}

fn box_collapse(mut v: Vec<String>) -> Vec<String> {
    loop {
        let redundant = match v.len() {
            4 => v[3] == v[1],
            3 => v[2] == v[0],
            2 => v[1] == v[0],
            _ => false,
        };
        if !redundant {
            return v;
        }
        v.pop();
    }
}

fn box_expand(v: &[String]) -> [String; 4] {
    match v.len() {
        1 => [v[0].clone(), v[0].clone(), v[0].clone(), v[0].clone()],
        2 => [v[0].clone(), v[1].clone(), v[0].clone(), v[1].clone()],
        3 => [v[0].clone(), v[1].clone(), v[2].clone(), v[1].clone()],
        _ => [v[0].clone(), v[1].clone(), v[2].clone(), v[3].clone()],
    }
}

fn radius_text(ws: &[&[u8]]) -> Option<String> {
    let halves = split_top_words(ws);
    let mut lists = Vec::new();
    for half in &halves {
        if !(1..=4).contains(&half.len()) || !half.iter().all(|w| length_percentage(w, true, true))
        {
            return None;
        }
        lists.push(
            half.iter()
                .map(|w| len_text(w))
                .collect::<Option<Vec<_>>>()?,
        );
    }
    let horizontal = box_expand(&lists[0]);
    let vertical = lists
        .get(1)
        .map_or_else(|| horizontal.clone(), |v| box_expand(v));
    if horizontal.iter().chain(&vertical).all(|t| is_zero(t)) {
        return Some(String::new());
    }
    let mut out = box_collapse(horizontal.to_vec()).join(" ");
    if vertical != horizontal {
        out.push_str(" / ");
        out.push_str(&box_collapse(vertical.to_vec()).join(" "));
    }
    Some(out)
}

fn split_round<'a>(ws: &'a [&'a [u8]]) -> Option<(&'a [&'a [u8]], String)> {
    match ws.iter().position(|w| eq(w, b"round")) {
        Some(i) => Some((&ws[..i], radius_text(&ws[i + 1..])?)),
        None => Some((ws, String::new())),
    }
}

fn with_radius(mut out: String, radius: &str) -> String {
    if !radius.is_empty() {
        out.push_str(" round ");
        out.push_str(radius);
    }
    out
}

fn position_text(ws: &[&[u8]]) -> Option<String> {
    let joined = ws.join(&b' ');
    let canon = crate::position::canonical_ex(&joined, true, false)?;
    let words: Vec<String> = words(&canon)
        .iter()
        .map(|w| {
            len_text(w)
                .unwrap_or_else(|| String::from_utf8_lossy(&w.to_ascii_lowercase()).into_owned())
        })
        .collect();
    Some(words.join(" "))
}

fn inset_text(args: &[u8]) -> Option<String> {
    let ws = words(args);
    let (edges, radius) = split_round(&ws)?;
    if !(1..=4).contains(&edges.len()) {
        return None;
    }
    let edges = edges
        .iter()
        .map(|w| len_text(w))
        .collect::<Option<Vec<_>>>()?;
    Some(with_radius(format!("inset({}", box_collapse(edges).join(" ")), &radius) + ")")
}

fn radius_word(w: &[u8]) -> Option<String> {
    if eq(w, b"closest-side") || eq(w, b"farthest-side") {
        return Some(String::from_utf8_lossy(&w.to_ascii_lowercase()).into_owned());
    }
    length_percentage(w, true, true)
        .then(|| len_text(w))
        .flatten()
}

fn circle_or_ellipse_text(name: &str, args: &[u8], radii: usize) -> Option<String> {
    let ws = words(args);
    let (sizes, at) = match ws.iter().position(|w| eq(w, b"at")) {
        Some(i) => (&ws[..i], Some(&ws[i + 1..])),
        None => (&ws[..], None),
    };
    if !(sizes.is_empty() || sizes.len() == radii) {
        return None;
    }
    let sizes = sizes
        .iter()
        .map(|w| radius_word(w))
        .collect::<Option<Vec<_>>>()?;
    let mut parts: Vec<String> = Vec::new();
    if !sizes.iter().all(|s| s == "closest-side") {
        parts.extend(sizes);
    }
    if let Some(at) = at {
        parts.push(format!("at {}", position_text(at)?));
    }
    Some(format!("{name}({})", parts.join(" ")))
}

fn polygon_text(args: &[u8]) -> Option<String> {
    let mut items = split_top(args, |c| c == b',');
    let mut prelude: Vec<String> = Vec::new();
    if let Some(Some(ok)) = items.first().map(|i| polygon_prelude(i)) {
        if !ok {
            return None;
        }
        let ws = words(items[0]);
        for (i, w) in ws.iter().enumerate() {
            if eq(w, b"evenodd") {
                prelude.push("evenodd".into());
            } else if eq(w, b"round") {
                let r = len_text(ws.get(i + 1)?)?;
                if !is_zero(&r) {
                    prelude.push(format!("round {r}"));
                }
            }
        }
        items.remove(0);
    }
    let mut points = Vec::new();
    for item in &items {
        let ws = words(item);
        if ws.len() != 2 {
            return None;
        }
        points.push(format!("{} {}", len_text(ws[0])?, len_text(ws[1])?));
    }
    if points.is_empty() {
        return None;
    }
    let mut out = String::from("polygon(");
    if !prelude.is_empty() {
        out.push_str(&prelude.join(" "));
        out.push_str(", ");
    }
    out.push_str(&points.join(", "));
    out.push(')');
    Some(out)
}

fn path_number(s: &[u8], p: &mut usize) -> Option<f64> {
    let start = *p;
    if matches!(s.get(*p), Some(b'+' | b'-')) {
        *p += 1;
    }
    let mut digits = 0;
    while s.get(*p).is_some_and(u8::is_ascii_digit) {
        *p += 1;
        digits += 1;
    }
    if s.get(*p) == Some(&b'.') {
        *p += 1;
        while s.get(*p).is_some_and(u8::is_ascii_digit) {
            *p += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        *p = start;
        return None;
    }
    if matches!(s.get(*p), Some(b'e' | b'E')) {
        let save = *p;
        *p += 1;
        if matches!(s.get(*p), Some(b'+' | b'-')) {
            *p += 1;
        }
        if !s.get(*p).is_some_and(u8::is_ascii_digit) {
            *p = save;
        }
        while s.get(*p).is_some_and(u8::is_ascii_digit) {
            *p += 1;
        }
    }
    core::str::from_utf8(&s[start..*p]).ok()?.parse().ok()
}

fn path_data_text(data: &[u8]) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    let mut p = 0;
    let skip = |p: &mut usize| {
        while data
            .get(*p)
            .is_some_and(|c| c.is_ascii_whitespace() || *c == b',')
        {
            *p += 1;
        }
    };
    skip(&mut p);
    if !matches!(data.get(p), Some(b'M' | b'm')) {
        return None;
    }
    while p < data.len() {
        let c = data[p];
        if b"MmLlHhVvCcSsQqTtAaZz".contains(&c) {
            out.push(if c == b'z' {
                "Z".into()
            } else {
                (c as char).to_string()
            });
            p += 1;
        } else {
            let n = path_number(data, &mut p)?;
            out.push(southstar_numfmt::g(n));
        }
        skip(&mut p);
    }
    Some(out.join(" "))
}

fn path_text(args: &[u8]) -> Option<String> {
    let items = split_top(args, |c| c == b',');
    let (rule, data) = match items.as_slice() {
        [data] => (None, *data),
        [rule, data] if fill_rule(rule) => (Some(rule.trim_ascii()), *data),
        _ => return None,
    };
    let data = data.trim_ascii();
    if data.len() < 2 || !matches!(data[0], b'"' | b'\'') || data[data.len() - 1] != data[0] {
        return None;
    }
    let normalized = path_data_text(&data[1..data.len() - 1])?;
    let mut out = String::from("path(");
    if rule.is_some_and(|r| eq(r, b"evenodd")) {
        out.push_str("evenodd, ");
    }
    out.push('"');
    out.push_str(&normalized);
    out.push_str("\")");
    Some(out)
}

fn four_text(name: &str, args: &[u8], auto_ok: bool, non_negative_from: usize) -> Option<String> {
    let ws = words(args);
    let (values, radius) = split_round(&ws)?;
    if values.len() != 4 {
        return None;
    }
    let mut parts = Vec::new();
    for (i, w) in values.iter().enumerate() {
        if auto_ok && eq(w, b"auto") {
            parts.push("auto".to_string());
        } else if length_percentage(w, i >= non_negative_from, true) {
            parts.push(len_text(w)?);
        } else {
            return None;
        }
    }
    Some(with_radius(format!("{name}({}", parts.join(" ")), &radius) + ")")
}

fn coordinate_pair(ws: &[&[u8]]) -> Option<String> {
    match ws {
        [x, y] if length_percentage(x, false, true) && length_percentage(y, false, true) => {
            Some(format!("{} {}", len_text(x)?, len_text(y)?))
        }
        _ => None,
    }
}

fn shape_point(by: bool, ws: &[&[u8]]) -> Option<String> {
    if by {
        coordinate_pair(ws)
    } else {
        position(ws).then(|| position_text(ws)).flatten()
    }
}

fn control_point(ws: &[&[u8]]) -> Option<String> {
    if let Some(i) = ws.iter().position(|w| eq(w, b"from")) {
        let anchor = ws.get(i + 1)?;
        if i + 2 != ws.len()
            || !(eq(anchor, b"start") || eq(anchor, b"end") || eq(anchor, b"origin"))
        {
            return None;
        }
        let pair = coordinate_pair(&ws[..i])?;
        return Some(format!(
            "{pair} from {}",
            String::from_utf8_lossy(&anchor.to_ascii_lowercase())
        ));
    }
    position(ws).then(|| position_text(ws)).flatten()
}

fn shape_command(item: &[u8]) -> Option<String> {
    let ws = words(item);
    let (cmd, rest) = ws.split_first()?;
    let cmd = String::from_utf8_lossy(&cmd.to_ascii_lowercase()).into_owned();
    if cmd == "close" {
        return rest.is_empty().then_some(cmd);
    }
    let (mode, rest) = rest.split_first()?;
    let by = if eq(mode, b"by") {
        true
    } else if eq(mode, b"to") {
        false
    } else {
        return None;
    };
    let mode = if by { "by" } else { "to" };
    match cmd.as_str() {
        "move" | "line" => Some(format!("{cmd} {mode} {}", shape_point(by, rest)?)),
        "hline" | "vline" => {
            let [w] = rest else { return None };
            let keywords: &[&[u8]] = if cmd == "hline" {
                &[b"left", b"center", b"right", b"x-start", b"x-end"]
            } else {
                &[b"top", b"center", b"bottom", b"y-start", b"y-end"]
            };
            let value = if !by && keywords.iter().any(|k| eq(k, w)) {
                String::from_utf8_lossy(&w.to_ascii_lowercase()).into_owned()
            } else if length_percentage(w, false, true) {
                len_text(w)?
            } else {
                return None;
            };
            Some(format!("{cmd} {mode} {value}"))
        }
        "curve" | "smooth" => {
            let with = rest.iter().position(|w| eq(w, b"with"));
            let end = shape_point(by, &rest[..with.unwrap_or(rest.len())])?;
            let mut out = format!("{cmd} {mode} {end}");
            match with {
                Some(i) => {
                    let controls = split_top_words(&rest[i + 1..]);
                    let max = if cmd == "curve" { 2 } else { 1 };
                    if controls.is_empty() || controls.len() > max {
                        return None;
                    }
                    let controls = controls
                        .iter()
                        .map(|c| control_point(c))
                        .collect::<Option<Vec<_>>>()?;
                    out.push_str(" with ");
                    out.push_str(&controls.join(" / "));
                }
                None if cmd == "curve" => return None,
                None => {}
            }
            Some(out)
        }
        "arc" => {
            let of = rest.iter().position(|w| eq(w, b"of"))?;
            let end = shape_point(by, &rest[..of])?;
            let tail = &rest[of + 1..];
            let mut radii = Vec::new();
            let mut i = 0;
            while i < tail.len() && radii.len() < 2 && length_percentage(tail[i], false, true) {
                radii.push(len_text(tail[i])?);
                i += 1;
            }
            if radii.is_empty() {
                return None;
            }
            let (mut sweep, mut size, mut rotate) = (None, None, None);
            while i < tail.len() {
                let w = tail[i];
                if (eq(w, b"cw") || eq(w, b"ccw")) && sweep.is_none() {
                    sweep = Some(w.to_ascii_lowercase());
                } else if (eq(w, b"large") || eq(w, b"small")) && size.is_none() {
                    size = Some(w.to_ascii_lowercase());
                } else if eq(w, b"rotate") && rotate.is_none() {
                    i += 1;
                    let angle = tail.get(i)?;
                    let deg = crate::transform::parse_angle_any(angle)?;
                    rotate = Some((
                        deg,
                        String::from_utf8_lossy(&angle.to_ascii_lowercase()).into_owned(),
                    ));
                } else {
                    return None;
                }
                i += 1;
            }
            let mut out = format!("arc {mode} {end} of {}", radii.join(" "));
            if let Some(s) = sweep.filter(|s| s.as_slice() == b"cw") {
                out.push(' ');
                out.push_str(&String::from_utf8_lossy(&s));
            }
            if let Some(s) = size.filter(|s| s.as_slice() == b"large") {
                out.push(' ');
                out.push_str(&String::from_utf8_lossy(&s));
            }
            if let Some((_, text)) = rotate.filter(|(deg, _)| *deg != 0.0) {
                out.push_str(" rotate ");
                out.push_str(&text);
            }
            Some(out)
        }
        _ => None,
    }
}

fn shape_text(args: &[u8]) -> Option<String> {
    let items = split_top(args, |c| c == b',');
    let (first, commands) = items.split_first()?;
    let ws = words(first);
    let (rule, rest) = match ws.split_first() {
        Some((rule, rest)) if fill_rule(rule) => (Some(rule), rest),
        _ => (None, &ws[..]),
    };
    let (from, point) = rest.split_first()?;
    if !eq(from, b"from") || commands.is_empty() {
        return None;
    }
    let mut out = String::from("shape(");
    if rule.is_some_and(|r| eq(r, b"evenodd")) {
        out.push_str("evenodd ");
    }
    out.push_str("from ");
    out.push_str(&shape_point(false, point)?);
    for command in commands {
        out.push_str(", ");
        out.push_str(&shape_command(command)?);
    }
    out.push(')');
    Some(out)
}

fn basic_shape_text(token: &[u8]) -> Option<String> {
    if let Some(a) = function(token, b"inset") {
        return inset_text(a);
    }
    if let Some(a) = function(token, b"circle") {
        return circle_or_ellipse_text("circle", a, 1);
    }
    if let Some(a) = function(token, b"ellipse") {
        return circle_or_ellipse_text("ellipse", a, 2);
    }
    if let Some(a) = function(token, b"polygon") {
        return polygon_text(a);
    }
    if let Some(a) = function(token, b"path") {
        return path_text(a);
    }
    if let Some(a) = function(token, b"xywh") {
        return four_text("xywh", a, false, 2);
    }
    if let Some(a) = function(token, b"rect") {
        return four_text("rect", a, true, 4);
    }
    function(token, b"shape").and_then(shape_text)
}

pub(crate) fn canonical(t: &[u8], default_box: &[u8]) -> Option<Vec<u8>> {
    let ws = words(t);
    let lower = |w: &[u8]| String::from_utf8_lossy(&w.to_ascii_lowercase()).into_owned();
    let is_box = |w: &[u8]| GEOMETRY_BOXES.iter().any(|b| eq(b, w));
    let text = match ws.as_slice() {
        [w] if eq(w, b"none") => "none".to_string(),
        [w] if is_box(w) => lower(w),
        [w] => basic_shape_text(w)?,
        [a, b] => {
            let (shape, geometry) = if is_box(a) { (b, a) } else { (a, b) };
            if !is_box(geometry) {
                return None;
            }
            let mut text = basic_shape_text(shape)?;
            if !eq(geometry, default_box) {
                text.push(' ');
                text.push_str(&lower(geometry));
            }
            text
        }
        _ => return None,
    };
    Some(text.into_bytes())
}

pub(crate) fn is_shape_text(t: &[u8]) -> bool {
    words(t).iter().any(|w| {
        [
            &b"inset"[..],
            b"circle",
            b"ellipse",
            b"polygon",
            b"path",
            b"xywh",
            b"rect",
            b"shape",
        ]
        .iter()
        .any(|name| function(w, name).is_some())
    })
}
