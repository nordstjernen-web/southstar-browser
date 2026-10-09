//! Southstar — CSS easing functions: the keywords, linear(), steps() and cubic-bezier() parsed into the ns_css_timing css.h declares, and their canonical text.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::ffi;
use crate::scan::{starts_with_ci, strip, strtol10};
use crate::units::number_text;

pub(crate) const LINEAR: i32 = 0;
pub(crate) const EASE: i32 = 1;
pub(crate) const EASE_IN: i32 = 2;
pub(crate) const EASE_OUT: i32 = 3;
pub(crate) const EASE_IN_OUT: i32 = 4;
pub(crate) const STEPS: i32 = 5;
pub(crate) const CUBIC: i32 = 6;

pub(crate) const JUMP_END: i32 = 0;
pub(crate) const JUMP_START: i32 = 1;
pub(crate) const JUMP_NONE: i32 = 2;
pub(crate) const JUMP_BOTH: i32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq)]
pub(crate) struct Timing {
    pub kind: i32,
    pub steps: i32,
    pub step_pos: i32,
    pub jump_keyword: i32,
    pub cb: [f64; 4],
}

#[cfg(target_pointer_width = "64")]
const _: () = assert!(core::mem::size_of::<Timing>() == 48);

impl Timing {
    pub(crate) fn of(kind: i32) -> Timing {
        Timing {
            kind,
            ..Timing::default()
        }
    }
}

const KEYWORDS: [(&[u8], i32); 5] = [
    (b"linear", LINEAR),
    (b"ease", EASE),
    (b"ease-in", EASE_IN),
    (b"ease-out", EASE_OUT),
    (b"ease-in-out", EASE_IN_OUT),
];

pub(crate) fn keyword_matches(kw: &[u8]) -> bool {
    KEYWORDS
        .iter()
        .any(|(name, _)| kw.eq_ignore_ascii_case(name))
        || kw.eq_ignore_ascii_case(b"step-start")
        || kw.eq_ignore_ascii_case(b"step-end")
}

fn keyword(kw: &[u8]) -> Timing {
    if let Some((_, kind)) = KEYWORDS
        .iter()
        .find(|(name, _)| kw.eq_ignore_ascii_case(name))
    {
        return Timing::of(*kind);
    }
    let step_pos = if kw.eq_ignore_ascii_case(b"step-start") {
        JUMP_START
    } else if kw.eq_ignore_ascii_case(b"step-end") {
        JUMP_END
    } else {
        return Timing::of(EASE);
    };
    Timing {
        kind: STEPS,
        steps: 1,
        step_pos,
        ..Timing::default()
    }
}

fn split_commas(body: &[u8]) -> Vec<&[u8]> {
    if body.is_empty() {
        return Vec::new();
    }
    body.split(|&c| c == b',').collect()
}

fn steps(parts: &[&[u8]]) -> Option<Timing> {
    let count = strip(parts[0]);
    let (n, end) = strtol10(count);
    if end == 0 || end != count.len() || n < 1 {
        return None;
    }
    let mut pos = JUMP_END;
    let mut jump_keyword = false;
    if let Some(k) = parts.get(1) {
        let k = strip(k);
        jump_keyword = starts_with_ci(k, b"jump-");
        pos = if k.eq_ignore_ascii_case(b"jump-start") || k.eq_ignore_ascii_case(b"start") {
            JUMP_START
        } else if k.eq_ignore_ascii_case(b"jump-end") || k.eq_ignore_ascii_case(b"end") {
            JUMP_END
        } else if k.eq_ignore_ascii_case(b"jump-none") {
            JUMP_NONE
        } else if k.eq_ignore_ascii_case(b"jump-both") {
            JUMP_BOTH
        } else {
            return None;
        };
    }
    if pos == JUMP_NONE && n < 2 {
        return None;
    }
    Some(Timing {
        kind: STEPS,
        steps: n as i32,
        step_pos: pos,
        jump_keyword: i32::from(jump_keyword),
        cb: [0.0; 4],
    })
}

fn cubic(parts: &[&[u8]]) -> Option<Timing> {
    let mut cb = [0.0f64; 4];
    for (slot, part) in cb.iter_mut().zip(parts) {
        let text = CString::new(strip(part)).ok()?;
        let (v, end) = ffi::strtod(&text, 0);
        if end == 0 || end != text.to_bytes().len() {
            return None;
        }
        *slot = v;
    }
    if cb[0] < 0.0 || cb[0] > 1.0 || cb[2] < 0.0 || cb[2] > 1.0 {
        return None;
    }
    Some(Timing {
        cb,
        ..Timing::of(CUBIC)
    })
}

pub(crate) fn item_parse(item: &[u8]) -> Option<Timing> {
    let item = &item[..item.iter().position(|&c| c == 0).unwrap_or(item.len())];
    let last = *item.last()?;
    if keyword_matches(item) {
        return Some(keyword(item));
    }
    if starts_with_ci(item, b"linear(") && last == b')' {
        return Some(Timing::of(LINEAR));
    }
    let is_steps = starts_with_ci(item, b"steps(");
    let is_cubic = starts_with_ci(item, b"cubic-bezier(");
    if (!is_steps && !is_cubic) || last != b')' {
        return None;
    }
    let body = &item[if is_steps { 6 } else { 13 }..item.len() - 1];
    let parts = split_commas(body);
    match parts.len() {
        1 | 2 if is_steps => steps(&parts),
        4 if is_cubic => cubic(&parts),
        _ => None,
    }
}

pub(crate) fn parse(text: &[u8]) -> Option<Timing> {
    item_parse(strip(text))
}

pub(crate) fn serialize(t: Option<&Timing>) -> Vec<u8> {
    let Some(t) = t else {
        return b"ease".to_vec();
    };
    match t.kind {
        LINEAR => b"linear".to_vec(),
        EASE_IN => b"ease-in".to_vec(),
        EASE_OUT => b"ease-out".to_vec(),
        EASE_IN_OUT => b"ease-in-out".to_vec(),
        STEPS => {
            let suffix: &str = match t.step_pos {
                JUMP_START if t.jump_keyword != 0 => ", jump-start",
                JUMP_START => ", start",
                JUMP_NONE => ", jump-none",
                JUMP_BOTH => ", jump-both",
                _ => "",
            };
            format!("steps({}{suffix})", t.steps).into_bytes()
        }
        CUBIC => {
            let mut out = b"cubic-bezier(".to_vec();
            for (i, v) in t.cb.iter().enumerate() {
                if i > 0 {
                    out.extend_from_slice(b", ");
                }
                out.extend_from_slice(&number_text(*v));
            }
            out.push(b')');
            out
        }
        _ => b"ease".to_vec(),
    }
}
