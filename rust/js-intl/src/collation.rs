//! Southstar — a compact multi-level collation in root order, with numeric runs and the German phonebook tailoring.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::cmp::Ordering;

const VARIABLE: u32 = 1 << 24;
const DIGIT: u32 = 2 << 24;
const LETTER: u32 = 3 << 24;
const DIAERESIS: char = '\u{308}';
const TAILORED_SECONDARY: u32 = 1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Strength {
    Base,
    Accent,
    Case,
    Variant,
}

impl Strength {
    pub(crate) fn from_name(name: &str) -> Strength {
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

pub(crate) struct Settings {
    pub(crate) strength: Strength,
    pub(crate) ignore_punctuation: bool,
    pub(crate) numeric: bool,
    pub(crate) upper_first: bool,
    pub(crate) umlaut_as_e: bool,
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
    Some((base, c as u32))
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn is_ignorable(c: char) -> bool {
    c.is_control() || matches!(c, '\u{200b}'..='\u{200f}' | '\u{2060}'..='\u{2064}' | '\u{feff}')
}

struct Builder<'a> {
    settings: &'a Settings,
    elements: Vec<Element>,
    skipping_marks: bool,
}

impl Builder<'_> {
    fn letter(&mut self, c: char, accent: Option<u32>) {
        let mut element = Element {
            primary: LETTER | lower(c) as u32,
            upper: c.is_uppercase(),
            ..Element::default()
        };
        element.secondary.extend(accent);
        self.elements.push(element);
    }

    fn mark(&mut self, c: char) {
        if self.skipping_marks {
            return;
        }
        let Some(last) = self.elements.last_mut() else {
            return;
        };
        if self.settings.umlaut_as_e
            && c == DIAERESIS
            && last.secondary.is_empty()
            && ['a', 'o', 'u']
                .iter()
                .any(|&v| last.primary == LETTER | v as u32)
        {
            let upper = last.upper;
            self.elements.push(Element {
                primary: LETTER | 'e' as u32,
                secondary: vec![TAILORED_SECONDARY],
                upper,
            });
            return;
        }
        last.secondary.push(c as u32 + 2);
    }

    fn digits(&mut self, run: &[u32]) {
        if !self.settings.numeric {
            for &digit in run {
                self.elements.push(Element {
                    primary: DIGIT | digit,
                    ..Element::default()
                });
            }
            return;
        }
        let significant: Vec<u32> = run.iter().copied().skip_while(|&d| d == 0).collect();
        let digits = if significant.is_empty() {
            vec![0]
        } else {
            significant
        };
        self.elements.push(Element {
            primary: DIGIT | (digits.len() as u32).min(0xff_ffff),
            ..Element::default()
        });
        for digit in digits {
            self.elements.push(Element {
                primary: DIGIT | digit,
                ..Element::default()
            });
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
            if let Some(digit) = c.to_digit(10) {
                let mut run = vec![digit];
                while let Some(next) = decomposed.get(at).and_then(|d| d.to_digit(10)) {
                    run.push(next);
                    at += 1;
                }
                self.digits(&run);
            } else if let Some(expanded) = expansion(c) {
                for part in expanded.chars() {
                    self.letter(part, None);
                }
            } else if let Some((base, accent)) = secondary_base(c) {
                self.letter(base, Some(accent));
            } else if c.is_alphanumeric() {
                self.letter(c, None);
            } else if self.settings.ignore_punctuation {
                self.skipping_marks = true;
            } else {
                self.elements.push(Element {
                    primary: VARIABLE | c as u32,
                    ..Element::default()
                });
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

pub(crate) fn compare(a: &str, b: &str, settings: &Settings) -> Ordering {
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
