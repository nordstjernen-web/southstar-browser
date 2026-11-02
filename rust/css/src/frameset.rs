//! Southstar — frameset presentational hints: the grid tracks a frameset's rows and cols lists of dimensions stand for, and the frame borders frameborder and border turn off.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_dom::{Node, attrs, controls};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Dimension {
    Absolute(f64),
    Percentage(f64),
    Relative(f64),
}

fn digits(input: &[u8]) -> usize {
    input.iter().take_while(|c| c.is_ascii_digit()).count()
}

fn parse_dimension(token: &[u8]) -> Dimension {
    let mut rest = token.trim_ascii_start();
    let whole = digits(rest);
    let mut value = std::str::from_utf8(&rest[..whole])
        .ok()
        .and_then(|text| text.parse::<f64>().ok())
        .unwrap_or(0.0);
    let mut seen_digits = whole > 0;
    rest = &rest[whole..];
    if let [b'.', tail @ ..] = rest {
        let span = tail
            .iter()
            .take_while(|c| c.is_ascii_digit() || c.is_ascii_whitespace())
            .count();
        let mut scale = 0.1;
        for &c in tail[..span].iter().filter(|c| c.is_ascii_digit()) {
            value += f64::from(c - b'0') * scale;
            scale /= 10.0;
            seen_digits = true;
        }
        rest = &tail[span..];
    }
    match rest.trim_ascii_start().first() {
        Some(b'%') => Dimension::Percentage(value),
        Some(b'*') if !seen_digits => Dimension::Relative(1.0),
        Some(b'*') => Dimension::Relative(value),
        _ => Dimension::Absolute(value),
    }
}

pub(crate) fn parse_dimensions(input: &[u8]) -> Vec<Dimension> {
    let input = input.strip_suffix(b",").unwrap_or(input);
    if input.is_empty() {
        return Vec::new();
    }
    input.split(|&c| c == b',').map(parse_dimension).collect()
}

fn number(value: f64) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    format!("{rounded}")
}

pub(crate) fn grid_tracks(dimensions: &[Dimension]) -> Option<String> {
    if dimensions.is_empty() {
        return None;
    }
    let has_relative = dimensions
        .iter()
        .any(|d| matches!(d, Dimension::Relative(_)));
    let all_absolute = dimensions
        .iter()
        .all(|d| matches!(d, Dimension::Absolute(_)));
    let all_percentage = dimensions
        .iter()
        .all(|d| matches!(d, Dimension::Percentage(_)));
    let scale_to_fill = !has_relative && (all_absolute || all_percentage);
    let tracks: Vec<String> = dimensions
        .iter()
        .map(|&d| match d {
            Dimension::Absolute(v) | Dimension::Percentage(v) if scale_to_fill => {
                format!("{}fr", number(v))
            }
            Dimension::Absolute(v) => format!("{}px", number(v)),
            Dimension::Percentage(v) => format!("{}%", number(v)),
            Dimension::Relative(v) => format!("{}fr", number(v)),
        })
        .collect();
    Some(tracks.join(" "))
}

fn frameborder_off(el: Node<'_>) -> Option<bool> {
    let value = attrs::get(el, c"frameborder")?.to_bytes().trim_ascii();
    if value.eq_ignore_ascii_case(b"no") {
        return Some(true);
    }
    if value.eq_ignore_ascii_case(b"yes") {
        return Some(false);
    }
    let parsed = controls::parse_int(
        attrs::get(el, c"frameborder"),
        1,
        i32::MIN / 2,
        i32::MAX / 2,
    );
    Some(parsed == 0)
}

fn borders_off(frame: Node<'_>) -> bool {
    if let Some(off) = frameborder_off(frame) {
        return off;
    }
    let mut cursor = frame.parent();
    while let Some(set) = cursor.filter(|n| n.element_name() == Some(b"frameset")) {
        if let Some(off) = frameborder_off(set) {
            return off;
        }
        if attrs::get(set, c"border").is_some() {
            return controls::parse_int(attrs::get(set, c"border"), 1, 0, i32::MAX / 2) == 0;
        }
        cursor = set.parent();
    }
    false
}

pub(crate) fn frameset_hints(out: &mut Vec<u8>, el: Node<'_>) {
    for (attr, prop) in [
        (c"cols", "grid-template-columns"),
        (c"rows", "grid-template-rows"),
    ] {
        let tracks = attrs::get(el, attr)
            .map(|value| parse_dimensions(value.to_bytes()))
            .and_then(|dimensions| grid_tracks(&dimensions))
            .unwrap_or_else(|| "1fr".to_owned());
        out.extend_from_slice(format!("{prop}: {tracks};").as_bytes());
    }
}

pub(crate) fn frame_hints(out: &mut Vec<u8>, el: Node<'_>) {
    if borders_off(el) {
        out.extend_from_slice(b"border-width: 0;");
    }
}
