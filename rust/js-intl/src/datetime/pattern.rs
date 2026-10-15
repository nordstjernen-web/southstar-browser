//! Southstar — Intl.DateTimeFormat patterns: format records, skeleton matching, rendering to parts and date ranges.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::calendar::{self, CalendarDate, Wall};
use super::data::{self, Cycle, Formats, LocaleData, pick};
use super::zone::{self, NameStyle};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Field {
    Weekday,
    Era,
    Year,
    Month,
    Day,
    DayPeriod,
    Hour,
    Minute,
    Second,
    Fraction,
    ZoneName,
}

pub(crate) const FIELDS: [Field; 11] = [
    Field::Weekday,
    Field::Era,
    Field::Year,
    Field::Month,
    Field::Day,
    Field::DayPeriod,
    Field::Hour,
    Field::Minute,
    Field::Second,
    Field::Fraction,
    Field::ZoneName,
];

const TEXT_STYLES: [&str; 3] = ["narrow", "short", "long"];
const NUMERIC_STYLES: [&str; 2] = ["2-digit", "numeric"];

impl Field {
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    pub(crate) fn option(self) -> &'static str {
        match self {
            Field::Weekday => "weekday",
            Field::Era => "era",
            Field::Year => "year",
            Field::Month => "month",
            Field::Day => "day",
            Field::DayPeriod => "dayPeriod",
            Field::Hour => "hour",
            Field::Minute => "minute",
            Field::Second => "second",
            Field::Fraction => "fractionalSecondDigits",
            Field::ZoneName => "timeZoneName",
        }
    }

    pub(crate) fn values(self) -> &'static [&'static str] {
        match self {
            Field::Weekday | Field::Era | Field::DayPeriod => &TEXT_STYLES,
            Field::Month => &["2-digit", "numeric", "narrow", "short", "long"],
            Field::ZoneName => &[
                "short",
                "long",
                "shortOffset",
                "longOffset",
                "shortGeneric",
                "longGeneric",
            ],
            _ => &NUMERIC_STYLES,
        }
    }

    fn is_date(self) -> bool {
        matches!(
            self,
            Field::Weekday | Field::Era | Field::Year | Field::Month | Field::Day
        )
    }

    fn is_clock(self) -> bool {
        matches!(
            self,
            Field::DayPeriod | Field::Hour | Field::Minute | Field::Second
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Style {
    Numeric,
    TwoDigit,
    Narrow,
    Short,
    Long,
    ShortOffset,
    LongOffset,
    ShortGeneric,
    LongGeneric,
    Digits(u8),
}

impl Style {
    pub(crate) fn parse(text: &str) -> Option<Style> {
        Some(match text {
            "numeric" => Style::Numeric,
            "2-digit" => Style::TwoDigit,
            "narrow" => Style::Narrow,
            "short" => Style::Short,
            "long" => Style::Long,
            "shortOffset" => Style::ShortOffset,
            "longOffset" => Style::LongOffset,
            "shortGeneric" => Style::ShortGeneric,
            "longGeneric" => Style::LongGeneric,
            _ => return None,
        })
    }

    pub(crate) fn name(self) -> &'static str {
        match self {
            Style::Numeric => "numeric",
            Style::TwoDigit => "2-digit",
            Style::Narrow => "narrow",
            Style::Short => "short",
            Style::Long => "long",
            Style::ShortOffset => "shortOffset",
            Style::LongOffset => "longOffset",
            Style::ShortGeneric => "shortGeneric",
            Style::LongGeneric => "longGeneric",
            Style::Digits(_) => "",
        }
    }

    fn text_width(self) -> u8 {
        match self {
            Style::Narrow => 5,
            Style::Long => 4,
            _ => 3,
        }
    }
}

pub(crate) type Request = [Option<Style>; 11];

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Token {
    Text(String),
    Field(u8, u8),
}

#[derive(Clone)]
pub(crate) struct Format {
    pub(crate) fields: Request,
    pub(crate) tokens: Vec<Token>,
}

impl Format {
    pub(crate) fn has(&self, field: Field) -> bool {
        self.fields[field.index()].is_some()
    }
}

fn push_text(tokens: &mut Vec<Token>, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Token::Text(last)) = tokens.last_mut() {
        last.push_str(text);
    } else {
        tokens.push(Token::Text(text.to_string()));
    }
}

fn extend(tokens: &mut Vec<Token>, more: Vec<Token>) {
    for token in more {
        match token {
            Token::Text(text) => push_text(tokens, &text),
            field => tokens.push(field),
        }
    }
}

pub(crate) fn parse_pattern(pattern: &str) -> Vec<Token> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        if c == '\'' {
            if chars.get(at + 1) == Some(&'\'') {
                push_text(&mut tokens, "'");
                at += 2;
                continue;
            }
            let start = at + 1;
            at = start;
            while at < chars.len() && chars[at] != '\'' {
                at += 1;
            }
            let quoted: String = chars[start..at.min(chars.len())].iter().collect();
            push_text(&mut tokens, &quoted);
            at += 1;
        } else if c.is_ascii_alphabetic() {
            let start = at;
            while at < chars.len() && chars[at] == c {
                at += 1;
            }
            tokens.push(Token::Field(c as u8, (at - start) as u8));
        } else {
            push_text(&mut tokens, &c.to_string());
            at += 1;
        }
    }
    tokens
}

fn text_style(count: u8) -> Style {
    match count {
        4 => Style::Long,
        5 => Style::Narrow,
        _ => Style::Short,
    }
}

fn digit_style(count: u8) -> Style {
    if count == 2 {
        Style::TwoDigit
    } else {
        Style::Numeric
    }
}

pub(crate) fn token_field(letter: u8, count: u8) -> Option<(Field, Style)> {
    Some(match letter {
        b'G' => (Field::Era, text_style(count)),
        b'y' | b'r' => (Field::Year, digit_style(count)),
        b'M' | b'L' if count <= 2 => (Field::Month, digit_style(count)),
        b'M' | b'L' => (Field::Month, text_style(count)),
        b'd' => (Field::Day, digit_style(count)),
        b'E' | b'c' => (Field::Weekday, text_style(count)),
        b'B' => (Field::DayPeriod, text_style(count)),
        b'h' | b'H' | b'K' | b'k' => (Field::Hour, digit_style(count)),
        b'm' => (Field::Minute, digit_style(count)),
        b's' => (Field::Second, digit_style(count)),
        b'S' => (Field::Fraction, Style::Digits(count)),
        b'z' if count >= 4 => (Field::ZoneName, Style::Long),
        b'z' => (Field::ZoneName, Style::Short),
        b'O' if count >= 4 => (Field::ZoneName, Style::LongOffset),
        b'O' => (Field::ZoneName, Style::ShortOffset),
        b'v' if count >= 4 => (Field::ZoneName, Style::LongGeneric),
        b'v' => (Field::ZoneName, Style::ShortGeneric),
        _ => return None,
    })
}

pub(crate) fn fields_of(tokens: &[Token]) -> Request {
    let mut fields: Request = [None; 11];
    for token in tokens {
        if let Token::Field(letter, count) = token
            && let Some((field, style)) = token_field(*letter, *count)
        {
            fields[field.index()] = Some(style);
        }
    }
    fields
}

fn is_text(field: Field, style: Style) -> bool {
    match field {
        Field::Month => !matches!(style, Style::Numeric | Style::TwoDigit),
        Field::Weekday | Field::Era | Field::DayPeriod => true,
        _ => false,
    }
}

fn width(style: Style) -> u8 {
    match style {
        Style::Numeric => 1,
        Style::TwoDigit => 2,
        other => other.text_width(),
    }
}

fn distance(field: Field, wanted: Style, found: Style) -> u32 {
    if is_text(field, wanted) != is_text(field, found) {
        return 3000;
    }
    u32::from(width(wanted).abs_diff(width(found)))
}

type Skeleton = Vec<(Field, Style, u8)>;

fn skeleton(key: &str) -> Skeleton {
    let bytes = key.as_bytes();
    let mut fields = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let letter = bytes[at];
        let start = at;
        while at < bytes.len() && bytes[at] == letter {
            at += 1;
        }
        if let Some((field, style)) = token_field(letter, (at - start) as u8) {
            fields.push((field, style, letter));
        }
    }
    fields
}

fn field_token(field: Field, style: Style, cycle: Cycle) -> Token {
    let count = match field {
        Field::Weekday | Field::Era | Field::DayPeriod | Field::Month if is_text(field, style) => {
            style.text_width()
        }
        _ => width(style),
    };
    let count = match (field, count) {
        (Field::Era | Field::DayPeriod, 3) => 1,
        _ => count,
    };
    let letter = match field {
        Field::Weekday => b'E',
        Field::Era => b'G',
        Field::Year => b'y',
        Field::Month => b'M',
        Field::Day => b'd',
        Field::DayPeriod => b'B',
        Field::Hour => cycle.letter(),
        Field::Minute => b'm',
        Field::Second => b's',
        Field::Fraction => b'S',
        Field::ZoneName => b'z',
    };
    Token::Field(letter, count)
}

fn adjust(token: &mut Token, wanted: &[(Field, Style)], matched: &[(Field, Style, u8)]) {
    let Token::Field(letter, count) = token else {
        return;
    };
    let Some((field, found)) = token_field(*letter, *count) else {
        return;
    };
    let Some(&(_, style)) = wanted.iter().find(|(want, _)| *want == field) else {
        return;
    };
    if is_text(field, style) != is_text(field, found) || *letter == b'r' {
        return;
    }
    if let Some(&(_, skeleton_style, _)) = matched.iter().find(|(known, ..)| *known == field)
        && (width(skeleton_style) == width(style)
            || is_text(field, skeleton_style) != is_text(field, style))
    {
        return;
    }
    *count = match field {
        Field::Era | Field::DayPeriod if style == Style::Short => 1,
        _ if is_text(field, style) => style.text_width(),
        _ => width(style),
    };
}

fn strip_am_pm(tokens: Vec<Token>) -> Vec<Token> {
    let mut out: Vec<Token> = Vec::new();
    let mut skip_space = false;
    for token in tokens {
        match token {
            Token::Field(b'a', _) => {
                if let Some(Token::Text(text)) = out.last_mut() {
                    let trimmed = text.trim_end().to_string();
                    if trimmed.is_empty() {
                        out.pop();
                    } else {
                        *text = trimmed;
                    }
                }
                skip_space = true;
            }
            Token::Text(text) if skip_space => {
                skip_space = false;
                let rest = if out.is_empty() {
                    text.trim_start().to_string()
                } else {
                    text
                };
                push_text(&mut out, &rest);
            }
            other => {
                skip_space = false;
                out.push(other);
            }
        }
    }
    out
}

pub(crate) fn set_hour_cycle(tokens: Vec<Token>, cycle: Cycle) -> Vec<Token> {
    let tokens: Vec<Token> = tokens
        .into_iter()
        .map(|token| match token {
            Token::Field(b'h' | b'H' | b'K' | b'k', count) => Token::Field(cycle.letter(), count),
            other => other,
        })
        .collect();
    if cycle.twelve_hour() {
        tokens
    } else {
        strip_am_pm(tokens)
    }
}

fn match_part(tables: &[Formats], wanted: &[(Field, Style)], cycle: Cycle) -> Vec<Token> {
    let twelve = cycle.twelve_hour();
    let mut best: Option<(u32, &str, Skeleton)> = None;
    for table in tables {
        for &(key, pattern) in table.iter() {
            let fields = skeleton(key);
            if fields
                .iter()
                .any(|(field, _, _)| !wanted.iter().any(|(want, _)| want == field))
            {
                continue;
            }
            let mut cost = 0;
            for &(field, style) in wanted {
                match fields.iter().find(|(found, _, _)| *found == field) {
                    Some(&(_, found, letter)) => {
                        cost += distance(field, style, found);
                        if field == Field::Hour && (letter == b'h') != twelve {
                            cost += 1024;
                        }
                    }
                    None if matches!(field, Field::Era | Field::Weekday) => cost += 2048,
                    None => cost += 4096,
                }
            }
            if best.as_ref().is_none_or(|(least, ..)| cost < *least) {
                best = Some((cost, pattern, fields));
            }
        }
    }
    let (mut tokens, matched) = best.map_or_else(
        || (Vec::new(), Vec::new()),
        |(_, pattern, fields)| (parse_pattern(pattern), fields),
    );
    for token in &mut tokens {
        adjust(token, wanted, &matched);
    }
    let present = fields_of(&tokens);
    for &(field, style) in wanted {
        if present[field.index()].is_none() {
            if !tokens.is_empty() {
                push_text(&mut tokens, " ");
            }
            tokens.push(field_token(field, style, cycle));
        }
    }
    if let Some(&(_, style)) = wanted.iter().find(|(field, _)| *field == Field::DayPeriod) {
        let period = field_token(Field::DayPeriod, style, cycle);
        for token in &mut tokens {
            if matches!(token, Token::Field(b'a', _)) {
                *token = period.clone();
            }
        }
    }
    set_hour_cycle(tokens, cycle)
}

fn glue(pattern: &str, date: Vec<Token>, time: Vec<Token>) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut rest = pattern;
    while !rest.is_empty() {
        let next = rest.find('{').unwrap_or(rest.len());
        extend(&mut tokens, parse_pattern(&rest[..next]));
        rest = &rest[next..];
        if rest.starts_with("{1}") {
            extend(&mut tokens, date.clone());
            rest = &rest[3..];
        } else if rest.starts_with("{0}") {
            extend(&mut tokens, time.clone());
            rest = &rest[3..];
        } else if !rest.is_empty() {
            push_text(&mut tokens, "{");
            rest = &rest[1..];
        }
    }
    tokens
}

fn calendar_tables(data: &LocaleData, calendar: &str) -> Vec<Formats> {
    let english = data.id.starts_with("en");
    let mut tables: Vec<Formats> = Vec::new();
    match calendar {
        "chinese" | "dangi" if data.id == "zh" => tables.push(data::ZH_CHINESE_FORMATS),
        "chinese" | "dangi" if english => tables.push(data::EN_CHINESE_FORMATS),
        "hebrew" if english => tables.push(data::EN_HEBREW_FORMATS),
        "gregory" | "iso8601" => {}
        _ if english => tables.push(data::EN_GENERIC_FORMATS),
        _ => {}
    }
    tables.push(data.formats);
    tables.push(data::SINGLE_FIELD_FORMATS);
    tables
}

fn calendar_date_styles(data: &LocaleData, calendar: &str) -> [&'static str; 4] {
    let english = data.id.starts_with("en");
    match calendar {
        "chinese" | "dangi" if data.id == "zh" => data::ZH_CHINESE_STYLES,
        "chinese" | "dangi" if english => data::EN_CHINESE_STYLES,
        "hebrew" if english => data::EN_HEBREW_STYLES,
        "gregory" | "iso8601" => data.date_styles,
        _ if english => data::EN_GENERIC_STYLES,
        _ => data.date_styles,
    }
}

fn add_fraction(tokens: &mut Vec<Token>, digits: u8, decimal: &str) {
    let fraction = Token::Field(b'S', digits);
    match tokens
        .iter()
        .position(|token| matches!(token, Token::Field(b's', _)))
    {
        Some(at) => {
            tokens.insert(at + 1, fraction);
            tokens.insert(at + 1, Token::Text(decimal.to_string()));
        }
        None => {
            if !tokens.is_empty() {
                push_text(tokens, " ");
            }
            tokens.push(fraction);
        }
    }
}

fn zone_token(style: Style) -> Token {
    match style {
        Style::Long => Token::Field(b'z', 4),
        Style::ShortOffset => Token::Field(b'O', 1),
        Style::LongOffset => Token::Field(b'O', 4),
        Style::ShortGeneric => Token::Field(b'v', 1),
        Style::LongGeneric => Token::Field(b'v', 4),
        _ => Token::Field(b'z', 1),
    }
}

pub(crate) fn components(
    data: &LocaleData,
    calendar: &str,
    request: &Request,
    cycle: Cycle,
    decimal: &str,
) -> Format {
    let tables = calendar_tables(data, calendar);
    let wanted = |test: fn(Field) -> bool| -> Vec<(Field, Style)> {
        FIELDS
            .iter()
            .filter(|field| test(**field))
            .filter(|field| **field != Field::Era || calendar::has_eras(calendar))
            .filter_map(|field| request[field.index()].map(|style| (*field, style)))
            .collect()
    };
    let date_wanted = wanted(Field::is_date);
    let clock_wanted = wanted(Field::is_clock);
    let date = (!date_wanted.is_empty()).then(|| match_part(&tables, &date_wanted, cycle));
    let mut time = (!clock_wanted.is_empty()).then(|| match_part(&tables, &clock_wanted, cycle));
    if let Some(Style::Digits(digits)) = request[Field::Fraction.index()] {
        add_fraction(time.get_or_insert_with(Vec::new), digits, decimal);
    }
    if let Some(style) = request[Field::ZoneName.index()] {
        let time = time.get_or_insert_with(Vec::new);
        if !time.is_empty() {
            push_text(time, " ");
        }
        time.push(zone_token(style));
    }
    let tokens = match (date, time) {
        (Some(date), Some(time)) => glue(data.glue, date, time),
        (Some(date), None) => date,
        (None, Some(time)) => time,
        (None, None) => Vec::new(),
    };
    Format {
        fields: fields_of(&tokens),
        tokens,
    }
}

pub(crate) fn styles(
    data: &LocaleData,
    calendar: &str,
    date_style: Option<usize>,
    time_style: Option<usize>,
    cycle: Cycle,
) -> Format {
    let date = date_style.map(|level| parse_pattern(calendar_date_styles(data, calendar)[level]));
    let time = time_style.map(|level| {
        let patterns = if cycle.twelve_hour() {
            data.time_styles12
        } else {
            data.time_styles24
        };
        set_hour_cycle(parse_pattern(patterns[level]), cycle)
    });
    let tokens = match (date, time) {
        (Some(date), Some(time)) => glue(data.style_glue[date_style.unwrap_or(0)], date, time),
        (Some(date), None) => date,
        (None, Some(time)) => time,
        (None, None) => Vec::new(),
    };
    Format {
        fields: fields_of(&tokens),
        tokens,
    }
}

const NUMBERING: [(&str, u32); 77] = [
    ("adlm", 0x1E950),
    ("ahom", 0x11730),
    ("arab", 0x0660),
    ("arabext", 0x06F0),
    ("bali", 0x1B50),
    ("beng", 0x09E6),
    ("bhks", 0x11C50),
    ("brah", 0x11066),
    ("cakm", 0x11136),
    ("cham", 0xAA50),
    ("deva", 0x0966),
    ("diak", 0x11950),
    ("fullwide", 0xFF10),
    ("gara", 0x10D40),
    ("gong", 0x11DA0),
    ("gonm", 0x11D50),
    ("gujr", 0x0AE6),
    ("gukh", 0x16130),
    ("guru", 0x0A66),
    ("hmng", 0x16B50),
    ("hmnp", 0x1E140),
    ("java", 0xA9D0),
    ("kali", 0xA900),
    ("kawi", 0x11F50),
    ("khmr", 0x17E0),
    ("knda", 0x0CE6),
    ("krai", 0x16D70),
    ("lana", 0x1A80),
    ("lanatham", 0x1A90),
    ("laoo", 0x0ED0),
    ("latn", 0x0030),
    ("lepc", 0x1C40),
    ("limb", 0x1946),
    ("mathbold", 0x1D7CE),
    ("mathdbl", 0x1D7D8),
    ("mathmono", 0x1D7F6),
    ("mathsanb", 0x1D7EC),
    ("mathsans", 0x1D7E2),
    ("mlym", 0x0D66),
    ("modi", 0x11650),
    ("mong", 0x1810),
    ("mroo", 0x16A60),
    ("mtei", 0xABF0),
    ("mymr", 0x1040),
    ("mymrepka", 0x116DA),
    ("mymrpao", 0x116D0),
    ("mymrshan", 0x1090),
    ("mymrtlng", 0xA9F0),
    ("nagm", 0x1E4F0),
    ("newa", 0x11450),
    ("nkoo", 0x07C0),
    ("olck", 0x1C50),
    ("onao", 0x1E5F1),
    ("orya", 0x0B66),
    ("osma", 0x104A0),
    ("outlined", 0x1CCF0),
    ("rohg", 0x10D30),
    ("saur", 0xA8D0),
    ("segment", 0x1FBF0),
    ("shrd", 0x111D0),
    ("sind", 0x112F0),
    ("sinh", 0x0DE6),
    ("sora", 0x110F0),
    ("sund", 0x1BB0),
    ("sunu", 0x11BF0),
    ("takr", 0x116C0),
    ("talu", 0x19D0),
    ("tamldec", 0x0BE6),
    ("telu", 0x0C66),
    ("thai", 0x0E50),
    ("tibt", 0x0F20),
    ("tirh", 0x114D0),
    ("tnsa", 0x16AC0),
    ("tols", 0x11DE0),
    ("vaii", 0xA620),
    ("wara", 0x118E0),
    ("wcho", 0x1E2F0),
];

const HANIDEC: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];

pub(crate) fn numbering_supported(name: &str) -> bool {
    name == "hanidec" || NUMBERING.iter().any(|(known, _)| *known == name)
}

pub(crate) fn decimal_for(data: &LocaleData, numbering: &str) -> &'static str {
    match numbering {
        "arab" | "arabext" => "\u{066B}",
        _ => data.decimal,
    }
}

fn digit_map(numbering: &str) -> [char; 10] {
    if numbering == "hanidec" {
        return HANIDEC;
    }
    let zero = NUMBERING
        .iter()
        .find(|(known, _)| *known == numbering)
        .map_or(0x30, |(_, zero)| *zero);
    core::array::from_fn(|digit| char::from_u32(zero + digit as u32).unwrap_or('0'))
}

pub(crate) struct Moment {
    pub(crate) wall: Wall,
    pub(crate) date: CalendarDate,
    pub(crate) weekday: u32,
    pub(crate) offset_ms: i64,
    pub(crate) daylight: bool,
}

pub(crate) type Part = (&'static str, String);

pub(crate) struct Renderer<'a> {
    pub(crate) data: &'static LocaleData,
    pub(crate) calendar: &'a str,
    pub(crate) zone: &'a str,
    pub(crate) digits: [char; 10],
}

fn level(letter: u8) -> Option<u8> {
    Some(match letter {
        b'G' => 0,
        b'y' | b'r' | b'U' => 1,
        b'M' | b'L' => 2,
        b'd' | b'E' | b'c' => 3,
        b'a' | b'B' => 4,
        b'h' | b'H' | b'K' | b'k' => 5,
        b'm' => 6,
        b's' => 7,
        b'S' => 8,
        _ => return None,
    })
}

fn shareable(token: Option<&Token>) -> bool {
    match token {
        Some(Token::Text(text)) => text.chars().any(|c| !matches!(c, '/' | ':' | '.' | '-')),
        _ => false,
    }
}

fn width_index(count: u8) -> usize {
    match count {
        5 => 0,
        4 => 2,
        _ => 1,
    }
}

impl Renderer<'_> {
    pub(crate) fn new<'a>(
        data: &'static LocaleData,
        calendar: &'a str,
        zone: &'a str,
        numbering: &str,
    ) -> Renderer<'a> {
        Renderer {
            data,
            calendar,
            zone,
            digits: digit_map(numbering),
        }
    }

    fn english(&self) -> bool {
        self.data.id.starts_with("en")
    }

    fn number(&self, value: i64, min_digits: usize) -> String {
        let text = format!("{:0min_digits$}", value.unsigned_abs());
        let mut out = String::new();
        if value < 0 {
            out.push('-');
        }
        out.extend(
            text.bytes()
                .map(|digit| self.digits[usize::from(digit - b'0')]),
        );
        out
    }

    fn two_digits(&self, value: i64) -> String {
        self.number(value.rem_euclid(100), 2)
    }

    fn sized(&self, value: i64, count: u8) -> String {
        if count >= 2 {
            self.number(value, 2)
        } else {
            self.number(value, 1)
        }
    }

    fn month_name(&self, date: &CalendarDate, count: u8, standalone: bool) -> String {
        let width = width_index(count);
        let index = date.month.saturating_sub(1) as usize;
        let long = |list: &str| pick(list, index).to_string();
        let first = |name: String| name.chars().next().map(String::from).unwrap_or_default();
        let name = match self.calendar {
            "hebrew" => {
                let slot = match (date.month, date.leap_month) {
                    (5, true) => 5,
                    (6, _) if date.months_in_year == 13 => 13,
                    (month, _) if month <= 5 => month as usize - 1,
                    (month, _) => month as usize,
                };
                pick(data::HEBREW_MONTHS, slot).to_string()
            }
            "islamic-civil" | "islamic-tbla" | "islamic-umalqura" => {
                pick(data::ISLAMIC_MONTHS[usize::from(width == 2)], index).to_string()
            }
            "persian" => long(data::PERSIAN_MONTHS),
            "indian" => long(data::INDIAN_MONTHS),
            "coptic" => long(data::COPTIC_MONTHS),
            "ethiopic" | "ethioaa" => long(data::ETHIOPIC_MONTHS),
            "chinese" | "dangi" if self.data.id == "zh" => {
                let name = long(data::ZH_CHINESE_MONTHS);
                return if date.leap_month {
                    format!("闰{name}")
                } else {
                    name
                };
            }
            "chinese" | "dangi" => {
                let name = pick(data::CHINESE_MONTHS[usize::from(width == 2)], index).to_string();
                return if date.leap_month {
                    format!("{name}bis")
                } else {
                    name
                };
            }
            _ => {
                let lists = match (standalone, self.data.standalone_months) {
                    (true, Some(lists)) => lists,
                    _ => self.data.months,
                };
                return pick(lists[width], index).to_string();
            }
        };
        if width == 0 { first(name) } else { name }
    }

    fn month(&self, date: &CalendarDate, letter: u8, count: u8) -> String {
        if count >= 3 || self.calendar == "hebrew" {
            return self.month_name(date, count.max(3), letter == b'L');
        }
        match self.calendar {
            "chinese" | "dangi" => {
                let number = self.sized(i64::from(date.month), count);
                if date.leap_month {
                    format!("{number}bis")
                } else {
                    number
                }
            }
            _ => self.sized(i64::from(date.ordinal), count),
        }
    }

    fn era(&self, date: &CalendarDate, count: u8) -> String {
        let width = width_index(count);
        let code = date.era.as_deref().unwrap_or("ce");
        let gregorian_names = matches!(self.calendar, "gregory" | "iso8601")
            || (self.calendar == "japanese" && matches!(code, "ce" | "bce"));
        if gregorian_names {
            return pick(self.data.eras[width], usize::from(code != "bce")).to_string();
        }
        let key = if self.calendar == "hebrew" && code == "am" {
            "am-hebrew"
        } else {
            code
        };
        data::ERA_NAMES
            .iter()
            .find(|(known, ..)| *known == key)
            .map_or_else(
                || code.to_ascii_uppercase(),
                |(_, narrow, short, long)| [*narrow, *short, *long][width].to_string(),
            )
    }

    fn year_name(&self, year: i64) -> String {
        let cycle = (year - 4).rem_euclid(60);
        let chinese_script = matches!(self.data.id, "zh" | "ja");
        let script = usize::from(chinese_script);
        let stem = pick(data::STEMS[script], (cycle % 10) as usize);
        let branch = pick(data::BRANCHES[script], (cycle % 12) as usize);
        if chinese_script {
            format!("{stem}{branch}")
        } else {
            format!("{stem}-{branch}")
        }
    }

    fn day_period(&self, moment: &Moment, count: u8) -> String {
        let hour = moment.wall.hour;
        let Some(periods) = self.data.periods else {
            return pick(self.data.am_pm, usize::from(hour >= 12)).to_string();
        };
        let index = if hour == 12 && moment.wall.minute == 0 && moment.wall.second == 0 {
            0
        } else {
            match hour {
                6..=11 => 1,
                12..=17 => 2,
                18..=20 => 3,
                _ => 4,
            }
        };
        pick(periods[width_index(count)], index).to_string()
    }

    fn zone_name(&self, moment: &Moment, letter: u8, count: u8) -> String {
        let style = match (letter, count >= 4) {
            (b'O', false) => NameStyle::ShortOffset,
            (b'O', true) => NameStyle::LongOffset,
            (b'v', false) => NameStyle::ShortGeneric,
            (b'v', true) => NameStyle::LongGeneric,
            (_, true) => NameStyle::Long,
            (_, false) => NameStyle::Short,
        };
        zone::display(
            self.zone,
            moment.offset_ms,
            moment.daylight,
            style,
            self.english(),
        )
    }

    fn hour(&self, moment: &Moment, letter: u8, count: u8) -> String {
        let hour = i64::from(moment.wall.hour);
        let value = match letter {
            b'h' if hour % 12 == 0 => 12,
            b'h' | b'K' => hour % 12,
            b'k' if hour == 0 => 24,
            _ => hour,
        };
        self.sized(value, count)
    }

    pub(crate) fn token(&self, token: &Token, moment: &Moment) -> Part {
        let (letter, count) = match token {
            Token::Text(text) => return ("literal", text.clone()),
            Token::Field(letter, count) => (*letter, *count),
        };
        let date = &moment.date;
        match letter {
            b'G' => ("era", self.era(date, count)),
            b'y' => {
                let year = date.era_year.unwrap_or(date.year);
                let value = if count == 2 {
                    self.two_digits(year)
                } else {
                    self.number(year, 1)
                };
                ("year", value)
            }
            b'r' => ("relatedYear", self.number(date.year, 1)),
            b'U' => ("yearName", self.year_name(date.year)),
            b'M' | b'L' => ("month", self.month(date, letter, count)),
            b'd' => ("day", self.sized(i64::from(date.day), count)),
            b'E' | b'c' => {
                let lists = match (letter, self.data.standalone_weekdays) {
                    (b'c', Some(standalone)) => standalone,
                    _ => self.data.weekdays,
                };
                let list = lists[width_index(count)];
                ("weekday", pick(list, moment.weekday as usize).to_string())
            }
            b'a' => (
                "dayPeriod",
                pick(self.data.am_pm, usize::from(moment.wall.hour >= 12)).to_string(),
            ),
            b'B' => ("dayPeriod", self.day_period(moment, count)),
            b'h' | b'H' | b'K' | b'k' => ("hour", self.hour(moment, letter, count)),
            b'm' => ("minute", self.sized(i64::from(moment.wall.minute), count)),
            b's' => ("second", self.sized(i64::from(moment.wall.second), count)),
            b'S' => {
                let digits = u32::from(count.clamp(1, 3));
                let value = moment.wall.millisecond / 10u32.pow(3 - digits);
                (
                    "fractionalSecond",
                    self.number(i64::from(value), digits as usize),
                )
            }
            b'z' | b'O' | b'v' | b'V' => ("timeZoneName", self.zone_name(moment, letter, count)),
            _ => ("literal", String::new()),
        }
    }

    pub(crate) fn parts(&self, tokens: &[Token], moment: &Moment) -> Vec<Part> {
        tokens
            .iter()
            .map(|token| self.token(token, moment))
            .filter(|(_, value)| !value.is_empty())
            .collect()
    }

    pub(crate) fn range(
        &self,
        tokens: &[Token],
        start: &Moment,
        end: &Moment,
    ) -> Vec<(&'static str, String, &'static str)> {
        let first: Vec<Part> = tokens.iter().map(|t| self.token(t, start)).collect();
        let second: Vec<Part> = tokens.iter().map(|t| self.token(t, end)).collect();
        let levels: Vec<Option<u8>> = tokens
            .iter()
            .map(|token| match token {
                Token::Field(letter, _) => level(*letter),
                Token::Text(_) => None,
            })
            .collect();
        let greatest = (0..tokens.len())
            .filter(|&i| levels[i].is_some() && first[i] != second[i])
            .filter_map(|i| levels[i])
            .min();
        let tagged = |parts: &[Part], range: core::ops::Range<usize>, source: &'static str| {
            parts[range]
                .iter()
                .filter(|(_, value)| !value.is_empty())
                .map(|(kind, value)| (*kind, value.clone(), source))
                .collect::<Vec<_>>()
        };
        let Some(greatest) = greatest else {
            return tagged(&first, 0..tokens.len(), "shared");
        };
        let has_clock = levels.iter().flatten().any(|level| *level >= 4);
        let whole = greatest <= 3 && has_clock;
        let in_span = |i: usize| whole || levels[i].is_some_and(|level| level >= greatest);
        let mut lo = (0..tokens.len()).find(|&i| in_span(i)).unwrap_or(0);
        let mut hi = (0..tokens.len())
            .rfind(|&i| in_span(i))
            .map_or(tokens.len(), |i| i + 1);
        if whole || (lo > 0 && !shareable(tokens.get(lo - 1))) {
            lo = 0;
        }
        if whole || (hi < tokens.len() && !shareable(tokens.get(hi))) {
            hi = tokens.len();
        }
        let mut out = tagged(&first, 0..lo, "shared");
        out.extend(tagged(&first, lo..hi, "startRange"));
        out.push(("literal", self.data.range.to_string(), "shared"));
        out.extend(tagged(&second, lo..hi, "endRange"));
        out.extend(tagged(&first, hi..tokens.len(), "shared"));
        out
    }
}
