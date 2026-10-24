//! Southstar unicode — the Unicode character properties text layout reads: category, line break class, script, width, combining class, case and decomposition.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod case;
mod tables;

#[cfg(test)]
mod tests;

pub use case::{lower_string, to_lower, to_title, to_upper, upper_string};

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Control,
    Format,
    Unassigned,
    PrivateUse,
    Surrogate,
    LowercaseLetter,
    ModifierLetter,
    OtherLetter,
    TitlecaseLetter,
    UppercaseLetter,
    SpacingMark,
    EnclosingMark,
    NonSpacingMark,
    DecimalNumber,
    LetterNumber,
    OtherNumber,
    ConnectPunctuation,
    DashPunctuation,
    ClosePunctuation,
    FinalPunctuation,
    InitialPunctuation,
    OtherPunctuation,
    OpenPunctuation,
    CurrencySymbol,
    ModifierSymbol,
    MathSymbol,
    OtherSymbol,
    LineSeparator,
    ParagraphSeparator,
    SpaceSeparator,
}

const CATEGORIES: [Category; 30] = [
    Category::Control,
    Category::Format,
    Category::Unassigned,
    Category::PrivateUse,
    Category::Surrogate,
    Category::LowercaseLetter,
    Category::ModifierLetter,
    Category::OtherLetter,
    Category::TitlecaseLetter,
    Category::UppercaseLetter,
    Category::SpacingMark,
    Category::EnclosingMark,
    Category::NonSpacingMark,
    Category::DecimalNumber,
    Category::LetterNumber,
    Category::OtherNumber,
    Category::ConnectPunctuation,
    Category::DashPunctuation,
    Category::ClosePunctuation,
    Category::FinalPunctuation,
    Category::InitialPunctuation,
    Category::OtherPunctuation,
    Category::OpenPunctuation,
    Category::CurrencySymbol,
    Category::ModifierSymbol,
    Category::MathSymbol,
    Category::OtherSymbol,
    Category::LineSeparator,
    Category::ParagraphSeparator,
    Category::SpaceSeparator,
];

impl Category {
    pub fn is_letter(self) -> bool {
        matches!(
            self,
            Category::LowercaseLetter
                | Category::UppercaseLetter
                | Category::TitlecaseLetter
                | Category::ModifierLetter
                | Category::OtherLetter
        )
    }

    pub fn is_number(self) -> bool {
        matches!(
            self,
            Category::DecimalNumber | Category::LetterNumber | Category::OtherNumber
        )
    }

    pub fn is_mark(self) -> bool {
        matches!(
            self,
            Category::NonSpacingMark | Category::SpacingMark | Category::EnclosingMark
        )
    }
}

fn lookup(starts: &[u32], values: &[u8], ch: u32) -> u8 {
    let index = starts.partition_point(|&start| start <= ch);
    values[index.saturating_sub(1)]
}

pub fn category(ch: u32) -> Category {
    let index = lookup(&tables::CATEGORY_STARTS, &tables::CATEGORY_VALUES, ch);
    CATEGORIES[usize::from(index)]
}

pub fn line_break(ch: u32) -> u8 {
    lookup(&tables::LINE_BREAK_STARTS, &tables::LINE_BREAK_VALUES, ch)
}

pub fn script(ch: u32) -> u8 {
    lookup(&tables::SCRIPT_STARTS, &tables::SCRIPT_VALUES, ch)
}

pub fn script_count() -> usize {
    tables::SCRIPT_TAGS.len()
}

pub fn script_tag(script: u8) -> u32 {
    let tag = tables::SCRIPT_TAGS
        .get(usize::from(script))
        .copied()
        .unwrap_or(*b"Zzzz");
    u32::from_be_bytes(tag)
}

pub fn combining_class(ch: u32) -> u8 {
    lookup(
        &tables::COMBINING_CLASS_STARTS,
        &tables::COMBINING_CLASS_VALUES,
        ch,
    )
}

pub fn is_wide(ch: u32) -> bool {
    let index = tables::WIDE_STARTS.partition_point(|&start| start <= ch);
    (index % 2 == 1) == tables::WIDE_FIRST
}

pub fn is_zero_width(ch: u32) -> bool {
    if ch == 0x00ad {
        return false;
    }
    matches!(
        category(ch),
        Category::NonSpacingMark | Category::EnclosingMark | Category::Format
    ) || (0x1160..0x1200).contains(&ch)
        || (0xd7b0..0xd800).contains(&ch)
        || ch == 0x200b
}

pub fn is_space(ch: u32) -> bool {
    matches!(ch, 0x09 | 0x0a | 0x0d | 0x0c)
        || matches!(
            category(ch),
            Category::SpaceSeparator | Category::LineSeparator | Category::ParagraphSeparator
        )
}

pub fn is_mark(ch: u32) -> bool {
    category(ch).is_mark()
}

pub fn is_lower(ch: u32) -> bool {
    category(ch) == Category::LowercaseLetter
}

pub fn is_alnum(ch: u32) -> bool {
    let category = category(ch);
    category.is_letter() || category.is_number()
}

const HANGUL_S_BASE: u32 = 0xac00;
const HANGUL_L_BASE: u32 = 0x1100;
const HANGUL_V_BASE: u32 = 0x1161;
const HANGUL_T_BASE: u32 = 0x11a7;
const HANGUL_T_COUNT: u32 = 28;
const HANGUL_N_COUNT: u32 = 21 * HANGUL_T_COUNT;
const HANGUL_S_COUNT: u32 = 19 * HANGUL_N_COUNT;

pub fn decompose(ch: u32) -> Option<(u32, u32)> {
    if (HANGUL_S_BASE..HANGUL_S_BASE + HANGUL_S_COUNT).contains(&ch) {
        let index = ch - HANGUL_S_BASE;
        let trailing = index % HANGUL_T_COUNT;
        return Some(if trailing != 0 {
            (ch - trailing, HANGUL_T_BASE + trailing)
        } else {
            (
                HANGUL_L_BASE + index / HANGUL_N_COUNT,
                HANGUL_V_BASE + (index % HANGUL_N_COUNT) / HANGUL_T_COUNT,
            )
        });
    }
    let index = tables::DECOMPOSITION_KEYS.binary_search(&ch).ok()?;
    Some(tables::DECOMPOSITION_VALUES[index])
}
