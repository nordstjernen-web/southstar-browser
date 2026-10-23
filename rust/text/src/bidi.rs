//! Southstar text — bidirectional text: character directions, a paragraph's base direction and its embedding levels.
//! Copyright 2002, 2008 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::ffi::fribidi;
use crate::types::Direction;

const MASK_RTL: u32 = 0x0000_0001;
const MASK_ARABIC: u32 = 0x0000_0002;
const MASK_STRONG: u32 = 0x0000_0010;
const MASK_WEAK: u32 = 0x0000_0020;
const MASK_NEUTRAL: u32 = 0x0000_0040;
const MASK_LETTER: u32 = 0x0000_0100;
const MASK_NUMBER: u32 = 0x0000_0200;
const MASK_EXPLICIT: u32 = 0x0000_1000;
const MASK_ISOLATE: u32 = 0x0000_8000;

const TYPE_ON: u32 = MASK_NEUTRAL;
const TYPE_RLE: u32 = MASK_STRONG | MASK_EXPLICIT | MASK_RTL;

const PAR_LTR: u32 = MASK_STRONG | MASK_LETTER;
const PAR_RTL: u32 = MASK_STRONG | MASK_LETTER | MASK_RTL;
const PAR_WLTR: u32 = MASK_WEAK;
const PAR_WRTL: u32 = MASK_WEAK | MASK_RTL;

const NO_BRACKET: u32 = 0;

fn is_rtl(bidi_type: u32) -> bool {
    bidi_type & MASK_RTL != 0
}

fn is_strong(bidi_type: u32) -> bool {
    bidi_type & MASK_STRONG != 0
}

fn is_weak(bidi_type: u32) -> bool {
    bidi_type & MASK_WEAK != 0
}

fn is_letter(bidi_type: u32) -> bool {
    bidi_type & MASK_LETTER != 0
}

fn is_number(bidi_type: u32) -> bool {
    bidi_type & MASK_NUMBER != 0
}

fn is_arabic(bidi_type: u32) -> bool {
    bidi_type & MASK_ARABIC != 0
}

fn is_isolate(bidi_type: u32) -> bool {
    bidi_type & MASK_ISOLATE != 0
}

pub fn log2vis_fill_embedding_levels(
    text: &str,
    embedding_levels: &mut [u8],
    base_dir: &mut Direction,
) {
    let n_chars = embedding_levels.len();
    let mut fribidi_base_dir = match *base_dir {
        Direction::Ltr | Direction::TtbRtl => PAR_LTR,
        Direction::Rtl | Direction::TtbLtr => PAR_RTL,
        Direction::WeakRtl => PAR_WRTL,
        Direction::WeakLtr | Direction::Neutral => PAR_WLTR,
    };

    let mut bidi_types = vec![0u32; n_chars];
    let mut bracket_types = vec![NO_BRACKET; n_chars];
    let mut ored_types = 0u32;
    let mut anded_strongs = TYPE_RLE;

    for (i, character) in text.chars().take(n_chars).enumerate() {
        let ch = u32::from(character);
        let char_type = fribidi::bidi_type(ch);
        bidi_types[i] = char_type;
        ored_types |= char_type;
        if is_strong(char_type) {
            anded_strongs &= char_type;
        }
        if char_type == TYPE_ON {
            bracket_types[i] = fribidi::bracket(ch);
        }
    }

    if !is_isolate(ored_types)
        && !is_rtl(ored_types)
        && !is_arabic(ored_types)
        && (!is_rtl(fribidi_base_dir) || (is_weak(fribidi_base_dir) && is_letter(ored_types)))
    {
        fribidi_base_dir = PAR_LTR;
        embedding_levels.fill(0);
    } else if !is_isolate(ored_types)
        && !is_number(ored_types)
        && is_rtl(anded_strongs)
        && (is_rtl(fribidi_base_dir) || (is_weak(fribidi_base_dir) && is_letter(ored_types)))
    {
        fribidi_base_dir = PAR_RTL;
        embedding_levels.fill(1);
    } else {
        let max_level = fribidi::par_embedding_levels(
            &bidi_types,
            &bracket_types,
            &mut fribidi_base_dir,
            embedding_levels,
        );
        if max_level == 0 {
            embedding_levels.fill(0);
        }
    }

    *base_dir = if fribidi_base_dir == PAR_LTR {
        Direction::Ltr
    } else {
        Direction::Rtl
    };
}

pub fn unichar_direction(ch: u32) -> Direction {
    let char_type = fribidi::bidi_type(ch);
    if !is_strong(char_type) {
        Direction::Neutral
    } else if is_rtl(char_type) {
        Direction::Rtl
    } else {
        Direction::Ltr
    }
}

pub fn find_base_dir(text: &str) -> Direction {
    let mut dir = Direction::Neutral;
    for character in text.chars() {
        if character == '\0' {
            break;
        }
        dir = unichar_direction(u32::from(character));
        if dir != Direction::Neutral {
            break;
        }
    }
    dir
}
