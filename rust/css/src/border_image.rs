//! Southstar — border-image: the slice, width, outset and repeat longhands in canonical form, the tokens of the shorthand, and the slice, widths, outsets and tiling painting reads from a computed style.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::scan::{is_ws, split_ws_limit};
use crate::units::{self, NUMBER, PERCENT, number_text};

pub(crate) const TILE_STRETCH: u32 = 0;
pub(crate) const TILE_REPEAT: u32 = 1;
pub(crate) const TILE_ROUND: u32 = 2;
pub(crate) const TILE_SPACE: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub(crate) struct Params {
    pub slice: [f64; 4],
    pub slice_percent: [i32; 4],
    pub fill: i32,
    pub width: [f64; 4],
    pub width_unit: [u32; 4],
    pub width_auto: [i32; 4],
    pub outset: [f64; 4],
    pub outset_unit: [u32; 4],
    pub tile: [u32; 2],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<Params>() == 176);

fn length(tok: &[u8]) -> Option<(f64, u32)> {
    units::parse_length(&CString::new(tok).ok()?)
}

fn quad_serialize(side: &[Vec<u8>; 4]) -> Vec<u8> {
    let parts: &[Vec<u8>] = if side[1] == side[3] {
        if side[0] == side[2] {
            if side[0] == side[1] {
                &side[..1]
            } else {
                &side[..2]
            }
        } else {
            &side[..3]
        }
    } else {
        &side[..]
    };
    parts.join(&b' ')
}

fn fill_sides(mut sides: Vec<Vec<u8>>) -> [Vec<u8>; 4] {
    let count = sides.len();
    if count < 2 {
        sides.push(sides[0].clone());
    }
    if count < 3 {
        sides.push(sides[0].clone());
    }
    if count < 4 {
        sides.push(sides[1].clone());
    }
    [
        sides[0].clone(),
        sides[1].clone(),
        sides[2].clone(),
        sides[3].clone(),
    ]
}

pub(crate) fn slice_canonical(t: &[u8]) -> Option<Vec<u8>> {
    let tokens = split_ws_limit(t, 6);
    if !(1..=5).contains(&tokens.len()) {
        return None;
    }
    let mut fill = false;
    let mut sides = Vec::new();
    for tok in tokens {
        if tok.eq_ignore_ascii_case(b"fill") {
            if fill {
                return None;
            }
            fill = true;
            continue;
        }
        if sides.len() >= 4 {
            return None;
        }
        let (num, unit) = length(tok)?;
        if num < 0.0 || (unit != NUMBER && unit != PERCENT) {
            return None;
        }
        let mut side = number_text(num);
        if unit == PERCENT {
            side.push(b'%');
        }
        sides.push(side);
    }
    if sides.is_empty() {
        return None;
    }
    let mut out = quad_serialize(&fill_sides(sides));
    if fill {
        out.extend_from_slice(b" fill");
    }
    Some(out)
}

pub(crate) fn length_serialize(
    token: &[u8],
    allow_auto: bool,
    allow_percent: bool,
) -> Option<Vec<u8>> {
    if allow_auto && token.eq_ignore_ascii_case(b"auto") {
        return Some(b"auto".to_vec());
    }
    let (num, unit) = length(token)?;
    if num < 0.0 || (unit == PERCENT && !allow_percent) {
        return None;
    }
    let mut out = number_text(num);
    if unit != NUMBER {
        out.extend_from_slice(units::unit_suffix(unit).to_bytes());
    }
    Some(out)
}

pub(crate) fn quad_canonical(t: &[u8], allow_auto: bool, allow_percent: bool) -> Option<Vec<u8>> {
    let tokens = split_ws_limit(t, 5);
    if !(1..=4).contains(&tokens.len()) {
        return None;
    }
    let sides = tokens
        .iter()
        .map(|tok| length_serialize(tok, allow_auto, allow_percent))
        .collect::<Option<Vec<_>>>()?;
    Some(quad_serialize(&fill_sides(sides)))
}

pub(crate) fn tile_keyword(token: &[u8]) -> bool {
    [&b"stretch"[..], b"repeat", b"round", b"space"]
        .iter()
        .any(|kw| token.eq_ignore_ascii_case(kw))
}

pub(crate) fn repeat_canonical(t: &[u8]) -> Option<Vec<u8>> {
    let tokens = split_ws_limit(t, 3);
    let n = tokens.len();
    if !(n == 1 || n == 2) || !tile_keyword(tokens[0]) || (n == 2 && !tile_keyword(tokens[1])) {
        return None;
    }
    let first = tokens[0].to_ascii_lowercase();
    let second = tokens[n - 1].to_ascii_lowercase();
    if first == second {
        return Some(first);
    }
    let mut out = first;
    out.push(b' ');
    out.extend_from_slice(&second);
    Some(out)
}

pub(crate) fn shorthand_tokens(text: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut depth = 0u32;
    let mut start: Option<usize> = None;
    for p in 0..=text.len() {
        let c = text.get(p).copied().unwrap_or(0);
        if c == b'(' {
            depth += 1;
        } else if c == b')' && depth > 0 {
            depth -= 1;
        }
        if c != 0 && (depth > 0 || (!is_ws(c) && c != b'/')) {
            start.get_or_insert(p);
            continue;
        }
        if let Some(s) = start.take() {
            out.push(text[s..p].to_vec());
        }
        if c == 0 {
            break;
        }
        if c == b'/' {
            out.push(b"/".to_vec());
        }
    }
    out
}

fn copy_from(i: usize, count: usize) -> usize {
    if i == 3 && count >= 2 { 1 } else { 0 }
}

pub(crate) fn params(slice: &[u8], width: &[u8], outset: &[u8], repeat: &[u8]) -> Params {
    let mut out = Params::default();
    let mut count = 0;
    for tok in split_ws_limit(slice, 5) {
        if tok.eq_ignore_ascii_case(b"fill") {
            out.fill = 1;
        } else if count < 4
            && let Some((num, unit)) = length(tok)
        {
            out.slice[count] = num;
            out.slice_percent[count] = i32::from(unit == PERCENT);
            count += 1;
        }
    }
    if count == 0 {
        out.slice[0] = 100.0;
        out.slice_percent[0] = 1;
        count = 1;
    }
    for i in count..4 {
        let src = copy_from(i, count);
        out.slice[i] = out.slice[src];
        out.slice_percent[i] = out.slice_percent[src];
    }

    count = 0;
    for tok in split_ws_limit(width, 4) {
        if count >= 4 {
            continue;
        }
        if tok.eq_ignore_ascii_case(b"auto") {
            out.width_auto[count] = 1;
            out.width_unit[count] = NUMBER;
            count += 1;
        } else if let Some((num, unit)) = length(tok) {
            out.width[count] = num;
            out.width_unit[count] = unit;
            count += 1;
        }
    }
    if count == 0 {
        out.width[0] = 1.0;
        out.width_unit[0] = NUMBER;
        count = 1;
    }
    for i in count..4 {
        let src = copy_from(i, count);
        out.width[i] = out.width[src];
        out.width_unit[i] = out.width_unit[src];
        out.width_auto[i] = out.width_auto[src];
    }

    count = 0;
    for tok in split_ws_limit(outset, 4) {
        if count < 4
            && let Some((num, unit)) = length(tok)
        {
            out.outset[count] = num;
            out.outset_unit[count] = unit;
            count += 1;
        }
    }
    for i in count..4 {
        let src = copy_from(i, count);
        out.outset[i] = out.outset[src];
        out.outset_unit[i] = out.outset_unit[src];
    }

    let tokens = split_ws_limit(repeat, 2);
    for (i, slot) in out.tile.iter_mut().enumerate() {
        let kw: &[u8] = tokens
            .get(i)
            .or(tokens.first())
            .copied()
            .unwrap_or(b"stretch");
        *slot = if kw.eq_ignore_ascii_case(b"repeat") {
            TILE_REPEAT
        } else if kw.eq_ignore_ascii_case(b"round") {
            TILE_ROUND
        } else if kw.eq_ignore_ascii_case(b"space") {
            TILE_SPACE
        } else {
            TILE_STRETCH
        };
    }
    out
}
