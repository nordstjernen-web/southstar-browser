//! Southstar — a compact multi-level collation in root order, with numeric runs and the German phonebook tailoring.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;

const CLASS_SHIFT: u32 = 28;
const VARIABLE: u32 = 1 << CLASS_SHIFT;
const SYMBOL: u32 = 2 << CLASS_SHIFT;
const DIGIT: u32 = 3 << CLASS_SHIFT;
const LETTER: u32 = 4 << CLASS_SHIFT;
const DIAERESIS: char = '\u{308}';
const TAILORED_SECONDARY: u32 = 1;
const RANKED_MARK_BASE: u32 = 2;
const UNRANKED_MARK_BASE: u32 = 0x1000;
const EXPANDED_SECONDARY: u32 = 0x20_0000;
const PUNCTUATION_ORDER: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`";
const SYMBOL_ORDER: &str = "^+<=>|~$";

const MARK_ORDER: [u16; 111] = [
    0x332, 0x313, 0x343, 0x314, 0x301, 0x341, 0x300, 0x340, 0x306, 0x302, 0x30c, 0x30a, 0x342,
    0x308, 0x344, 0x30b, 0x303, 0x307, 0x338, 0x327, 0x328, 0x304, 0x30d, 0x30e, 0x312, 0x315,
    0x31a, 0x33d, 0x33e, 0x33f, 0x346, 0x34a, 0x34b, 0x34c, 0x350, 0x351, 0x352, 0x357, 0x35b,
    0x35d, 0x35e, 0x316, 0x317, 0x318, 0x319, 0x31c, 0x31d, 0x31e, 0x31f, 0x320, 0x329, 0x32a,
    0x32b, 0x32c, 0x32f, 0x333, 0x33a, 0x33b, 0x33c, 0x347, 0x348, 0x349, 0x34d, 0x34e, 0x353,
    0x354, 0x355, 0x356, 0x359, 0x35a, 0x35c, 0x35f, 0x362, 0x336, 0x337, 0x335, 0x305, 0x309,
    0x30f, 0x310, 0x311, 0x31b, 0x321, 0x322, 0x323, 0x324, 0x325, 0x326, 0x32d, 0x32e, 0x330,
    0x331, 0x334, 0x339, 0x345, 0x358, 0x360, 0x361, 0x363, 0x368, 0x369, 0x364, 0x36a, 0x365,
    0x36b, 0x366, 0x36c, 0x36d, 0x367, 0x36e, 0x36f,
];

const DIGIT_ZEROS: [u32; 20] = [
    0x30, 0x660, 0x6f0, 0x7c0, 0x966, 0x9e6, 0xa66, 0xae6, 0xb66, 0xbe6, 0xc66, 0xce6, 0xd66,
    0xde6, 0xe50, 0xed0, 0xf20, 0x1040, 0x17e0, 0xff10,
];

const LATIN_PUNCTUATION: &str = "¡§«¶·»¿";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Strength {
    Base,
    Accent,
    Case,
    Variant,
}

impl Strength {
    pub fn from_name(name: &str) -> Strength {
        match name {
            "base" => Strength::Base,
            "accent" => Strength::Accent,
            "case" => Strength::Case,
            _ => Strength::Variant,
        }
    }

    fn compares_accents(self) -> bool {
        matches!(self, Strength::Accent | Strength::Variant)
    }

    fn compares_case(self) -> bool {
        matches!(self, Strength::Case | Strength::Variant)
    }
}

pub struct Settings {
    pub strength: Strength,
    pub ignore_punctuation: bool,
    pub numeric: bool,
    pub upper_first: bool,
    pub umlaut_as_e: bool,
}

#[derive(Default)]
struct Element {
    primary: u32,
    secondary: Vec<u32>,
    upper: bool,
}

fn expansion(c: char) -> Option<&'static str> {
    match c {
        'ß' => Some("ss"),
        'ẞ' => Some("SS"),
        'æ' => Some("ae"),
        'Æ' => Some("AE"),
        'œ' => Some("oe"),
        'Œ' => Some("OE"),
        _ => None,
    }
}

fn secondary_base(c: char) -> Option<(char, u32)> {
    let base = match c {
        'ø' => 'o',
        'Ø' => 'O',
        'đ' => 'd',
        'Đ' => 'D',
        'ł' => 'l',
        'Ł' => 'L',
        'ħ' => 'h',
        'Ħ' => 'H',
        'ŧ' => 't',
        'Ŧ' => 'T',
        _ => return None,
    };
    Some((base, UNRANKED_MARK_BASE + c as u32))
}

fn letter_after(c: char) -> Option<char> {
    match c {
        'ð' | 'Ð' => Some('d'),
        'ı' => Some('i'),
        'ŋ' | 'Ŋ' => Some('n'),
        'ĸ' => Some('q'),
        _ => None,
    }
}

fn mark_weight(c: char) -> u32 {
    let code = c as u32;
    MARK_ORDER
        .iter()
        .position(|&mark| u32::from(mark) == code)
        .map_or(UNRANKED_MARK_BASE + code, |rank| {
            RANKED_MARK_BASE + rank as u32
        })
}

fn digit_value(c: char) -> Option<u32> {
    let code = c as u32;
    DIGIT_ZEROS
        .iter()
        .find(|&&zero| (zero..zero + 10).contains(&code))
        .map(|zero| code - zero)
}

fn is_punctuation(c: char) -> bool {
    if c.is_whitespace() {
        return true;
    }
    if c.is_ascii() {
        return PUNCTUATION_ORDER.contains(c);
    }
    LATIN_PUNCTUATION.contains(c)
        || matches!(
            c,
            '\u{2010}'..='\u{2027}'
                | '\u{2030}'..='\u{205e}'
                | '\u{3001}'..='\u{3003}'
                | '\u{3008}'..='\u{3011}'
                | '\u{3014}'..='\u{301f}'
                | '\u{fe10}'..='\u{fe19}'
                | '\u{fe30}'..='\u{fe4f}'
                | '\u{ff01}'..='\u{ff03}'
                | '\u{ff05}'..='\u{ff0a}'
                | '\u{ff0c}'..='\u{ff0f}'
                | '\u{ff1a}'..='\u{ff1b}'
                | '\u{ff1f}'..='\u{ff20}'
                | '\u{ff3b}'..='\u{ff3d}'
                | '\u{ff3f}'
                | '\u{ff5b}'
                | '\u{ff5d}'
                | '\u{ff5f}'..='\u{ff65}'
                | '\u{60c}'
                | '\u{61b}'
                | '\u{61f}'
                | '\u{66a}'..='\u{66d}'
                | '\u{6d4}'
                | '\u{964}'..='\u{965}'
        )
}

fn ordered_weight(class: u32, order: &str, c: char) -> u32 {
    let rank = order
        .find(c)
        .map_or(order.len() as u32 + c as u32, |at| at as u32);
    class | rank
}

fn letter_weight(c: char) -> u32 {
    let (base, offset) = match letter_after(c) {
        Some(base) => (base, 1),
        None => (c.to_lowercase().next().unwrap_or(c), 0),
    };
    LETTER | ((base as u32) << 4) | offset
}

fn is_ignorable(c: char) -> bool {
    (c.is_control() && !c.is_whitespace())
        || matches!(
            c,
            '\u{34f}' | '\u{200b}'..='\u{200f}' | '\u{2060}'..='\u{2064}' | '\u{feff}'
        )
}

struct Builder<'a> {
    settings: &'a Settings,
    elements: Vec<Element>,
    skipping_marks: bool,
}

impl Builder<'_> {
    fn push(&mut self, primary: u32, upper: bool, accent: Option<u32>) {
        self.elements.push(Element {
            primary,
            secondary: accent.into_iter().collect(),
            upper,
        });
    }

    fn letter(&mut self, c: char, accent: Option<u32>) {
        self.push(letter_weight(c), c.is_uppercase(), accent);
    }

    fn umlaut_as_e(&mut self) -> bool {
        let Some(last) = self.elements.last_mut() else {
            return false;
        };
        let vowel = ['a', 'o', 'u']
            .iter()
            .any(|&v| last.primary == letter_weight(v));
        if !self.settings.umlaut_as_e || !last.secondary.is_empty() || !vowel {
            return false;
        }
        last.secondary.push(TAILORED_SECONDARY);
        let upper = last.upper;
        self.push(letter_weight('e'), upper, None);
        true
    }

    fn mark(&mut self, c: char) {
        if self.skipping_marks || (c == DIAERESIS && self.umlaut_as_e()) {
            return;
        }
        if let Some(last) = self.elements.last_mut() {
            last.secondary.push(mark_weight(c));
        }
    }

    fn digits(&mut self, run: &[u32]) {
        if !self.settings.numeric {
            for &digit in run {
                self.push(DIGIT | digit, false, None);
            }
            return;
        }
        let significant: Vec<u32> = run.iter().copied().skip_while(|&d| d == 0).collect();
        let digits = if significant.is_empty() {
            vec![0]
        } else {
            significant
        };
        self.push(DIGIT | (digits.len() as u32).min(0xfff_ffff), false, None);
        for digit in digits {
            self.push(DIGIT | digit, false, None);
        }
    }

    fn character(&mut self, c: char) {
        if let Some(expanded) = expansion(c) {
            for (index, part) in expanded.chars().enumerate() {
                self.letter(part, (index == 0).then_some(EXPANDED_SECONDARY));
            }
        } else if let Some((base, accent)) = secondary_base(c) {
            self.letter(base, Some(accent));
        } else if c.is_numeric() {
            self.push(DIGIT | (10 + c as u32), false, None);
        } else if c.is_alphanumeric() {
            self.letter(c, None);
        } else if !is_punctuation(c) {
            self.push(ordered_weight(SYMBOL, SYMBOL_ORDER, c), false, None);
        } else if self.settings.ignore_punctuation {
            self.skipping_marks = true;
        } else {
            self.push(ordered_weight(VARIABLE, PUNCTUATION_ORDER, c), false, None);
        }
    }

    fn build(mut self, text: &[char]) -> Vec<Element> {
        let decomposed = southstar_url::nfd(text);
        let mut at = 0;
        while at < decomposed.len() {
            let c = decomposed[at];
            at += 1;
            if southstar_url::combining_class(c) != 0 {
                self.mark(c);
                continue;
            }
            self.skipping_marks = false;
            if is_ignorable(c) {
                continue;
            }
            if let Some(digit) = digit_value(c) {
                let mut run = vec![digit];
                while let Some(next) = decomposed.get(at).copied().and_then(digit_value) {
                    run.push(next);
                    at += 1;
                }
                self.digits(&run);
            } else {
                self.character(c);
            }
        }
        self.elements
    }
}

fn elements(text: &str, settings: &Settings) -> Vec<Element> {
    let chars: Vec<char> = text.chars().collect();
    Builder {
        settings,
        elements: Vec::new(),
        skipping_marks: false,
    }
    .build(&chars)
}

fn compare_level<T: Ord>(a: &[Element], b: &[Element], key: impl Fn(&Element) -> T) -> Ordering {
    a.iter().map(&key).cmp(b.iter().map(&key))
}

fn secondaries(elements: &[Element]) -> Vec<u32> {
    let mut weights = Vec::new();
    for element in elements {
        weights.push(0);
        weights.extend(&element.secondary);
    }
    weights
}

pub fn compare(a: &str, b: &str, settings: &Settings) -> Ordering {
    let a = elements(a, settings);
    let b = elements(b, settings);
    let primary = compare_level(&a, &b, |e| e.primary);
    if primary != Ordering::Equal {
        return primary;
    }
    if settings.strength.compares_accents() {
        let secondary = secondaries(&a).cmp(&secondaries(&b));
        if secondary != Ordering::Equal {
            return secondary;
        }
    }
    if settings.strength.compares_case() {
        return compare_level(&a, &b, |e| e.upper != settings.upper_first);
    }
    Ordering::Equal
}
