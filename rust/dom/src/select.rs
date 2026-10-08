//! Southstar — option text, labels and values and the option a select shows as chosen.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi::Node;
use crate::{MAX_DEPTH, children};

fn text_skipping_scripts(node: Node, out: &mut Vec<u8>, depth: i32) {
    if depth >= MAX_DEPTH {
        return;
    }
    for child in children(node) {
        if child.is_text() {
            if let Some(text) = child.text() {
                out.extend_from_slice(text.to_bytes());
            }
        } else if !child
            .element_name()
            .is_some_and(|name| name.eq_ignore_ascii_case(b"script"))
        {
            text_skipping_scripts(child, out, depth + 1);
        }
    }
}

fn utf8_skip(lead: u8) -> usize {
    match lead {
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf7 => 4,
        0xf8..=0xfb => 5,
        0xfc..=0xfd => 6,
        _ => 1,
    }
}

fn utf8_char(s: &[u8], at: usize) -> Option<u32> {
    let byte = |i: usize| s.get(i).copied().unwrap_or(0);
    let lead = byte(at);
    let (len, mask) = match lead {
        0..0x80 => (1, 0x7f),
        _ if lead & 0xe0 == 0xc0 => (2, 0x1f),
        _ if lead & 0xf0 == 0xe0 => (3, 0x0f),
        _ if lead & 0xf8 == 0xf0 => (4, 0x07),
        _ if lead & 0xfc == 0xf8 => (5, 0x03),
        _ if lead & 0xfe == 0xfc => (6, 0x01),
        _ => return None,
    };
    let mut result = u32::from(lead & mask);
    for i in 1..len {
        let next = byte(at + i);
        if next & 0xc0 != 0x80 {
            return None;
        }
        result = (result << 6) | u32::from(next & 0x3f);
    }
    Some(result)
}

fn strip_and_collapse(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut in_space = true;
    let mut p = 0;
    while p < s.len() {
        let next = (p + utf8_skip(s[p])).min(s.len());
        if matches!(utf8_char(s, p), Some(0x09 | 0x0a | 0x0c | 0x0d | 0x20)) {
            if !in_space {
                out.push(b' ');
                in_space = true;
            }
        } else {
            out.extend_from_slice(&s[p..next]);
            in_space = false;
        }
        p = next;
    }
    if out.last() == Some(&b' ') {
        out.pop();
    }
    out
}

pub fn option_text(option: Node) -> Vec<u8> {
    let mut raw = Vec::new();
    text_skipping_scripts(option, &mut raw, 0);
    strip_and_collapse(&raw)
}

pub fn option_label(option: Node) -> Vec<u8> {
    match option.attr(c"label") {
        Some(label) => label.to_bytes().to_vec(),
        None => option_text(option),
    }
}

pub fn option_value(option: Node) -> Vec<u8> {
    match option.attr(c"value") {
        Some(value) => value.to_bytes().to_vec(),
        None => option_text(option),
    }
}

fn is_selected_option(node: Node) -> bool {
    node.element_name() == Some(b"option") && node.attr(c"selected").is_some()
}

pub fn first_selected_option(select: Node<'_>) -> Option<Node<'_>> {
    let last_wins = select.attr(c"multiple").is_none();
    let mut found = None;
    for child in children(select) {
        let candidates: Vec<Node> = if child.element_name() == Some(b"optgroup") {
            children(child).collect()
        } else {
            vec![child]
        };
        for option in candidates.into_iter().filter(|n| is_selected_option(*n)) {
            if !last_wins {
                return Some(option);
            }
            found = Some(option);
        }
    }
    found
}

fn size_lists_rows(size: &[u8]) -> bool {
    let start = size
        .iter()
        .take_while(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r'))
        .count();
    let rest = &size[start..];
    let (negative, digits) = match rest.first() {
        Some(b'-') => (true, &rest[1..]),
        Some(b'+') => (false, &rest[1..]),
        _ => (false, rest),
    };
    let count = digits.iter().take_while(|c| c.is_ascii_digit()).count();
    let value = digits[..count].iter().fold(0u64, |v, &d| {
        v.saturating_mul(10).saturating_add(u64::from(d - b'0'))
    });
    count > 0 && !negative && value > 1
}

fn enabled_option(node: Node) -> bool {
    node.element_name() == Some(b"option") && node.attr(c"disabled").is_none()
}

pub fn chosen_option(select: Node<'_>) -> Option<Node<'_>> {
    if let Some(selected) = first_selected_option(select) {
        return Some(selected);
    }
    if select.attr(c"data-nd-noselect").is_some() || select.attr(c"multiple").is_some() {
        return None;
    }
    if select
        .attr(c"size")
        .is_some_and(|size| size_lists_rows(size.to_bytes()))
    {
        return None;
    }
    children(select).find_map(|child| {
        if child.element_name() == Some(b"optgroup") {
            if child.attr(c"disabled").is_some() {
                return None;
            }
            children(child).find(|option| enabled_option(*option))
        } else {
            enabled_option(child).then_some(child)
        }
    })
}
