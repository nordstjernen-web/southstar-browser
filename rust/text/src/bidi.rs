//! Southstar text — bidirectional text: character directions, a paragraph's base direction and its embedding levels.
//! Copyright 2002, 2008 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use southstar_bidi::{BidiClass, ParagraphDirection, bracket, class, resolve_paragraph};

use crate::types::Direction;

pub fn log2vis_fill_embedding_levels(
    text: &str,
    embedding_levels: &mut [u8],
    base_dir: &mut Direction,
) {
    let n_chars = embedding_levels.len();
    let direction = match *base_dir {
        Direction::Ltr | Direction::TtbRtl => ParagraphDirection::Ltr,
        Direction::Rtl | Direction::TtbLtr => ParagraphDirection::Rtl,
        Direction::WeakRtl => ParagraphDirection::WeakRtl,
        Direction::WeakLtr | Direction::Neutral => ParagraphDirection::WeakLtr,
    };

    let mut classes = Vec::with_capacity(n_chars);
    let mut brackets = Vec::with_capacity(n_chars);
    let mut any_isolate = false;
    let mut any_rtl = false;
    let mut any_number = false;
    let mut any_letter = false;
    let mut all_strong_rtl = true;

    for character in text.chars().take(n_chars) {
        let ch = u32::from(character);
        let char_class = class(ch);
        classes.push(char_class);
        brackets.push(if char_class == BidiClass::ON {
            bracket(ch)
        } else {
            None
        });
        any_isolate |= char_class.is_isolate_control();
        any_rtl |= char_class.is_rtl()
            || matches!(
                char_class,
                BidiClass::AN | BidiClass::RLE | BidiClass::RLO | BidiClass::RLI
            );
        any_number |= char_class.is_number();
        any_letter |= char_class.is_strong();
        if char_class.is_strong() || char_class.is_embedding_or_override() {
            all_strong_rtl &=
                char_class.is_rtl() || matches!(char_class, BidiClass::RLE | BidiClass::RLO);
        }
    }
    classes.resize(n_chars, BidiClass::ON);
    brackets.resize(n_chars, None);

    let weak = matches!(
        direction,
        ParagraphDirection::WeakLtr | ParagraphDirection::WeakRtl
    );
    let rtl = matches!(
        direction,
        ParagraphDirection::Rtl | ParagraphDirection::WeakRtl
    );
    let base_level = if !any_isolate && !any_rtl && (!rtl || (weak && any_letter)) {
        embedding_levels.fill(0);
        0
    } else if !any_isolate && !any_number && all_strong_rtl && (rtl || (weak && any_letter)) {
        embedding_levels.fill(1);
        1
    } else {
        resolve_paragraph(&classes, &brackets, direction, embedding_levels).base_level
    };

    *base_dir = if base_level == 0 {
        Direction::Ltr
    } else {
        Direction::Rtl
    };
}

pub fn unichar_direction(ch: u32) -> Direction {
    let char_class = class(ch);
    if !char_class.is_strong() && !char_class.is_embedding_or_override() {
        Direction::Neutral
    } else if char_class.is_rtl() || matches!(char_class, BidiClass::RLE | BidiClass::RLO) {
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
