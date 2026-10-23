//! Southstar text — the default break pass: UAX #29 grapheme, word and sentence boundaries and UAX #14 line breaks in one sweep.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use super::table;
use crate::emoji::{is_emoji_base_character, is_emoji_extended_pictographic};
use crate::script::Script;
use crate::types::LogAttr;
use crate::unicode::{self, BreakType as B, UnicodeType as T};

const PARAGRAPH_SEPARATOR: u32 = 0x2029;

fn in_ranges(ch: u32, ranges: &[(u32, u32)]) -> bool {
    ranges
        .binary_search_by(|&(start, end)| {
            if end < ch {
                core::cmp::Ordering::Less
            } else if start > ch {
                core::cmp::Ordering::Greater
            } else {
                core::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

fn is_sterm(ch: u32) -> bool {
    in_ranges(ch, table::STERM)
}

fn is_virama(ch: u32) -> bool {
    in_ranges(ch, table::VIRAMA)
}

fn is_vowel_dependent(ch: u32) -> bool {
    in_ranges(ch, table::VOWEL_DEPENDENT)
}

fn is_consonant_prefixed(ch: u32) -> bool {
    in_ranges(ch, table::CONSONANT_PREFIXED)
}

fn is_consonant_preceding_repha(ch: u32) -> bool {
    in_ranges(ch, table::CONSONANT_PRECEDING_REPHA)
}

fn is_indic_conjunct_break_linker(ch: u32) -> bool {
    in_ranges(ch, table::INDIC_CONJUNCT_BREAK_LINKER)
}

fn is_indic_conjunct_break_consonant(ch: u32) -> bool {
    in_ranges(ch, table::INDIC_CONJUNCT_BREAK_CONSONANT)
}

fn is_indic_conjunct_break_extend(ch: u32) -> bool {
    in_ranges(ch, table::INDIC_CONJUNCT_BREAK_EXTEND)
}

fn is_east_asian_wide(ch: u32) -> bool {
    in_ranges(ch, table::EAST_ASIAN_WIDE)
}

pub(super) fn line_break_class(ch: u32) -> B {
    if matches!(
        ch,
        0x058A
            | 0x05BE
            | 0x1400
            | 0x2010
            | 0x2012
            | 0x2013
            | 0x2E17
            | 0x2E40
            | 0x2E5D
            | 0x10D6E
            | 0x10EAD
    ) {
        B::UnambiguousHyphen
    } else {
        unicode::break_type(ch)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Jamo {
    L = 0,
    V = 1,
    T = 2,
    Lv = 3,
    Lvt = 4,
    None = 5,
}

const HANGUL_JAMO_PROPS: [(Jamo, Jamo); 6] = [
    (Jamo::L, Jamo::L),
    (Jamo::V, Jamo::V),
    (Jamo::T, Jamo::T),
    (Jamo::L, Jamo::V),
    (Jamo::L, Jamo::T),
    (Jamo::None, Jamo::None),
];

fn jamo_type(break_type: B) -> Jamo {
    match break_type {
        B::HangulLJamo => Jamo::L,
        B::HangulVJamo => Jamo::V,
        B::HangulTJamo => Jamo::T,
        B::HangulLvSyllable => Jamo::Lv,
        B::HangulLvtSyllable => Jamo::Lvt,
        _ => Jamo::None,
    }
}

fn japanese(wc: u32) -> bool {
    (0x2F00..=0x30FF).contains(&wc)
}

fn kanji(wc: u32) -> bool {
    (0x2F00..=0x2FDF).contains(&wc)
}

fn hiragana(wc: u32) -> bool {
    (0x3040..=0x309F).contains(&wc)
}

fn katakana(wc: u32) -> bool {
    (0x30A0..=0x30FF).contains(&wc)
}

fn backspace_deletes_character(wc: u32) -> bool {
    let latin = (0x0020..=0x02AF).contains(&wc) || (0x1E00..=0x1EFF).contains(&wc);
    let cyrillic = (0x0400..=0x052F).contains(&wc);
    let greek = (0x0370..=0x03FF).contains(&wc) || (0x1F00..=0x1FFF).contains(&wc);
    let kana = (0x3040..=0x30FF).contains(&wc);
    let hangul = (0xAC00..=0xD7A3).contains(&wc);
    let math = (0x2200..=0x22FF).contains(&wc);
    !latin && !cyrillic && !greek && !kana && !hangul && !is_emoji_base_character(wc) && !math
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordType {
    None,
    Letters,
    Numbers,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Gb {
    Other,
    ControlCrlf,
    Extend,
    Zwj,
    Prepend,
    SpacingMark,
    InHangulSyllable,
    RiOdd,
    RiEven,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Wb {
    Other,
    NewlineCrlf,
    ExtendFormat,
    Katakana,
    HebrewLetter,
    ALetter,
    MidNumLet,
    MidLetter,
    MidNum,
    Numeric,
    ExtendNumLet,
    RiOdd,
    RiEven,
    WSegSpace,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sb {
    Other,
    ExtendFormat,
    ParaSep,
    Sp,
    Lower,
    Upper,
    OLetter,
    Numeric,
    ATerm,
    SContinue,
    STerm,
    Close,
    ATermCloseSp,
    STermCloseSp,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lb {
    Other,
    Numeric,
    NumericClose,
    RiOdd,
    RiEven,
    DottedCircle,
    Hyphen,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BreakOpportunity {
    AlreadyHandled,
    Prohibited,
    Allowed,
}

fn is_other_term(sb: Sb) -> bool {
    !matches!(
        sb,
        Sb::OLetter
            | Sb::Upper
            | Sb::Lower
            | Sb::ParaSep
            | Sb::ATerm
            | Sb::STerm
            | Sb::ATermCloseSp
            | Sb::STermCloseSp
    )
}

fn grapheme_break_type(wc: u32, ty: T, makes_hangul_syllable: bool, prev_gb: Gb) -> Gb {
    let other_letter = || {
        let mut gb = Gb::Other;
        if makes_hangul_syllable {
            gb = Gb::InHangulSyllable;
        }
        if is_consonant_preceding_repha(wc) || is_consonant_prefixed(wc) {
            gb = Gb::Prepend;
        }
        gb
    };
    match ty {
        T::Format => {
            if wc == 0x200C {
                Gb::Extend
            } else if wc == 0x200D {
                Gb::Zwj
            } else if (0x600..=0x605).contains(&wc)
                || matches!(wc, 0x6DD | 0x70F | 0x8E2 | 0x110BD | 0x110CD)
            {
                Gb::Prepend
            } else if (0xE0020..=0xE00FF).contains(&wc) {
                Gb::Extend
            } else {
                Gb::ControlCrlf
            }
        }
        T::Control | T::LineSeparator | T::ParagraphSeparator | T::Surrogate => Gb::ControlCrlf,
        T::Unassigned => {
            if (0xFFF0..=0xFFF8).contains(&wc) || (0xE0000..=0xE0FFF).contains(&wc) {
                Gb::ControlCrlf
            } else {
                other_letter()
            }
        }
        T::OtherLetter => other_letter(),
        T::ModifierLetter => {
            if (0xFF9E..=0xFF9F).contains(&wc) {
                Gb::Extend
            } else {
                Gb::Other
            }
        }
        T::SpacingMark => {
            let mut gb = Gb::Other;
            if !(wc == 0x102B
                || wc == 0x102C
                || wc == 0x1038
                || (0x1062..=0x1064).contains(&wc)
                || (0x1067..=0x106D).contains(&wc)
                || wc == 0x1083
                || (0x1087..=0x108C).contains(&wc)
                || wc == 0x108F
                || (0x109A..=0x109C).contains(&wc)
                || wc == 0x1A61
                || wc == 0x1A63
                || wc == 0x1A64
                || wc == 0xAA7B
                || wc == 0xAA7D
                || wc == 0x11720
                || wc == 0x11721)
            {
                gb = Gb::SpacingMark;
            }
            if wc >= 0x0900
                && (matches!(
                    wc,
                    0x09BE
                        | 0x09D7
                        | 0x0B3E
                        | 0x0B57
                        | 0x0BBE
                        | 0x0BD7
                        | 0x0CC2
                        | 0x0CD5
                        | 0x0CD6
                        | 0x0D3E
                        | 0x0D57
                        | 0x0DCF
                        | 0x0DDF
                        | 0x1D165
                ) || (0x1D16E..=0x1D172).contains(&wc))
            {
                gb = Gb::Extend;
            }
            gb
        }
        T::EnclosingMark | T::NonSpacingMark => Gb::Extend,
        T::OtherSymbol => {
            if (0x1F1E6..=0x1F1FF).contains(&wc) {
                if prev_gb == Gb::RiOdd {
                    Gb::RiEven
                } else {
                    Gb::RiOdd
                }
            } else {
                Gb::Other
            }
        }
        T::ModifierSymbol => {
            if (0x1F3FB..=0x1F3FF).contains(&wc) {
                Gb::Extend
            } else {
                Gb::Other
            }
        }
        _ => Gb::Other,
    }
}

fn word_break_type(wc: u32, ty: T, break_type: B, script: Script, prev_wb: Wb) -> Wb {
    let mut wb = Wb::Other;

    if script == Script::KATAKANA {
        wb = Wb::Katakana;
    }
    if script == Script::HEBREW && ty == T::OtherLetter {
        wb = Wb::HebrewLetter;
    }

    if wb == Wb::Other {
        match wc >> 8 {
            0x00 => {
                if wc == 0x00b8 {
                    wb = Wb::ALetter;
                }
            }
            0x30 => {
                if matches!(
                    wc,
                    0x3031 | 0x3032 | 0x3033 | 0x3034 | 0x3035 | 0x309b | 0x309c | 0x30a0 | 0x30fc
                ) {
                    wb = Wb::Katakana;
                }
            }
            0xFF => {
                if wc == 0xFF70 {
                    wb = Wb::Katakana;
                } else if (0xFF9E..=0xFF9F).contains(&wc) {
                    wb = Wb::ExtendFormat;
                }
            }
            0x05 => {
                if wc == 0x058A {
                    wb = Wb::ALetter;
                }
            }
            0x07 => {
                if wc == 0x070F {
                    wb = Wb::ALetter;
                }
            }
            _ => {}
        }
    }

    if wb == Wb::Other {
        match break_type {
            B::Numeric => {
                if wc != 0x066C {
                    wb = Wb::Numeric;
                }
            }
            B::InfixSeparator => {
                if wc != 0x003A && wc != 0xFE13 && wc != 0x002E {
                    wb = Wb::MidNum;
                }
            }
            _ => {}
        }
    }

    if wb == Wb::Other {
        let alphabetic = break_type != B::ComplexContext && script != Script::HIRAGANA;
        match ty {
            T::Control => {
                if matches!(wc, 0x000D | 0x000A | 0x000B | 0x000C | 0x0085) {
                    wb = Wb::NewlineCrlf;
                }
            }
            T::LineSeparator | T::ParagraphSeparator => wb = Wb::NewlineCrlf,
            T::Format | T::SpacingMark | T::EnclosingMark | T::NonSpacingMark => {
                wb = Wb::ExtendFormat;
            }
            T::ConnectPunctuation => wb = Wb::ExtendNumLet,
            T::InitialPunctuation | T::FinalPunctuation => {
                if wc == 0x2018 || wc == 0x2019 {
                    wb = Wb::MidNumLet;
                }
            }
            T::OtherPunctuation => {
                if (0x055a..=0x055c).contains(&wc) || wc == 0x055e || wc == 0x05f3 {
                    wb = Wb::ALetter;
                } else if matches!(wc, 0x0027 | 0x002e | 0x2024 | 0xfe52 | 0xff07 | 0xff0e) {
                    wb = Wb::MidNumLet;
                } else if matches!(
                    wc,
                    0x00b7 | 0x05f4 | 0x2027 | 0x003a | 0x0387 | 0x055f | 0xfe13 | 0xfe55 | 0xff1a
                ) {
                    wb = Wb::MidLetter;
                } else if matches!(wc, 0x066c | 0xfe50 | 0xfe54 | 0xff0c | 0xff1b) {
                    wb = Wb::MidNum;
                }
            }
            T::OtherSymbol => {
                if (0x24B6..=0x24E9).contains(&wc) {
                    if alphabetic {
                        wb = Wb::ALetter;
                    }
                } else if (0x1F1E6..=0x1F1FF).contains(&wc) {
                    wb = if prev_wb == Wb::RiOdd {
                        Wb::RiEven
                    } else {
                        Wb::RiOdd
                    };
                }
            }
            T::OtherLetter | T::LetterNumber => {
                let ideographic = wc == 0x3006
                    || wc == 0x3007
                    || (0x3021..=0x3029).contains(&wc)
                    || (0x3038..=0x303A).contains(&wc)
                    || (0x3400..=0x4DB5).contains(&wc)
                    || (0x4E00..=0x9FC3).contains(&wc)
                    || (0xF900..=0xFA2D).contains(&wc)
                    || (0xFA30..=0xFA6A).contains(&wc)
                    || (0xFA70..=0xFAD9).contains(&wc)
                    || (0x20000..=0x2A6D6).contains(&wc)
                    || (0x2F800..=0x2FA1D).contains(&wc);
                if !ideographic && alphabetic {
                    wb = Wb::ALetter;
                }
            }
            T::LowercaseLetter | T::ModifierLetter | T::TitlecaseLetter | T::UppercaseLetter => {
                if alphabetic {
                    wb = Wb::ALetter;
                }
            }
            _ => {}
        }
    }

    if wb == Wb::Other && ty == T::SpaceSeparator && break_type != B::NonBreakingGlue {
        wb = Wb::WSegSpace;
    }

    wb
}

fn sentence_break_type(wc: u32, ty: T, break_type: B) -> Sb {
    let mut sb = Sb::Other;

    if break_type == B::Numeric {
        sb = Sb::Numeric;
    }

    if sb == Sb::Other {
        match ty {
            T::Control => {
                if wc == u32::from(b'\r') || wc == u32::from(b'\n') {
                    sb = Sb::ParaSep;
                } else if wc == 0x0009 || wc == 0x000B || wc == 0x000C {
                    sb = Sb::Sp;
                } else if wc == 0x0085 {
                    sb = Sb::ParaSep;
                }
            }
            T::SpaceSeparator => {
                if wc == 0x0020
                    || wc == 0x00A0
                    || wc == 0x1680
                    || (0x2000..=0x200A).contains(&wc)
                    || wc == 0x202F
                    || wc == 0x205F
                    || wc == 0x3000
                {
                    sb = Sb::Sp;
                }
            }
            T::LineSeparator | T::ParagraphSeparator => sb = Sb::ParaSep,
            T::Format | T::SpacingMark | T::EnclosingMark | T::NonSpacingMark => {
                sb = Sb::ExtendFormat;
            }
            T::ModifierLetter => {
                if (0xFF9E..=0xFF9F).contains(&wc) {
                    sb = Sb::ExtendFormat;
                }
            }
            T::TitlecaseLetter => sb = Sb::Upper,
            T::DashPunctuation => {
                if wc == 0x002D
                    || wc == 0x003B
                    || wc == 0x037E
                    || (0x2013..=0x2014).contains(&wc)
                    || wc == 0xFE14
                    || (0xFE31..=0xFE32).contains(&wc)
                    || matches!(
                        wc,
                        0xFE54 | 0xFE58 | 0xFE63 | 0xFF0D | 0xFF1A | 0xFF1B | 0xFF64
                    )
                {
                    sb = Sb::SContinue;
                }
            }
            T::OtherPunctuation => {
                if wc == 0x05F3 {
                    sb = Sb::OLetter;
                } else if matches!(wc, 0x002E | 0x2024 | 0xFE52 | 0xFF0E) {
                    sb = Sb::ATerm;
                }

                if matches!(
                    wc,
                    0x002C | 0x003A | 0x055D | 0x07F8 | 0x1802 | 0x1808 | 0x3001 | 0xFE13
                ) || (0x060C..=0x060D).contains(&wc)
                    || (0xFE10..=0xFE11).contains(&wc)
                    || (0xFE50..=0xFE51).contains(&wc)
                    || matches!(wc, 0xFE55 | 0xFF0C | 0xFF1A | 0xFF64)
                {
                    sb = Sb::SContinue;
                }

                if is_sterm(wc) {
                    sb = Sb::STerm;
                }
            }
            _ => {}
        }
    }

    if sb == Sb::Other {
        if ty == T::LowercaseLetter {
            sb = Sb::Lower;
        } else if ty == T::UppercaseLetter {
            sb = Sb::Upper;
        } else if matches!(ty, T::TitlecaseLetter | T::ModifierLetter | T::OtherLetter) {
            sb = Sb::OLetter;
        }

        if ty == T::OpenPunctuation || ty == T::ClosePunctuation || break_type == B::Quotation {
            sb = Sb::Close;
        }
    }

    sb
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordRule {
    Boundary,
    NoBoundary,
    JoinWithPrevious,
}

fn is_letter_or_number(wb: Wb) -> bool {
    matches!(wb, Wb::ALetter | Wb::HebrewLetter | Wb::Numeric)
}

fn is_letter(wb: Wb) -> bool {
    matches!(wb, Wb::ALetter | Wb::HebrewLetter)
}

fn word_rule(
    prev_prev_wb: Wb,
    prev_wb: Wb,
    wb: Wb,
    prev_wc: u32,
    wc: u32,
    is_extended_pictographic: bool,
    follows_previous: bool,
) -> WordRule {
    if (prev_wb == Wb::NewlineCrlf && follows_previous) || wb == Wb::NewlineCrlf {
        return WordRule::Boundary;
    }
    if (prev_wc == 0x200D && is_extended_pictographic)
        || (prev_wb == Wb::WSegSpace && wb == Wb::WSegSpace && follows_previous)
        || wb == Wb::ExtendFormat
        || (is_letter_or_number(prev_wb) && is_letter_or_number(wb))
        || (prev_wb == Wb::Katakana && wb == Wb::Katakana)
        || ((is_letter_or_number(prev_wb) || matches!(prev_wb, Wb::Katakana | Wb::ExtendNumLet))
            && wb == Wb::ExtendNumLet)
        || (prev_wb == Wb::ExtendNumLet && (is_letter_or_number(wb) || wb == Wb::Katakana))
    {
        return WordRule::NoBoundary;
    }
    if is_letter(prev_prev_wb)
        && is_letter(wb)
        && (prev_wb == Wb::MidLetter || prev_wb == Wb::MidNumLet || prev_wc == 0x0027)
    {
        return WordRule::JoinWithPrevious;
    }
    if prev_wb == Wb::HebrewLetter && wc == 0x0027 {
        return WordRule::NoBoundary;
    }
    if (prev_prev_wb == Wb::HebrewLetter && prev_wc == 0x0022 && wb == Wb::HebrewLetter)
        || (prev_prev_wb == Wb::Numeric
            && wb == Wb::Numeric
            && (prev_wb == Wb::MidNum || prev_wb == Wb::MidNumLet || prev_wc == 0x0027))
    {
        return WordRule::JoinWithPrevious;
    }
    if prev_wb == Wb::RiOdd && wb == Wb::RiEven {
        return WordRule::NoBoundary;
    }
    WordRule::Boundary
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SentenceRule {
    Boundary(bool),
    Retype(Sb),
    Reopen,
}

fn is_any_term(sb: Sb) -> bool {
    matches!(
        sb,
        Sb::ATerm | Sb::ATermCloseSp | Sb::STerm | Sb::STermCloseSp
    )
}

fn sentence_rule(
    prev_prev_sb: Sb,
    prev_sb: Sb,
    sb: Sb,
    prev_wc: u32,
    wc: u32,
    follows_previous: bool,
) -> SentenceRule {
    if wc == u32::from(b'\n') && prev_wc == u32::from(b'\r') {
        return SentenceRule::Boundary(false);
    }
    if prev_sb == Sb::ParaSep && follows_previous {
        return SentenceRule::Boundary(true);
    }
    if sb == Sb::ExtendFormat
        || (prev_sb == Sb::ATerm && sb == Sb::Numeric)
        || (matches!(prev_prev_sb, Sb::Upper | Sb::Lower)
            && prev_sb == Sb::ATerm
            && sb == Sb::Upper)
    {
        return SentenceRule::Boundary(false);
    }
    match (prev_sb, sb) {
        (Sb::ATerm, Sb::Close) => return SentenceRule::Retype(Sb::ATerm),
        (Sb::STerm, Sb::Close) => return SentenceRule::Retype(Sb::STerm),
        (Sb::ATerm, Sb::Sp) => return SentenceRule::Retype(Sb::ATermCloseSp),
        (Sb::STerm, Sb::Sp) => return SentenceRule::Retype(Sb::STermCloseSp),
        _ => {}
    }
    if matches!(prev_sb, Sb::ATerm | Sb::ATermCloseSp) && sb == Sb::Lower {
        return SentenceRule::Boundary(false);
    }
    if matches!(prev_prev_sb, Sb::ATerm | Sb::ATermCloseSp)
        && is_other_term(prev_sb)
        && sb == Sb::Lower
    {
        return SentenceRule::Reopen;
    }
    if (is_any_term(prev_sb) && matches!(sb, Sb::SContinue | Sb::ATerm | Sb::STerm))
        || (matches!(prev_sb, Sb::ATerm | Sb::STerm)
            && matches!(sb, Sb::Close | Sb::Sp | Sb::ParaSep))
        || (is_any_term(prev_sb) && matches!(sb, Sb::Sp | Sb::ParaSep))
    {
        return SentenceRule::Boundary(false);
    }
    SentenceRule::Boundary(is_any_term(prev_sb) && sb != Sb::ParaSep)
}

fn is_hangul_break(bt: B) -> bool {
    matches!(
        bt,
        B::HangulLJamo
            | B::HangulVJamo
            | B::HangulTJamo
            | B::HangulLvSyllable
            | B::HangulLvtSyllable
    )
}

fn is_hyphen_break(bt: B) -> bool {
    bt == B::Hyphen || bt == B::UnambiguousHyphen
}

fn is_mandatory_class(bt: B) -> bool {
    matches!(
        bt,
        B::Mandatory | B::CarriageReturn | B::LineFeed | B::NextLine
    )
}

fn is_hyphen_char(wc: u32) -> bool {
    matches!(
        wc,
        0x002D | 0x058a | 0x1400 | 0x1806 | 0x2010 | 0x2e17 | 0x2e40 | 0x30a0 | 0xfe63 | 0xff0d
    )
}

fn is_white_char(wc: u32, ty: T) -> bool {
    match ty {
        T::SpaceSeparator | T::LineSeparator | T::ParagraphSeparator => true,
        T::Control => matches!(wc, 0x09 | 0x0A | 0x0D | 0x0C),
        _ => false,
    }
}

fn at(text: &str, index: usize) -> u32 {
    unicode::char_at(text, index)
}

fn at_end(text: &str, index: usize) -> bool {
    index >= text.len() || text.as_bytes()[index] == 0
}

pub(super) fn default_break(text: &str, attrs: &mut [LogAttr]) {
    let mut next = 0usize;

    let mut prev_break_type = B::Unknown;
    let mut prev_prev_break_type = B::Unknown;
    let mut prev_wc = 0u32;
    let mut prev_prev_wc = 0u32;
    let mut prev_script = Script::COMMON;
    let mut prev_jamo = Jamo::None;
    let mut prev_space_or_hyphen = false;

    let mut prev_gb_type = Gb::Other;
    let mut met_extended_pictographic = false;
    let mut met_incb_consonant = false;
    let mut met_incb_consonant_linker = false;

    let mut prev_prev_wb_type = Wb::Other;
    let mut prev_wb_type = Wb::Other;
    let mut prev_wb_i: isize = -1;

    let mut prev_prev_sb_type = Sb::Other;
    let mut prev_sb_type = Sb::Other;
    let mut prev_sb_i: isize = -1;

    let mut prev_prev_lb_type = Lb::Other;
    let mut prev_lb_type = Lb::Other;
    let mut met_lb15a = false;
    let mut prev_lb_i: isize = -1;
    let mut met_ext_pict_unassigned = false;

    let mut current_word_type = WordType::None;
    let mut last_word_letter = 0u32;
    let mut base_character = 0u32;

    let mut last_sentence_start: isize = -1;
    let mut last_non_space: isize = -1;

    let mut almost_done = false;
    let mut done = false;

    let mut next_wc;
    let mut next_next_wc;
    if at_end(text, 0) {
        next_wc = PARAGRAPH_SEPARATOR;
        almost_done = true;
    } else {
        next_wc = at(text, next);
    }
    let mut next_break_type = line_break_class(next_wc);
    let mut next_next_break_type = B::Unknown;

    let mut i: usize = 0;
    while !done {
        let wc = next_wc;
        let mut break_type = next_break_type;

        if almost_done {
            next_wc = 0;
            next_break_type = B::Unknown;
            done = true;
        } else {
            next = unicode::next_char_index(text, next);
            if at_end(text, next) {
                next_wc = PARAGRAPH_SEPARATOR;
                next_next_wc = PARAGRAPH_SEPARATOR;
                almost_done = true;
            } else {
                next_wc = at(text, next);
                let next_next = unicode::next_char_index(text, next);
                next_next_wc = if at_end(text, next_next) {
                    PARAGRAPH_SEPARATOR
                } else {
                    at(text, next_next)
                };
            }
            next_break_type = line_break_class(next_wc);
            next_next_break_type = line_break_class(next_next_wc);
        }

        let ty = unicode::unichar_type(wc);
        let jamo = jamo_type(break_type);

        let makes_hangul_syllable = if jamo == Jamo::None {
            false
        } else {
            let prev_end = HANGUL_JAMO_PROPS[prev_jamo as usize].1 as i32;
            let this_start = HANGUL_JAMO_PROPS[jamo as usize].0 as i32;
            prev_end == this_start || prev_end + 1 == this_start
        };

        attrs[i].set_is_white(is_white_char(wc, ty));
        attrs[i].set_is_expandable_space(wc == 0x0020 || wc == 0x00A0);
        let is_extended_pictographic = is_emoji_extended_pictographic(wc);

        let is_grapheme_boundary;
        {
            let gb_type = grapheme_break_type(wc, ty, makes_hangul_syllable, prev_gb_type);

            if met_extended_pictographic {
                met_extended_pictographic = gb_type == Gb::Extend
                    || (is_emoji_extended_pictographic(prev_wc) && gb_type == Gb::Zwj)
                    || (prev_gb_type == Gb::Extend && gb_type == Gb::Zwj)
                    || (prev_gb_type == Gb::Zwj && is_extended_pictographic);
            }

            let mut boundary = true;

            if prev_gb_type == Gb::RiOdd && gb_type == Gb::RiEven {
                boundary = false;
            }

            if is_extended_pictographic && prev_gb_type == Gb::Zwj && met_extended_pictographic {
                boundary = false;
            }

            if met_incb_consonant || met_incb_consonant_linker {
                if met_incb_consonant_linker && is_indic_conjunct_break_consonant(wc) {
                    met_incb_consonant = true;
                    met_incb_consonant_linker = false;
                    boundary = false;
                }
                if !(is_indic_conjunct_break_extend(wc) || is_indic_conjunct_break_linker(wc)) {
                    met_incb_consonant = false;
                    met_incb_consonant_linker = false;
                }
            }

            if prev_gb_type == Gb::Prepend {
                boundary = false;
            }
            if gb_type == Gb::SpacingMark {
                boundary = false;
            }
            if gb_type == Gb::Extend || gb_type == Gb::Zwj {
                boundary = false;
            }
            if gb_type == Gb::InHangulSyllable {
                boundary = false;
            }
            if prev_gb_type == Gb::ControlCrlf || gb_type == Gb::ControlCrlf {
                boundary = true;
            }
            if wc == u32::from(b'\n') && prev_wc == u32::from(b'\r') {
                boundary = false;
            }

            if is_extended_pictographic {
                met_extended_pictographic = true;
            }
            if met_incb_consonant
                && !met_incb_consonant_linker
                && is_indic_conjunct_break_linker(wc)
            {
                met_incb_consonant_linker = true;
            }
            if !met_incb_consonant
                && !met_incb_consonant_linker
                && is_indic_conjunct_break_consonant(wc)
            {
                met_incb_consonant = true;
            }

            attrs[i].set_is_cursor_position(boundary);
            if boundary {
                attrs[i].set_backspace_deletes_character(
                    backspace_deletes_character(base_character)
                        || is_virama(prev_wc)
                        || is_vowel_dependent(prev_wc),
                );
            } else {
                attrs[i].set_backspace_deletes_character(false);
            }

            prev_gb_type = gb_type;
            is_grapheme_boundary = boundary;
        }

        let script = unicode::script(wc);
        let mut is_word_boundary = false;
        if is_grapheme_boundary || (0x1F1E6..=0x1F1FF).contains(&wc) {
            let wb_type = word_break_type(wc, ty, break_type, script, prev_wb_type);
            let current = i as isize;

            match word_rule(
                prev_prev_wb_type,
                prev_wb_type,
                wb_type,
                prev_wc,
                wc,
                is_extended_pictographic,
                prev_wb_i + 1 == current,
            ) {
                WordRule::Boundary => is_word_boundary = true,
                WordRule::NoBoundary => is_word_boundary = false,
                WordRule::JoinWithPrevious => {
                    attrs[prev_wb_i.unsigned_abs()].set_is_word_boundary(false);
                    is_word_boundary = false;
                }
            }

            if wb_type != Wb::ExtendFormat {
                prev_prev_wb_type = prev_wb_type;
                prev_wb_type = wb_type;
                prev_wb_i = current;
            }
        }
        attrs[i].set_is_word_boundary(is_word_boundary);

        let mut is_sentence_boundary = false;
        if is_word_boundary || wc == u32::from(b'\r') || wc == u32::from(b'\n') {
            let mut sb_type = sentence_break_type(wc, ty, break_type);
            let current = i as isize;

            match sentence_rule(
                prev_prev_sb_type,
                prev_sb_type,
                sb_type,
                prev_wc,
                wc,
                prev_sb_i + 1 == current,
            ) {
                SentenceRule::Boundary(boundary) => is_sentence_boundary = boundary,
                SentenceRule::Retype(retyped) => sb_type = retyped,
                SentenceRule::Reopen => {
                    let prev_index = prev_sb_i.unsigned_abs();
                    attrs[prev_index].set_is_sentence_boundary(false);
                    attrs[prev_index].set_is_sentence_end(false);
                    last_sentence_start = -1;
                    for j in (0..prev_index).rev() {
                        attrs[j].set_is_sentence_end(false);
                        if attrs[j].is_sentence_boundary() {
                            last_sentence_start = j as isize;
                            break;
                        }
                    }
                }
            }

            if sb_type != Sb::ExtendFormat
                && !((prev_prev_sb_type == Sb::ATerm || prev_prev_sb_type == Sb::ATermCloseSp)
                    && is_other_term(prev_sb_type)
                    && is_other_term(sb_type))
            {
                prev_prev_sb_type = prev_sb_type;
                prev_sb_type = sb_type;
                prev_sb_i = current;
            }
        }

        if i == 0 || done {
            is_sentence_boundary = true;
        }
        attrs[i].set_is_sentence_boundary(is_sentence_boundary);

        let mut break_op = BreakOpportunity::AlreadyHandled;
        let mut row_break_type = if prev_break_type == B::Space {
            prev_prev_break_type
        } else {
            prev_break_type
        };

        attrs[i].set_is_char_break(false);
        attrs[i].set_is_line_break(false);
        attrs[i].set_is_mandatory_break(false);

        match break_type {
            B::Ambiguous | B::Surrogate | B::Unknown => break_type = B::Alphabetic,
            B::ComplexContext => {
                break_type = if ty == T::NonSpacingMark || ty == T::SpacingMark {
                    B::CombiningMark
                } else {
                    B::Alphabetic
                };
            }
            B::ConditionalJapaneseStarter => break_type = B::NonStarter,
            _ => {}
        }

        if attrs[i].is_cursor_position()
            || break_type == B::CombiningMark
            || break_type == B::ZeroWidthJoiner
            || break_type == B::NonBreakingGlue
            || is_hangul_break(break_type)
            || break_type == B::EmojiModifier
            || break_type == B::RegionalIndicator
            || break_type == B::Virama
            || break_type == B::ViramaFinal
        {
            let mut lb_type = Lb::Other;

            if wc == 0x2010 {
                lb_type = Lb::Hyphen;
            }
            if wc == 0x25CC {
                lb_type = Lb::DottedCircle;
            }
            if break_type == B::Numeric {
                lb_type = Lb::Numeric;
            }
            if (break_type == B::Symbol || break_type == B::InfixSeparator)
                && prev_lb_type != Lb::Numeric
            {
                lb_type = Lb::Other;
            }
            if break_type == B::ClosePunctuation || break_type == B::CloseParenthesis {
                lb_type = if prev_lb_type == Lb::Numeric {
                    Lb::NumericClose
                } else {
                    Lb::Other
                };
            }
            if break_type == B::RegionalIndicator {
                lb_type = if prev_lb_type == Lb::RiOdd {
                    Lb::RiEven
                } else {
                    Lb::RiOdd
                };
            }

            attrs[i].set_is_line_break(true);
            if attrs[i].is_cursor_position() {
                attrs[i].set_is_char_break(true);
            }

            if row_break_type == B::Unknown {
                row_break_type = B::Alphabetic;
            }

            if prev_break_type == B::EmojiBase && break_type == B::EmojiModifier {
                break_op = BreakOpportunity::Prohibited;
            }

            if met_ext_pict_unassigned && break_type == B::EmojiModifier {
                met_ext_pict_unassigned = false;
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_lb_type == Lb::RiOdd && lb_type == Lb::RiEven {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(
                prev_break_type,
                B::Alphabetic | B::HebrewLetter | B::Numeric
            ) && break_type == B::OpenPunctuation
                && !is_east_asian_wide(wc)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::CloseParenthesis
                && !is_east_asian_wide(prev_wc)
                && matches!(break_type, B::Alphabetic | B::HebrewLetter | B::Numeric)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::InfixSeparator
                && matches!(break_type, B::Alphabetic | B::HebrewLetter)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::AksaraPreBase
                && (break_type == B::Aksara
                    || lb_type == Lb::DottedCircle
                    || break_type == B::AksaraStart)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if (prev_break_type == B::Aksara
                || prev_lb_type == Lb::DottedCircle
                || prev_break_type == B::AksaraStart)
                && (break_type == B::ViramaFinal || break_type == B::Virama)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if (prev_prev_break_type == B::Aksara
                || prev_prev_lb_type == Lb::DottedCircle
                || prev_prev_break_type == B::AksaraStart)
                && prev_break_type == B::Virama
                && (break_type == B::Aksara || lb_type == Lb::DottedCircle)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if (prev_break_type == B::Aksara
                || prev_lb_type == Lb::DottedCircle
                || prev_break_type == B::AksaraStart)
                && (break_type == B::Aksara
                    || lb_type == Lb::DottedCircle
                    || break_type == B::AksaraStart)
                && next_break_type == B::ViramaFinal
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(prev_break_type, B::Alphabetic | B::HebrewLetter)
                && matches!(break_type, B::Alphabetic | B::HebrewLetter)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if is_hangul_break(prev_break_type) && break_type == B::Postfix {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Prefix && is_hangul_break(break_type) {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::HangulLJamo
                && matches!(
                    break_type,
                    B::HangulLJamo | B::HangulVJamo | B::HangulLvSyllable | B::HangulLvtSyllable
                )
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(prev_break_type, B::HangulVJamo | B::HangulLvSyllable)
                && matches!(break_type, B::HangulVJamo | B::HangulTJamo)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(prev_break_type, B::HangulTJamo | B::HangulLvtSyllable)
                && break_type == B::HangulTJamo
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_prev_lb_type == Lb::Numeric
                && matches!(prev_break_type, B::ClosePunctuation | B::CloseParenthesis)
                && matches!(break_type, B::Postfix | B::Prefix)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_lb_type == Lb::Numeric && matches!(break_type, B::Postfix | B::Prefix) {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Postfix
                && break_type == B::OpenPunctuation
                && next_break_type == B::Numeric
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Postfix
                && break_type == B::OpenPunctuation
                && next_break_type == B::InfixSeparator
                && next_next_break_type == B::Numeric
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Postfix && break_type == B::Numeric {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Prefix
                && break_type == B::OpenPunctuation
                && next_break_type == B::Numeric
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Prefix
                && break_type == B::OpenPunctuation
                && next_break_type == B::InfixSeparator
                && next_next_break_type == B::Numeric
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Prefix && break_type == B::Numeric {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Hyphen && break_type == B::Numeric {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::InfixSeparator && break_type == B::Numeric {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_lb_type == Lb::Numeric && break_type == B::Numeric {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(prev_break_type, B::Prefix | B::Postfix)
                && matches!(break_type, B::Alphabetic | B::HebrewLetter)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(prev_break_type, B::Alphabetic | B::HebrewLetter)
                && matches!(break_type, B::Prefix | B::Postfix)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Prefix
                && matches!(break_type, B::Ideographic | B::EmojiBase | B::EmojiModifier)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(
                prev_break_type,
                B::Ideographic | B::EmojiBase | B::EmojiModifier
            ) && break_type == B::Postfix
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(prev_break_type, B::Alphabetic | B::HebrewLetter)
                && break_type == B::Numeric
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Numeric
                && matches!(break_type, B::Alphabetic | B::HebrewLetter)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if break_type == B::Inseparable {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Symbol && break_type == B::HebrewLetter {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_prev_break_type == B::HebrewLetter
                && is_hyphen_break(prev_break_type)
                && break_type != B::HebrewLetter
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(
                break_type,
                B::After | B::UnambiguousHyphen | B::Hyphen | B::NonStarter
            ) || prev_break_type == B::Before
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if (prev_lb_i == 0
                || matches!(
                    prev_prev_break_type,
                    B::Mandatory
                        | B::CarriageReturn
                        | B::LineFeed
                        | B::NextLine
                        | B::Space
                        | B::ZeroWidthSpace
                        | B::Contingent
                        | B::NonBreakingGlue
                ))
                && is_hyphen_break(prev_break_type)
                && matches!(break_type, B::Alphabetic | B::HebrewLetter)
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Contingent || break_type == B::Contingent {
                break_op = BreakOpportunity::Allowed;
            }

            if !is_east_asian_wide(prev_wc) && break_type == B::Quotation {
                break_op = BreakOpportunity::Prohibited;
            }

            if break_type == B::Quotation && (!is_east_asian_wide(next_wc) || done) {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Quotation && !is_east_asian_wide(wc) {
                break_op = BreakOpportunity::Prohibited;
            }

            if (prev_lb_i == 0 || !is_east_asian_wide(prev_prev_wc))
                && prev_break_type == B::Quotation
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if break_type == B::Quotation && ty != T::InitialPunctuation {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Quotation
                && unicode::unichar_type(prev_wc) != T::FinalPunctuation
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Space {
                break_op = BreakOpportunity::Allowed;
            }

            if row_break_type == B::BeforeAndAfter && break_type == B::BeforeAndAfter {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(row_break_type, B::ClosePunctuation | B::CloseParenthesis)
                && break_type == B::NonStarter
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if break_type == B::InfixSeparator {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Space
                && break_type == B::InfixSeparator
                && next_break_type == B::Numeric
            {
                break_op = BreakOpportunity::Allowed;
            }

            if ty == T::FinalPunctuation
                && break_type == B::Quotation
                && (next_wc == 0
                    || matches!(
                        next_break_type,
                        B::Space
                            | B::NonBreakingGlue
                            | B::WordJoiner
                            | B::ClosePunctuation
                            | B::Quotation
                            | B::CloseParenthesis
                            | B::Exclamation
                            | B::InfixSeparator
                            | B::Symbol
                            | B::Mandatory
                            | B::CarriageReturn
                            | B::LineFeed
                            | B::NextLine
                            | B::ZeroWidthSpace
                    ))
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if met_lb15a
                && break_type != B::Space
                && break_type != B::CombiningMark
                && break_type != B::ZeroWidthJoiner
            {
                met_lb15a = false;
                break_op = BreakOpportunity::Prohibited;
            } else if ty == T::InitialPunctuation
                && break_type == B::Quotation
                && (i == 0
                    || matches!(
                        prev_break_type,
                        B::Mandatory
                            | B::CarriageReturn
                            | B::LineFeed
                            | B::NextLine
                            | B::OpenPunctuation
                            | B::Quotation
                            | B::NonBreakingGlue
                            | B::Space
                            | B::ZeroWidthSpace
                    ))
            {
                met_lb15a = true;
            }

            if row_break_type == B::OpenPunctuation {
                break_op = BreakOpportunity::Prohibited;
            }

            if matches!(
                break_type,
                B::ClosePunctuation | B::CloseParenthesis | B::Exclamation | B::Symbol
            ) {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_wc == u32::from(b'/') && wc > u32::from(b' ') && wc < 0x7f {
                break_op = BreakOpportunity::Prohibited;
            }

            if break_type == B::NonBreakingGlue
                && !matches!(
                    prev_break_type,
                    B::Space | B::After | B::Hyphen | B::UnambiguousHyphen
                )
            {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::NonBreakingGlue {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::WordJoiner || break_type == B::WordJoiner {
                break_op = BreakOpportunity::Prohibited;
            }

            if break_type == B::CombiningMark || break_type == B::ZeroWidthJoiner {
                if !matches!(
                    prev_break_type,
                    B::Mandatory
                        | B::CarriageReturn
                        | B::LineFeed
                        | B::NextLine
                        | B::Space
                        | B::ZeroWidthSpace
                ) {
                    break_op = BreakOpportunity::Prohibited;
                }
                if met_lb15a {
                    break_op = BreakOpportunity::Prohibited;
                }
            } else if is_emoji_extended_pictographic(wc) && ty == T::Unassigned {
                met_ext_pict_unassigned = true;
            }

            if prev_wc == 0x200D {
                break_op = BreakOpportunity::Prohibited;
            }

            if row_break_type == B::ZeroWidthSpace {
                break_op = BreakOpportunity::Allowed;
            }

            if break_type == B::Space || break_type == B::ZeroWidthSpace {
                break_op = BreakOpportunity::Prohibited;
            }

            if is_mandatory_class(break_type) {
                break_op = BreakOpportunity::Prohibited;
            }

            if prev_break_type == B::Mandatory
                || (prev_break_type == B::CarriageReturn && wc != u32::from(b'\n'))
                || prev_break_type == B::LineFeed
                || prev_break_type == B::NextLine
            {
                attrs[i].set_is_mandatory_break(true);
                break_op = BreakOpportunity::Allowed;
            }

            match break_op {
                BreakOpportunity::Prohibited => attrs[i].set_is_line_break(false),
                BreakOpportunity::Allowed => attrs[i].set_is_line_break(true),
                BreakOpportunity::AlreadyHandled => {}
            }

            if !(break_type == B::CombiningMark || break_type == B::ZeroWidthJoiner)
                && (!(break_type == B::Symbol || break_type == B::InfixSeparator)
                    || prev_lb_type != Lb::Numeric)
            {
                prev_prev_lb_type = prev_lb_type;
                prev_lb_type = lb_type;
            }
        }

        if break_type != B::Space {
            if break_type == B::CombiningMark || break_type == B::ZeroWidthJoiner {
                if i == 0
                    || matches!(
                        prev_break_type,
                        B::Mandatory
                            | B::CarriageReturn
                            | B::LineFeed
                            | B::NextLine
                            | B::Space
                            | B::ZeroWidthSpace
                    )
                {
                    prev_break_type = B::Alphabetic;
                }
            } else {
                prev_prev_break_type = prev_break_type;
                prev_break_type = break_type;
                prev_lb_i = i as isize;
            }
            prev_jamo = jamo;
        } else if prev_break_type != B::Space {
            prev_prev_break_type = prev_break_type;
            prev_break_type = break_type;
            prev_lb_i = i as isize;
        }

        attrs[i].set_is_word_start(false);
        attrs[i].set_is_word_end(false);

        if current_word_type != WordType::None {
            match ty {
                T::SpacingMark | T::EnclosingMark | T::NonSpacingMark | T::Format => {}
                T::LowercaseLetter
                | T::ModifierLetter
                | T::OtherLetter
                | T::TitlecaseLetter
                | T::UppercaseLetter => {
                    if current_word_type == WordType::Letters
                        && (japanese(last_word_letter) || japanese(wc))
                        && ((hiragana(last_word_letter) && !hiragana(wc))
                            || (katakana(last_word_letter) && !(katakana(wc) || hiragana(wc)))
                            || (kanji(last_word_letter) && !(hiragana(wc) || kanji(wc)))
                            || (japanese(last_word_letter) && !japanese(wc))
                            || (!japanese(last_word_letter) && japanese(wc)))
                    {
                        attrs[i].set_is_word_start(true);
                        attrs[i].set_is_word_end(true);
                    }
                    last_word_letter = wc;
                }
                T::DecimalNumber | T::LetterNumber | T::OtherNumber => {
                    last_word_letter = wc;
                }
                _ => {
                    attrs[i].set_is_word_end(true);
                    current_word_type = WordType::None;
                }
            }
        } else {
            match ty {
                T::LowercaseLetter
                | T::ModifierLetter
                | T::OtherLetter
                | T::TitlecaseLetter
                | T::UppercaseLetter => {
                    current_word_type = WordType::Letters;
                    last_word_letter = wc;
                    attrs[i].set_is_word_start(true);
                }
                T::DecimalNumber | T::LetterNumber | T::OtherNumber => {
                    current_word_type = WordType::Numbers;
                    last_word_letter = wc;
                    attrs[i].set_is_word_start(true);
                }
                _ => {}
            }
        }

        attrs[i].set_is_sentence_start(false);
        attrs[i].set_is_sentence_end(false);

        let current = i as isize;
        if last_sentence_start == -1 && !is_sentence_boundary {
            last_sentence_start = current - 1;
        }

        if i > 0 && !attrs[i - 1].is_white() {
            last_non_space = current;
        }

        if last_sentence_start != -1 && is_sentence_boundary {
            if last_non_space >= last_sentence_start {
                attrs[last_sentence_start.unsigned_abs()].set_is_sentence_start(true);
                attrs[last_non_space.unsigned_abs()].set_is_sentence_end(true);
            }
            last_sentence_start = -1;
            last_non_space = -1;
        }

        if last_sentence_start != -1
            && last_sentence_start == current - 1
            && attrs[i - 1].is_white()
        {
            last_sentence_start += 1;
        }

        {
            let insert_hyphens = match prev_script {
                Script::COMMON => prev_wc == 0x00ad,
                Script::HAN | Script::HANGUL | Script::HIRAGANA | Script::KATAKANA => false,
                _ => true,
            };

            let space_or_hyphen = is_white_char(wc, ty) || is_hyphen_char(wc);

            attrs[i].set_break_removes_preceding(false);
            attrs[i].set_break_inserts_hyphen(
                !attrs[i].is_word_boundary()
                    && !prev_space_or_hyphen
                    && !space_or_hyphen
                    && insert_hyphens,
            );

            if prev_wc == 0x2027 {
                attrs[i].set_break_inserts_hyphen(true);
                attrs[i].set_break_removes_preceding(true);
            }

            prev_space_or_hyphen = space_or_hyphen;
        }

        prev_prev_wc = prev_wc;
        prev_wc = wc;
        prev_script = script;

        if ty != T::SpacingMark && ty != T::EnclosingMark && ty != T::NonSpacingMark {
            base_character = wc;
        }

        i += 1;
    }

    let last = i - 1;

    attrs[0].set_is_cursor_position(true);
    attrs[last].set_is_cursor_position(true);

    attrs[0].set_is_word_boundary(true);
    attrs[last].set_is_word_boundary(true);

    attrs[0].set_is_line_break(false);
    attrs[last].set_is_line_break(true);
    attrs[last].set_is_mandatory_break(true);
}
