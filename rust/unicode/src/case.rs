//! Southstar unicode — case mapping: simple per-character mappings, the full mappings of SpecialCasing.txt for strings and case folding.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::iter::Peekable;
use std::str::Chars;

use crate::{Category, category, tables};

const GREEK_CAPITAL_SIGMA: u32 = 0x03a3;
const GREEK_SMALL_SIGMA: u32 = 0x03c3;
const GREEK_SMALL_FINAL_SIGMA: u32 = 0x03c2;
const COMBINING_YPOGEGRAMMENI: u32 = 0x0345;
const COMBINING_DOT_ABOVE: u32 = 0x0307;
const GREEK_CAPITAL_IOTA: u32 = 0x0399;

fn simple(keys: &[u32], values: &[u32], ch: u32) -> Option<u32> {
    keys.binary_search(&ch).ok().map(|index| values[index])
}

fn simple_upper(ch: u32) -> Option<u32> {
    simple(&tables::UPPER_KEYS, &tables::UPPER_VALUES, ch)
}

fn simple_lower(ch: u32) -> Option<u32> {
    simple(&tables::LOWER_KEYS, &tables::LOWER_VALUES, ch)
}

#[derive(Clone, Copy)]
enum Full {
    Lower = 0,
    Upper = 2,
}

fn special(ch: u32, which: Full) -> Option<&'static [u32]> {
    let index = tables::SPECIAL_KEYS.binary_search(&ch).ok()?;
    let packed = tables::SPECIAL_VALUES[index][which as usize];
    let start = (packed >> 8) as usize;
    let len = (packed & 0xff) as usize;
    Some(&tables::SPECIAL_POOL[start..start + len])
}

fn title_entry(ch: u32) -> Option<&'static [u32; 3]> {
    tables::TITLE_TABLE.iter().find(|entry| entry[0] == ch)
}

fn nonzero(ch: u32, fallback: u32) -> u32 {
    if ch == 0 { fallback } else { ch }
}

pub fn to_upper(ch: u32) -> u32 {
    match category(ch) {
        Category::LowercaseLetter => simple_upper(ch).unwrap_or(ch),
        Category::TitlecaseLetter => title_entry(ch).map_or(ch, |entry| nonzero(entry[1], ch)),
        _ => ch,
    }
}

pub fn to_lower(ch: u32) -> u32 {
    match category(ch) {
        Category::UppercaseLetter => simple_lower(ch).unwrap_or(ch),
        Category::TitlecaseLetter => title_entry(ch).map_or(ch, |entry| entry[2]),
        _ => ch,
    }
}

pub fn to_title(ch: u32) -> u32 {
    if ch == 0 {
        return ch;
    }
    if let Some(entry) = tables::TITLE_TABLE.iter().find(|entry| entry.contains(&ch)) {
        return entry[0];
    }
    if category(ch) == Category::LowercaseLetter {
        return to_upper(ch);
    }
    ch
}

fn push(out: &mut String, ch: u32) {
    out.push(char::from_u32(ch).unwrap_or(char::REPLACEMENT_CHARACTER));
}

fn push_marks(chars: &mut Peekable<Chars<'_>>, out: &mut String) {
    while let Some(&next) = chars.peek() {
        let next = u32::from(next);
        if !category(next).is_mark() {
            break;
        }
        if next != COMBINING_DOT_ABOVE {
            push(out, next);
        }
        chars.next();
    }
}

pub fn upper_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        let ch = u32::from(character);
        if ch == COMBINING_YPOGEGRAMMENI {
            push_marks(&mut chars, &mut out);
            push(&mut out, GREEK_CAPITAL_IOTA);
            continue;
        }
        match category(ch) {
            Category::LowercaseLetter | Category::TitlecaseLetter => {
                match special(ch, Full::Upper) {
                    Some(full) => full.iter().for_each(|&c| push(&mut out, c)),
                    None => push(&mut out, to_upper(ch)),
                }
            }
            _ => out.push(character),
        }
    }
    out
}

pub fn lower_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        let ch = u32::from(character);
        if ch == GREEK_CAPITAL_SIGMA {
            let medial = chars
                .peek()
                .is_some_and(|&next| category(u32::from(next)).is_letter());
            push(
                &mut out,
                if medial {
                    GREEK_SMALL_SIGMA
                } else {
                    GREEK_SMALL_FINAL_SIGMA
                },
            );
            continue;
        }
        match category(ch) {
            Category::UppercaseLetter | Category::TitlecaseLetter => {
                match special(ch, Full::Lower) {
                    Some(full) => full.iter().for_each(|&c| push(&mut out, c)),
                    None => push(&mut out, nonzero(to_lower(ch), ch)),
                }
            }
            _ => out.push(character),
        }
    }
    out
}

pub fn fold_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        let ch = u32::from(character);
        match tables::FOLD_KEYS.binary_search(&ch) {
            Ok(index) => {
                let packed = tables::FOLD_VALUES[index];
                let start = (packed >> 8) as usize;
                let len = (packed & 0xff) as usize;
                tables::FOLD_POOL[start..start + len]
                    .iter()
                    .for_each(|&c| push(&mut out, c));
            }
            Err(_) => push(&mut out, to_lower(ch)),
        }
    }
    out
}
