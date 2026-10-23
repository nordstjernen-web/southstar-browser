//! Southstar text — Unicode character properties from GLib's database, with GLib's numeric category and line break codes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::ffi::glib;
use crate::script::Script;

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum UnicodeType {
    Control = 0,
    Format = 1,
    Unassigned = 2,
    PrivateUse = 3,
    Surrogate = 4,
    LowercaseLetter = 5,
    ModifierLetter = 6,
    OtherLetter = 7,
    TitlecaseLetter = 8,
    UppercaseLetter = 9,
    SpacingMark = 10,
    EnclosingMark = 11,
    NonSpacingMark = 12,
    DecimalNumber = 13,
    LetterNumber = 14,
    OtherNumber = 15,
    ConnectPunctuation = 16,
    DashPunctuation = 17,
    ClosePunctuation = 18,
    FinalPunctuation = 19,
    InitialPunctuation = 20,
    OtherPunctuation = 21,
    OpenPunctuation = 22,
    CurrencySymbol = 23,
    ModifierSymbol = 24,
    MathSymbol = 25,
    OtherSymbol = 26,
    LineSeparator = 27,
    ParagraphSeparator = 28,
    SpaceSeparator = 29,
}

const UNICODE_TYPES: [UnicodeType; 30] = [
    UnicodeType::Control,
    UnicodeType::Format,
    UnicodeType::Unassigned,
    UnicodeType::PrivateUse,
    UnicodeType::Surrogate,
    UnicodeType::LowercaseLetter,
    UnicodeType::ModifierLetter,
    UnicodeType::OtherLetter,
    UnicodeType::TitlecaseLetter,
    UnicodeType::UppercaseLetter,
    UnicodeType::SpacingMark,
    UnicodeType::EnclosingMark,
    UnicodeType::NonSpacingMark,
    UnicodeType::DecimalNumber,
    UnicodeType::LetterNumber,
    UnicodeType::OtherNumber,
    UnicodeType::ConnectPunctuation,
    UnicodeType::DashPunctuation,
    UnicodeType::ClosePunctuation,
    UnicodeType::FinalPunctuation,
    UnicodeType::InitialPunctuation,
    UnicodeType::OtherPunctuation,
    UnicodeType::OpenPunctuation,
    UnicodeType::CurrencySymbol,
    UnicodeType::ModifierSymbol,
    UnicodeType::MathSymbol,
    UnicodeType::OtherSymbol,
    UnicodeType::LineSeparator,
    UnicodeType::ParagraphSeparator,
    UnicodeType::SpaceSeparator,
];

impl UnicodeType {
    pub fn from_i32(value: i32) -> Option<UnicodeType> {
        usize::try_from(value)
            .ok()
            .and_then(|index| UNICODE_TYPES.get(index).copied())
    }
}

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum BreakType {
    Mandatory = 0,
    CarriageReturn = 1,
    LineFeed = 2,
    CombiningMark = 3,
    Surrogate = 4,
    ZeroWidthSpace = 5,
    Inseparable = 6,
    NonBreakingGlue = 7,
    Contingent = 8,
    Space = 9,
    After = 10,
    Before = 11,
    BeforeAndAfter = 12,
    Hyphen = 13,
    NonStarter = 14,
    OpenPunctuation = 15,
    ClosePunctuation = 16,
    Quotation = 17,
    Exclamation = 18,
    Ideographic = 19,
    Numeric = 20,
    InfixSeparator = 21,
    Symbol = 22,
    Alphabetic = 23,
    Prefix = 24,
    Postfix = 25,
    ComplexContext = 26,
    Ambiguous = 27,
    Unknown = 28,
    NextLine = 29,
    WordJoiner = 30,
    HangulLJamo = 31,
    HangulVJamo = 32,
    HangulTJamo = 33,
    HangulLvSyllable = 34,
    HangulLvtSyllable = 35,
    CloseParenthesis = 36,
    ConditionalJapaneseStarter = 37,
    HebrewLetter = 38,
    RegionalIndicator = 39,
    EmojiBase = 40,
    EmojiModifier = 41,
    ZeroWidthJoiner = 42,
    Aksara = 43,
    AksaraPreBase = 44,
    AksaraStart = 45,
    ViramaFinal = 46,
    Virama = 47,
    UnambiguousHyphen = 48,
}

const BREAK_TYPES: [BreakType; 49] = [
    BreakType::Mandatory,
    BreakType::CarriageReturn,
    BreakType::LineFeed,
    BreakType::CombiningMark,
    BreakType::Surrogate,
    BreakType::ZeroWidthSpace,
    BreakType::Inseparable,
    BreakType::NonBreakingGlue,
    BreakType::Contingent,
    BreakType::Space,
    BreakType::After,
    BreakType::Before,
    BreakType::BeforeAndAfter,
    BreakType::Hyphen,
    BreakType::NonStarter,
    BreakType::OpenPunctuation,
    BreakType::ClosePunctuation,
    BreakType::Quotation,
    BreakType::Exclamation,
    BreakType::Ideographic,
    BreakType::Numeric,
    BreakType::InfixSeparator,
    BreakType::Symbol,
    BreakType::Alphabetic,
    BreakType::Prefix,
    BreakType::Postfix,
    BreakType::ComplexContext,
    BreakType::Ambiguous,
    BreakType::Unknown,
    BreakType::NextLine,
    BreakType::WordJoiner,
    BreakType::HangulLJamo,
    BreakType::HangulVJamo,
    BreakType::HangulTJamo,
    BreakType::HangulLvSyllable,
    BreakType::HangulLvtSyllable,
    BreakType::CloseParenthesis,
    BreakType::ConditionalJapaneseStarter,
    BreakType::HebrewLetter,
    BreakType::RegionalIndicator,
    BreakType::EmojiBase,
    BreakType::EmojiModifier,
    BreakType::ZeroWidthJoiner,
    BreakType::Aksara,
    BreakType::AksaraPreBase,
    BreakType::AksaraStart,
    BreakType::ViramaFinal,
    BreakType::Virama,
    BreakType::UnambiguousHyphen,
];

impl BreakType {
    pub fn from_i32(value: i32) -> Option<BreakType> {
        usize::try_from(value)
            .ok()
            .and_then(|index| BREAK_TYPES.get(index).copied())
    }
}

pub fn unichar_type(ch: u32) -> UnicodeType {
    UnicodeType::from_i32(glib::unichar_type(ch)).unwrap_or(UnicodeType::Unassigned)
}

pub fn break_type(ch: u32) -> BreakType {
    BreakType::from_i32(glib::unichar_break_type(ch)).unwrap_or(BreakType::Unknown)
}

pub fn script(ch: u32) -> Script {
    Script(glib::unichar_get_script(ch))
}

pub fn mirror_char(ch: u32) -> Option<u32> {
    glib::unichar_get_mirror_char(ch)
}

pub fn is_zero_width(ch: u32) -> bool {
    glib::unichar_iszerowidth(ch)
}

pub fn is_space(ch: u32) -> bool {
    glib::unichar_isspace(ch)
}

pub fn is_mark(ch: u32) -> bool {
    glib::unichar_ismark(ch)
}

pub fn is_wide(ch: u32) -> bool {
    glib::unichar_iswide(ch)
}

pub fn combining_class(ch: u32) -> i32 {
    glib::unichar_combining_class(ch)
}

pub fn to_lower(ch: u32) -> u32 {
    glib::unichar_tolower(ch)
}

pub fn to_upper(ch: u32) -> u32 {
    glib::unichar_toupper(ch)
}

pub fn to_title(ch: u32) -> u32 {
    glib::unichar_totitle(ch)
}

pub fn char_at(text: &str, index: usize) -> u32 {
    text.get(index..)
        .and_then(|rest| rest.chars().next())
        .map_or(0, u32::from)
}

pub fn next_char_index(text: &str, index: usize) -> usize {
    text.get(index..)
        .and_then(|rest| rest.chars().next())
        .map_or(index + 1, |ch| index + ch.len_utf8())
}

pub fn utf8_strlen(text: &str) -> usize {
    text.chars().count()
}

pub fn byte_to_char_offset(text: &str, index: usize) -> usize {
    text.char_indices().take_while(|&(i, _)| i < index).count()
}

pub fn char_offset_to_byte(text: &str, offset: usize) -> usize {
    text.char_indices()
        .nth(offset)
        .map_or(text.len(), |(i, _)| i)
}
