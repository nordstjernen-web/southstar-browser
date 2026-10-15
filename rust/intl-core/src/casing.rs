//! Southstar — case conversion with the language-sensitive SpecialCasing rules for Turkish, Azeri and Lithuanian.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const DOT_ABOVE: char = '\u{307}';
const ABOVE: u8 = 230;

const SOFT_DOTTED: [(char, char); 18] = [
    ('i', 'j'),
    ('\u{12f}', '\u{12f}'),
    ('\u{249}', '\u{249}'),
    ('\u{268}', '\u{268}'),
    ('\u{29d}', '\u{29d}'),
    ('\u{2b2}', '\u{2b2}'),
    ('\u{3f3}', '\u{3f3}'),
    ('\u{456}', '\u{456}'),
    ('\u{458}', '\u{458}'),
    ('\u{1d62}', '\u{1d62}'),
    ('\u{1d96}', '\u{1d96}'),
    ('\u{1da4}', '\u{1da4}'),
    ('\u{1da8}', '\u{1da8}'),
    ('\u{1e2d}', '\u{1e2d}'),
    ('\u{1ecb}', '\u{1ecb}'),
    ('\u{2071}', '\u{2071}'),
    ('\u{2148}', '\u{2149}'),
    ('\u{2c7c}', '\u{2c7c}'),
];

const MATH_SMALL_I: char = '\u{1d422}';
const MATH_SMALL_J_LAST: char = '\u{1d693}';
const MATH_ALPHABET_STRIDE: u32 = 52;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaseRules {
    Turkic,
    Lithuanian,
    Default,
}

impl CaseRules {
    pub fn for_language(language: &str) -> CaseRules {
        match language {
            "tr" | "az" => CaseRules::Turkic,
            "lt" => CaseRules::Lithuanian,
            _ => CaseRules::Default,
        }
    }
}

fn class(c: char) -> u8 {
    southstar_url::combining_class(c)
}

fn is_soft_dotted(c: char) -> bool {
    let math_letter = (MATH_SMALL_I..=MATH_SMALL_J_LAST).contains(&c)
        && (c as u32 - MATH_SMALL_I as u32) % MATH_ALPHABET_STRIDE < 2;
    math_letter
        || SOFT_DOTTED
            .iter()
            .any(|&(first, last)| (first..=last).contains(&c))
}

fn more_above(chars: &[char], at: usize) -> bool {
    for &c in &chars[at + 1..] {
        match class(c) {
            ABOVE => return true,
            0 => return false,
            _ => {}
        }
    }
    false
}

fn followed_by_dot_above(chars: &[char], at: usize) -> bool {
    for &c in &chars[at + 1..] {
        if c == DOT_ABOVE {
            return true;
        }
        if matches!(class(c), 0 | ABOVE) {
            return false;
        }
    }
    false
}

fn preceding_base(chars: &[char], at: usize) -> Option<char> {
    chars[..at]
        .iter()
        .rev()
        .find(|&&c| matches!(class(c), 0 | ABOVE))
        .copied()
}

fn turkic_lower(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    for (at, &c) in chars.iter().enumerate() {
        match c {
            '\u{130}' => out.push('i'),
            'I' if followed_by_dot_above(chars, at) => out.push('i'),
            'I' => out.push('\u{131}'),
            DOT_ABOVE if preceding_base(chars, at) == Some('I') => {}
            _ => out.push(c),
        }
    }
    out.to_lowercase()
}

fn lithuanian_lower(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    for (at, &c) in chars.iter().enumerate() {
        match c {
            'I' | 'J' | '\u{12e}' if more_above(chars, at) => {
                out.extend(c.to_lowercase());
                out.push(DOT_ABOVE);
            }
            '\u{cc}' => out.push_str("i\u{307}\u{300}"),
            '\u{cd}' => out.push_str("i\u{307}\u{301}"),
            '\u{128}' => out.push_str("i\u{307}\u{303}"),
            _ => out.push(c),
        }
    }
    out.to_lowercase()
}

fn turkic_upper(chars: &[char]) -> String {
    let replaced: String = chars
        .iter()
        .map(|&c| if c == 'i' { '\u{130}' } else { c })
        .collect();
    replaced.to_uppercase()
}

fn lithuanian_upper(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    for (at, &c) in chars.iter().enumerate() {
        if c == DOT_ABOVE && preceding_base(chars, at).is_some_and(is_soft_dotted) {
            continue;
        }
        out.push(c);
    }
    out.to_uppercase()
}

pub fn to_lower(text: &str, rules: CaseRules) -> String {
    let chars: Vec<char> = text.chars().collect();
    match rules {
        CaseRules::Turkic => turkic_lower(&chars),
        CaseRules::Lithuanian => lithuanian_lower(&chars),
        CaseRules::Default => text.to_lowercase(),
    }
}

pub fn to_upper(text: &str, rules: CaseRules) -> String {
    let chars: Vec<char> = text.chars().collect();
    match rules {
        CaseRules::Turkic => turkic_upper(&chars),
        CaseRules::Lithuanian => lithuanian_upper(&chars),
        CaseRules::Default => text.to_uppercase(),
    }
}
