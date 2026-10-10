//! Southstar — registered custom property <syntax> grammar: parsing a definition, matching values and computing them.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use core::f64::consts::PI;
use southstar_css_syntax::{self as css, Component, Kind};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SynKind {
    Length,
    Number,
    Percentage,
    LengthPercentage,
    Color,
    Image,
    Url,
    Integer,
    Angle,
    Time,
    Resolution,
    TransformFunction,
    TransformList,
    CustomIdent,
    String,
    Ident,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mult {
    None,
    Space,
    Comma,
}

struct SynComponent {
    kind: SynKind,
    mult: Mult,
    ident: Option<Vec<u8>>,
}

pub struct SyntaxDef {
    universal: bool,
    components: Vec<SynComponent>,
}

#[derive(Default)]
pub struct Context<'a> {
    pub font_size: f64,
    pub root_font_size: f64,
    pub line_height: f64,
    pub root_line_height: f64,
    pub ex_px: f64,
    pub ch_px: f64,
    pub cap_px: f64,
    pub ic_px: f64,
    pub root_ex_px: f64,
    pub root_ch_px: f64,
    pub root_cap_px: f64,
    pub root_ic_px: f64,
    pub viewport_w: f64,
    pub viewport_h: f64,
    pub container_w: f64,
    pub container_h: f64,
    pub current_color: Option<&'a [u8]>,
}

const LEN_PX: usize = 0;
const LEN_PC: usize = 6;
const LEN_EM: usize = 7;
const LEN_EX: usize = 8;
const LEN_CH: usize = 9;
const LEN_IC: usize = 10;
const LEN_CAP: usize = 11;
const LEN_LH: usize = 12;
const LEN_REM: usize = 13;
const LEN_REX: usize = 14;
const LEN_RCH: usize = 15;
const LEN_RIC: usize = 16;
const LEN_RCAP: usize = 17;
const LEN_RLH: usize = 18;
const LEN_VW: usize = 19;
const LEN_VH: usize = 20;
const LEN_VI: usize = 21;
const LEN_VB: usize = 22;
const LEN_VMIN: usize = 23;
const LEN_VMAX: usize = 24;
const LEN_CQW: usize = 25;
const LEN_CQH: usize = 26;
const LEN_CQI: usize = 27;
const LEN_CQB: usize = 28;
const LEN_CQMIN: usize = 29;
const LEN_CQMAX: usize = 30;
const LEN_COUNT: usize = 31;

const LENGTH_UNITS: [(&str, usize); 49] = [
    ("px", LEN_PX),
    ("cm", 1),
    ("mm", 2),
    ("q", 3),
    ("in", 4),
    ("pt", 5),
    ("pc", LEN_PC),
    ("em", LEN_EM),
    ("ex", LEN_EX),
    ("ch", LEN_CH),
    ("ic", LEN_IC),
    ("cap", LEN_CAP),
    ("lh", LEN_LH),
    ("rem", LEN_REM),
    ("rex", LEN_REX),
    ("rch", LEN_RCH),
    ("ric", LEN_RIC),
    ("rcap", LEN_RCAP),
    ("rlh", LEN_RLH),
    ("vw", LEN_VW),
    ("vh", LEN_VH),
    ("vi", LEN_VI),
    ("vb", LEN_VB),
    ("vmin", LEN_VMIN),
    ("vmax", LEN_VMAX),
    ("svw", LEN_VW),
    ("svh", LEN_VH),
    ("svi", LEN_VI),
    ("svb", LEN_VB),
    ("svmin", LEN_VMIN),
    ("svmax", LEN_VMAX),
    ("lvw", LEN_VW),
    ("lvh", LEN_VH),
    ("lvi", LEN_VI),
    ("lvb", LEN_VB),
    ("lvmin", LEN_VMIN),
    ("lvmax", LEN_VMAX),
    ("dvw", LEN_VW),
    ("dvh", LEN_VH),
    ("dvi", LEN_VI),
    ("dvb", LEN_VB),
    ("dvmin", LEN_VMIN),
    ("dvmax", LEN_VMAX),
    ("cqw", LEN_CQW),
    ("cqh", LEN_CQH),
    ("cqi", LEN_CQI),
    ("cqb", LEN_CQB),
    ("cqmin", LEN_CQMIN),
    ("cqmax", LEN_CQMAX),
];

const ABSOLUTE_UNIT_PX: [f64; 7] = [
    1.0,
    96.0 / 2.54,
    96.0 / 25.4,
    96.0 / 101.6,
    96.0,
    96.0 / 72.0,
    16.0,
];

fn len_unit_font_relative(unit: usize) -> bool {
    (LEN_EM..=LEN_RLH).contains(&unit)
}

fn caseless(a: &[u8], b: &str) -> bool {
    a.eq_ignore_ascii_case(b.as_bytes())
}

fn c_min(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

fn c_max(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

fn ascii_space(c: u8) -> bool {
    c.is_ascii_whitespace()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MathKind {
    Invalid,
    Number,
    Length,
    Percent,
    Angle,
    Time,
    Resolution,
    Flex,
}

#[derive(Clone, Copy)]
struct MathValue {
    kind: MathKind,
    number: f64,
    percent: f64,
    has_percent: bool,
    len: [f64; LEN_COUNT],
}

impl MathValue {
    fn zero() -> MathValue {
        MathValue {
            kind: MathKind::Invalid,
            number: 0.0,
            percent: 0.0,
            has_percent: false,
            len: [0.0; LEN_COUNT],
        }
    }

    fn number(number: f64) -> MathValue {
        MathValue {
            kind: MathKind::Number,
            number,
            ..MathValue::zero()
        }
    }

    fn is_zero(&self) -> bool {
        if self.has_percent && self.percent != 0.0 {
            return false;
        }
        self.len.iter().all(|&len| len == 0.0) && self.number == 0.0
    }

    fn has_length_terms(&self) -> bool {
        self.len.iter().any(|&len| len != 0.0)
    }

    fn font_relative(&self) -> bool {
        self.len
            .iter()
            .enumerate()
            .any(|(unit, &len)| len != 0.0 && len_unit_font_relative(unit))
    }

    fn is_plain_number(&self) -> bool {
        self.kind == MathKind::Number && !self.has_length_terms() && !self.has_percent
    }

    fn scale(&mut self, f: f64) {
        self.number *= f;
        self.percent *= f;
        for len in &mut self.len {
            *len *= f;
        }
    }

    fn add(&mut self, b: &MathValue, sign: f64) -> bool {
        if self.kind == MathKind::Invalid || b.kind == MathKind::Invalid {
            return false;
        }
        let (ka, kb) = (self.kind, b.kind);
        if ka != kb {
            let (a_zero, b_zero) = (self.is_zero(), b.is_zero());
            if ka == MathKind::Length && kb == MathKind::Percent {
            } else if ka == MathKind::Percent && kb == MathKind::Length {
                self.kind = MathKind::Length;
            } else if a_zero && ka == MathKind::Number {
                self.kind = kb;
            } else if b_zero && kb == MathKind::Number {
            } else {
                return false;
            }
        }
        self.number += sign * b.number;
        self.percent += sign * b.percent;
        self.has_percent = self.has_percent || b.has_percent;
        for (a, b) in self.len.iter_mut().zip(b.len.iter()) {
            *a += sign * b;
        }
        true
    }
}

struct Input<'a> {
    text: &'a [u8],
}

impl<'a> Input<'a> {
    fn value(&self, c: &Component) -> Option<&'a [u8]> {
        c.value.clone().map(|range| until_nul(&self.text[range]))
    }

    fn text_of(&self, c: &Component) -> &'a [u8] {
        until_nul(&self.text[c.start..c.end])
    }
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    bytes
        .iter()
        .position(|&byte| byte == 0)
        .map_or(bytes, |end| &bytes[..end])
}

fn is_ws(c: &Component) -> bool {
    c.kind == Kind::Whitespace
}

const MATH_FUNCTIONS: [&str; 22] = [
    "calc",
    "min",
    "max",
    "clamp",
    "round",
    "mod",
    "rem",
    "abs",
    "sign",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "atan2",
    "pow",
    "sqrt",
    "hypot",
    "log",
    "exp",
    "calc-size",
];

fn math_function_name(name: &[u8]) -> bool {
    MATH_FUNCTIONS.iter().any(|f| caseless(name, f))
}

fn eval_dimension(input: &Input<'_>, c: &Component) -> Option<MathValue> {
    match c.kind {
        Kind::Number => return Some(MathValue::number(c.number)),
        Kind::Percentage => {
            return Some(MathValue {
                kind: MathKind::Percent,
                percent: c.number,
                has_percent: true,
                ..MathValue::zero()
            });
        }
        Kind::Dimension => {}
        _ => return None,
    }
    let unit = input.value(c).unwrap_or_default();
    if let Some(&(_, index)) = LENGTH_UNITS.iter().find(|(name, _)| caseless(unit, name)) {
        let mut out = MathValue {
            kind: MathKind::Length,
            ..MathValue::zero()
        };
        out.len[index] = c.number;
        return Some(out);
    }
    let with = |kind: MathKind, number: f64| {
        Some(MathValue {
            kind,
            number,
            ..MathValue::zero()
        })
    };
    if ["deg", "grad", "rad", "turn"]
        .iter()
        .any(|u| caseless(unit, u))
    {
        let scale = if caseless(unit, "deg") {
            1.0
        } else if caseless(unit, "grad") {
            0.9
        } else if caseless(unit, "rad") {
            180.0 / PI
        } else {
            360.0
        };
        return with(MathKind::Angle, c.number * scale);
    }
    if caseless(unit, "s") || caseless(unit, "ms") {
        let number = if caseless(unit, "s") {
            c.number
        } else {
            c.number / 1000.0
        };
        return with(MathKind::Time, number);
    }
    if ["dppx", "x", "dpi", "dpcm"]
        .iter()
        .any(|u| caseless(unit, u))
    {
        let scale = if caseless(unit, "dpi") {
            1.0 / 96.0
        } else if caseless(unit, "dpcm") {
            2.54 / 96.0
        } else {
            1.0
        };
        return with(MathKind::Resolution, c.number * scale);
    }
    if caseless(unit, "fr") {
        return with(MathKind::Flex, c.number);
    }
    None
}

fn split_args(children: Option<&Vec<Component>>) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let Some(children) = children else {
        return ranges;
    };
    let mut start = 0;
    for (i, c) in children.iter().enumerate() {
        if c.kind != Kind::Comma {
            continue;
        }
        ranges.push((start, i));
        start = i + 1;
    }
    ranges.push((start, children.len()));
    ranges
}

fn eval_function(input: &Input<'_>, func: &Component, depth: i32) -> Option<MathValue> {
    if depth > 32 {
        return None;
    }
    let name = input.value(func).unwrap_or_default();
    let children = func.children.as_ref()?;
    let ranges = split_args(Some(children));
    let nargs = ranges.len();
    let eval = |(lo, hi): (usize, usize)| {
        let mut pos = lo;
        eval_sum(input, children, &mut pos, hi, depth + 1)
    };
    let named = |f: &str| caseless(name, f);

    if named("calc") && nargs == 1 {
        return eval(ranges[0]);
    }
    if (named("min") || named("max") || named("clamp") || named("hypot")) && nargs >= 1 {
        let mut acc: Option<MathValue> = None;
        for &range in &ranges {
            let v = eval(range)?;
            let Some(acc) = acc.as_mut() else {
                acc = Some(v);
                continue;
            };
            let mut probe = *acc;
            if !probe.add(&v, 1.0) {
                return None;
            }
            acc.kind = probe.kind;
            if v.has_length_terms() || v.has_percent || acc.has_length_terms() || acc.has_percent {
                for (a, &b) in acc.len.iter_mut().zip(v.len.iter()) {
                    if b != 0.0 {
                        *a = b;
                    }
                }
                if v.has_percent {
                    acc.has_percent = true;
                }
            } else if named("min") {
                acc.number = c_min(acc.number, v.number);
            } else if named("max") {
                acc.number = c_max(acc.number, v.number);
            } else if named("hypot") {
                acc.number = (acc.number * acc.number + v.number * v.number).sqrt();
            } else {
                acc.number = v.number;
            }
        }
        return acc;
    }
    if (named("abs") || named("sign")) && nargs == 1 {
        let mut out = eval(ranges[0])?;
        if named("abs") {
            if out.number < 0.0 {
                out.scale(-1.0);
            }
            return Some(out);
        }
        let n = out.number;
        let sign = if n > 0.0 {
            1.0
        } else if n < 0.0 {
            -1.0
        } else {
            0.0
        };
        return Some(MathValue::number(sign));
    }
    if (named("round") || named("mod") || named("rem")) && nargs >= 2 {
        let a = eval(ranges[nargs - 2])?;
        let b = eval(ranges[nargs - 1])?;
        let mut probe = a;
        return probe.add(&b, 1.0).then_some(a);
    }
    let numeric_args = |convert_angles: bool| -> Option<[f64; 2]> {
        let mut args = [0.0; 2];
        for (i, &range) in ranges.iter().take(2).enumerate() {
            let v = eval(range)?;
            if v.has_length_terms() || v.has_percent {
                return None;
            }
            args[i] = if convert_angles && v.kind == MathKind::Angle {
                v.number * PI / 180.0
            } else {
                v.number
            };
        }
        Some(args)
    };
    if ["sin", "cos", "tan", "sqrt", "exp", "log", "pow"]
        .iter()
        .any(|f| named(f))
        && nargs >= 1
    {
        let args = numeric_args(true)?;
        let number = if named("sin") {
            args[0].sin()
        } else if named("cos") {
            args[0].cos()
        } else if named("tan") {
            args[0].tan()
        } else if named("sqrt") {
            args[0].sqrt()
        } else if named("exp") {
            args[0].exp()
        } else if named("log") {
            if nargs > 1 {
                args[0].ln() / args[1].ln()
            } else {
                args[0].ln()
            }
        } else {
            args[0].powf(args[1])
        };
        return Some(MathValue::number(number));
    }
    if ["asin", "acos", "atan", "atan2"].iter().any(|f| named(f)) && nargs >= 1 {
        let args = numeric_args(false)?;
        let radians = if named("asin") {
            args[0].asin()
        } else if named("acos") {
            args[0].acos()
        } else if named("atan") {
            args[0].atan()
        } else {
            args[0].atan2(args[1])
        };
        return Some(MathValue {
            kind: MathKind::Angle,
            number: radians * 180.0 / PI,
            ..MathValue::zero()
        });
    }
    None
}

fn skip_ws(items: &[Component], pos: &mut usize, end: usize) -> bool {
    let mut saw = false;
    while *pos < end && is_ws(&items[*pos]) {
        *pos += 1;
        saw = true;
    }
    saw
}

fn eval_term(
    input: &Input<'_>,
    items: &[Component],
    pos: &mut usize,
    end: usize,
    depth: i32,
) -> Option<MathValue> {
    skip_ws(items, pos, end);
    if *pos >= end {
        return None;
    }
    let c = &items[*pos];
    if c.kind == Kind::Block && c.delimiter == b')' {
        let result = c.children.as_ref().and_then(|children| {
            let mut inner = 0;
            eval_sum(input, children, &mut inner, children.len(), depth + 1)
        });
        *pos += 1;
        return result;
    }
    if c.kind == Kind::Function {
        if !math_function_name(input.value(c).unwrap_or_default()) {
            return None;
        }
        let result = eval_function(input, c, depth);
        *pos += 1;
        return result;
    }
    let result = eval_dimension(input, c)?;
    *pos += 1;
    Some(result)
}

fn eval_product(
    input: &Input<'_>,
    items: &[Component],
    pos: &mut usize,
    end: usize,
    depth: i32,
) -> Option<MathValue> {
    let mut out = eval_term(input, items, pos, end, depth)?;
    loop {
        let save = *pos;
        skip_ws(items, pos, end);
        if *pos >= end {
            *pos = save;
            return Some(out);
        }
        let op = &items[*pos];
        if op.kind != Kind::Delim || (op.delimiter != b'*' && op.delimiter != b'/') {
            *pos = save;
            return Some(out);
        }
        let sign = op.delimiter;
        *pos += 1;
        let rhs = eval_term(input, items, pos, end, depth)?;
        if sign == b'*' {
            if out.is_plain_number() {
                let f = out.number;
                out = rhs;
                out.scale(f);
            } else if rhs.is_plain_number() {
                out.scale(rhs.number);
            } else {
                return None;
            }
        } else {
            if !rhs.is_plain_number() || rhs.number == 0.0 {
                return None;
            }
            out.scale(1.0 / rhs.number);
        }
    }
}

fn eval_sum(
    input: &Input<'_>,
    items: &[Component],
    pos: &mut usize,
    end: usize,
    depth: i32,
) -> Option<MathValue> {
    if depth > 32 {
        return None;
    }
    let mut out = eval_product(input, items, pos, end, depth)?;
    loop {
        let saw_ws = skip_ws(items, pos, end);
        if *pos >= end {
            return Some(out);
        }
        let op = &items[*pos];
        if op.kind != Kind::Delim || (op.delimiter != b'+' && op.delimiter != b'-') || !saw_ws {
            return None;
        }
        let sign = if op.delimiter == b'+' { 1.0 } else { -1.0 };
        *pos += 1;
        if *pos >= end || !is_ws(&items[*pos]) {
            return None;
        }
        let rhs = eval_product(input, items, pos, end, depth)?;
        if !out.add(&rhs, sign) {
            return None;
        }
    }
}

fn eval_component(input: &Input<'_>, c: &Component) -> Option<MathValue> {
    if c.kind == Kind::Function {
        if !math_function_name(input.value(c).unwrap_or_default()) {
            return None;
        }
        return eval_function(input, c, 0);
    }
    eval_dimension(input, c)
}

fn unescape_cp(cp: u32) -> char {
    if cp == 0 || cp > 0x10FFFF || (0xD800..=0xDFFF).contains(&cp) {
        return '\u{FFFD}';
    }
    char::from_u32(cp).unwrap_or('\u{FFFD}')
}

fn byte_at(text: &[u8], i: usize) -> u8 {
    text.get(i).copied().unwrap_or(0)
}

fn append_unescaped(out: &mut Vec<u8>, text: &[u8], mut p: usize) -> usize {
    if byte_at(text, p) == b'\\' && byte_at(text, p + 1) != 0 {
        p += 1;
        if byte_at(text, p).is_ascii_hexdigit() {
            let mut cp: u32 = 0;
            let mut n = 0;
            while n < 6 && byte_at(text, p).is_ascii_hexdigit() {
                let digit = (byte_at(text, p) as char).to_digit(16).unwrap_or(0);
                cp = cp.wrapping_mul(16).wrapping_add(digit);
                p += 1;
                n += 1;
            }
            let c = byte_at(text, p);
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c) {
                p += 1;
                if c == b'\r' && byte_at(text, p) == b'\n' {
                    p += 1;
                }
            }
            let mut buf = [0u8; 4];
            out.extend_from_slice(unescape_cp(cp).encode_utf8(&mut buf).as_bytes());
        } else {
            out.push(byte_at(text, p));
            p += 1;
        }
    } else {
        out.push(byte_at(text, p));
        p += 1;
    }
    p
}

fn ident_unescape(text: &[u8], start: usize, end: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut p = start;
    while p < end {
        if text[p] == b'\\' {
            let q = append_unescaped(&mut out, text, p);
            if q == p {
                out.push(text[p]);
                p += 1;
            } else {
                p = q;
            }
            continue;
        }
        out.push(text[p]);
        p += 1;
    }
    out
}

fn css_wide_keyword(ident: &[u8]) -> bool {
    ["initial", "inherit", "unset", "revert", "revert-layer"]
        .iter()
        .any(|kw| caseless(ident, kw))
}

fn reserved_ident(ident: &[u8]) -> bool {
    css_wide_keyword(ident) || caseless(ident, "default")
}

const SYNTAX_TYPES: [(&str, SynKind); 15] = [
    ("length", SynKind::Length),
    ("number", SynKind::Number),
    ("percentage", SynKind::Percentage),
    ("length-percentage", SynKind::LengthPercentage),
    ("color", SynKind::Color),
    ("image", SynKind::Image),
    ("url", SynKind::Url),
    ("integer", SynKind::Integer),
    ("angle", SynKind::Angle),
    ("time", SynKind::Time),
    ("resolution", SynKind::Resolution),
    ("transform-function", SynKind::TransformFunction),
    ("transform-list", SynKind::TransformList),
    ("custom-ident", SynKind::CustomIdent),
    ("string", SynKind::String),
];

fn ident_valid(text: &[u8]) -> bool {
    if text.is_empty() {
        return false;
    }
    let (items, valid) = css::parse(text);
    valid && items.len() == 1 && items[0].kind == Kind::Ident
}

fn parse_component(full: &[u8], mut start: usize, mut end: usize) -> Option<SynComponent> {
    while end > start && ascii_space(full[end - 1]) {
        end -= 1;
    }
    while end > start && ascii_space(full[start]) {
        start += 1;
    }
    if start == end {
        return None;
    }
    let mut mult = Mult::None;
    if full[end - 1] == b'+' {
        mult = Mult::Space;
        end -= 1;
    } else if full[end - 1] == b'#' {
        mult = Mult::Comma;
        end -= 1;
    }
    if start == end {
        return None;
    }
    let text = &full[start..end];
    if text[0] == b'<' {
        if text[text.len() - 1] != b'>' {
            return None;
        }
        let inner = text.get(1..text.len().wrapping_sub(1)).unwrap_or_default();
        let inner_len = text.len().wrapping_sub(2);
        let (_, kind) = SYNTAX_TYPES
            .iter()
            .find(|(name, _)| name.len() == inner_len && name.as_bytes() == inner)?;
        if *kind == SynKind::TransformList && mult != Mult::None {
            return None;
        }
        return Some(SynComponent {
            kind: *kind,
            mult,
            ident: None,
        });
    }
    if !ident_valid(text) {
        return None;
    }
    let ident = ident_unescape(full, start, end);
    if reserved_ident(&ident) {
        return None;
    }
    Some(SynComponent {
        kind: SynKind::Ident,
        mult,
        ident: Some(ident),
    })
}

pub fn parse(text: &[u8]) -> Option<SyntaxDef> {
    let mut p = 0;
    let mut end = text.len();
    while p < end && ascii_space(text[p]) {
        p += 1;
    }
    while end > p && ascii_space(text[end - 1]) {
        end -= 1;
    }
    if p == end {
        return None;
    }
    if end - p == 1 && text[p] == b'*' {
        return Some(SyntaxDef {
            universal: true,
            components: Vec::new(),
        });
    }
    let mut components = Vec::new();
    let mut seg = p;
    let mut q = p;
    while q <= end {
        if q < end && text[q] == b'\\' {
            if q + 1 < end {
                q += 1;
            }
            q += 1;
            continue;
        }
        if q < end && text[q] != b'|' {
            q += 1;
            continue;
        }
        components.push(parse_component(text, seg, q)?);
        seg = q + 1;
        q += 1;
    }
    if components.is_empty() {
        return None;
    }
    Some(SyntaxDef {
        universal: false,
        components,
    })
}

impl SyntaxDef {
    pub fn universal(&self) -> bool {
        self.universal
    }
}

#[derive(Clone, Copy)]
struct Chunk<'a> {
    items: &'a [Component],
    lo: usize,
    hi: usize,
}

impl Chunk<'_> {
    fn trim(&mut self) {
        while self.lo < self.hi && is_ws(&self.items[self.lo]) {
            self.lo += 1;
        }
        while self.hi > self.lo && is_ws(&self.items[self.hi - 1]) {
            self.hi -= 1;
        }
    }
}

fn color_function(name: &[u8]) -> bool {
    [
        "rgb",
        "rgba",
        "hsl",
        "hsla",
        "hwb",
        "lab",
        "lch",
        "oklab",
        "oklch",
        "color",
        "color-mix",
        "light-dark",
        "contrast-color",
        "device-cmyk",
    ]
    .iter()
    .any(|f| caseless(name, f))
}

fn image_function(name: &[u8]) -> bool {
    [
        "url",
        "src",
        "linear-gradient",
        "radial-gradient",
        "conic-gradient",
        "repeating-linear-gradient",
        "repeating-radial-gradient",
        "repeating-conic-gradient",
        "image",
        "image-set",
        "-webkit-image-set",
        "cross-fade",
        "paint",
        "light-dark",
        "-webkit-linear-gradient",
        "-webkit-radial-gradient",
        "-webkit-gradient",
    ]
    .iter()
    .any(|f| caseless(name, f))
}

fn transform_function(name: &[u8], argc: usize) -> bool {
    const FUNCTIONS: [(&str, usize, usize); 21] = [
        ("translate", 1, 2),
        ("translatex", 1, 1),
        ("translatey", 1, 1),
        ("translatez", 1, 1),
        ("translate3d", 3, 3),
        ("scale", 1, 2),
        ("scalex", 1, 1),
        ("scaley", 1, 1),
        ("scalez", 1, 1),
        ("scale3d", 3, 3),
        ("rotate", 1, 1),
        ("rotatex", 1, 1),
        ("rotatey", 1, 1),
        ("rotatez", 1, 1),
        ("rotate3d", 4, 4),
        ("skew", 1, 2),
        ("skewx", 1, 1),
        ("skewy", 1, 1),
        ("matrix", 6, 6),
        ("matrix3d", 16, 16),
        ("perspective", 1, 1),
    ];
    FUNCTIONS
        .iter()
        .find(|(f, _, _)| caseless(name, f))
        .is_some_and(|&(_, min, max)| argc >= min && argc <= max)
}

fn number_is_integer(input: &Input<'_>, c: &Component) -> bool {
    !input.text[c.start..c.end]
        .iter()
        .any(|&ch| matches!(ch, b'.' | b'e' | b'E'))
}

fn match_numeric(
    input: &Input<'_>,
    kind: SynKind,
    chunk: &Chunk<'_>,
    independent: bool,
) -> Option<MathValue> {
    if chunk.hi - chunk.lo != 1 {
        return None;
    }
    let c = &chunk.items[chunk.lo];
    let v = eval_component(input, c)?;
    let is_math = c.kind == Kind::Function;
    if independent && v.font_relative() {
        return None;
    }
    let ok = match kind {
        SynKind::Number => v.is_plain_number(),
        SynKind::Integer => v.is_plain_number() && (is_math || number_is_integer(input, c)),
        SynKind::Length => {
            (v.kind == MathKind::Number && !is_math && v.number == 0.0)
                || (v.kind == MathKind::Length && !v.has_percent)
        }
        SynKind::Percentage => v.kind == MathKind::Percent && !v.has_length_terms(),
        SynKind::LengthPercentage => {
            (v.kind == MathKind::Number && !is_math && v.number == 0.0)
                || v.kind == MathKind::Length
                || v.kind == MathKind::Percent
        }
        SynKind::Angle => v.kind == MathKind::Angle,
        SynKind::Time => v.kind == MathKind::Time,
        SynKind::Resolution => {
            v.kind == MathKind::Resolution
                && v.number.partial_cmp(&0.0) != Some(core::cmp::Ordering::Less)
        }
        _ => false,
    };
    ok.then_some(v)
}

fn len_unit_px(unit: usize, ctx: Option<&Context<'_>>) -> f64 {
    if unit <= LEN_PC {
        return ABSOLUTE_UNIT_PX[unit];
    }
    let Some(ctx) = ctx else {
        return 0.0;
    };
    match unit {
        LEN_EM => ctx.font_size,
        LEN_EX => ctx.ex_px,
        LEN_CH => ctx.ch_px,
        LEN_IC => ctx.ic_px,
        LEN_CAP => ctx.cap_px,
        LEN_LH => ctx.line_height,
        LEN_REM => ctx.root_font_size,
        LEN_REX => ctx.root_ex_px,
        LEN_RCH => ctx.root_ch_px,
        LEN_RIC => ctx.root_ic_px,
        LEN_RCAP => ctx.root_cap_px,
        LEN_RLH => ctx.root_line_height,
        LEN_VW | LEN_VI => ctx.viewport_w / 100.0,
        LEN_VH | LEN_VB => ctx.viewport_h / 100.0,
        LEN_VMIN => c_min(ctx.viewport_w, ctx.viewport_h) / 100.0,
        LEN_VMAX => c_max(ctx.viewport_w, ctx.viewport_h) / 100.0,
        LEN_CQW | LEN_CQI => ctx.container_w / 100.0,
        LEN_CQH | LEN_CQB => ctx.container_h / 100.0,
        LEN_CQMIN => c_min(ctx.container_w, ctx.container_h) / 100.0,
        LEN_CQMAX => c_max(ctx.container_w, ctx.container_h) / 100.0,
        _ => 0.0,
    }
}

fn length_px(v: &MathValue, ctx: Option<&Context<'_>>) -> f64 {
    let mut px = 0.0;
    for (unit, &len) in v.len.iter().enumerate() {
        if len != 0.0 {
            px += len * len_unit_px(unit, ctx);
        }
    }
    px
}

fn append_number(out: &mut Vec<u8>, mut n: f64) {
    if n.is_nan() {
        out.extend_from_slice(b"NaN");
        return;
    }
    if n.is_infinite() {
        out.extend_from_slice(if n < 0.0 { b"-infinity" } else { b"infinity" });
        return;
    }
    if n == 0.0 {
        n = 0.0;
    }
    let mut text = ffi::format_fixed6(n);
    if let Some(dot) = text.iter().position(|&b| b == b'.') {
        let mut end = text.len();
        while end > dot && text[end - 1] == b'0' {
            end -= 1;
        }
        if end - 1 == dot {
            end -= 1;
        }
        text.truncate(end);
    }
    if text == b"-0" {
        out.push(b'0');
    } else {
        out.extend_from_slice(&text);
    }
}

fn append_dimension(out: &mut Vec<u8>, n: f64, unit: &str) {
    append_number(out, n);
    out.extend_from_slice(unit.as_bytes());
}

fn append_string(out: &mut Vec<u8>, text: &[u8]) {
    out.push(b'"');
    for &b in text {
        if b == b'"' || b == b'\\' {
            out.push(b'\\');
        }
        out.push(b);
    }
    out.push(b'"');
}

fn append_color(out: &mut Vec<u8>, [r, g, b, a]: [u8; 4]) {
    use std::io::Write;
    if a == 255 {
        let _ = write!(out, "rgb({r}, {g}, {b})");
        return;
    }
    let _ = write!(out, "rgba({r}, {g}, {b}, ");
    append_number(
        out,
        f64::from((f64::from(a) * 100.0 / 255.0 + 0.5) as i32) / 100.0,
    );
    out.push(b')');
}

fn append_math(out: &mut Vec<u8>, kind: SynKind, v: &MathValue, ctx: Option<&Context<'_>>) {
    match kind {
        SynKind::Integer => return append_number(out, v.number.round()),
        SynKind::Number => return append_number(out, v.number),
        SynKind::Angle => return append_dimension(out, v.number, "deg"),
        SynKind::Time => return append_dimension(out, v.number, "s"),
        SynKind::Resolution => return append_dimension(out, v.number, "dppx"),
        SynKind::Percentage => return append_dimension(out, v.percent, "%"),
        _ => {}
    }
    let has_len = v.has_length_terms();
    if v.has_percent && has_len {
        let px = length_px(v, ctx);
        out.extend_from_slice(b"calc(");
        append_dimension(out, v.percent, "%");
        out.extend_from_slice(if px < 0.0 { b" - " } else { b" + " });
        append_dimension(out, if px < 0.0 { -px } else { px }, "px");
        out.push(b')');
        return;
    }
    if v.has_percent {
        append_dimension(out, v.percent, "%");
        return;
    }
    append_dimension(out, length_px(v, ctx), "px");
}

struct Emit<'a, 'b> {
    out: &'a mut Vec<u8>,
    ctx: Option<&'a Context<'b>>,
}

fn match_single(
    input: &Input<'_>,
    kind: SynKind,
    ident: Option<&[u8]>,
    mut chunk: Chunk<'_>,
    independent: bool,
    mut emit: Option<&mut Emit<'_, '_>>,
) -> bool {
    chunk.trim();
    if chunk.lo >= chunk.hi {
        return false;
    }
    let count = chunk.hi - chunk.lo;
    let first = &chunk.items[chunk.lo];
    match kind {
        SynKind::Ident | SynKind::CustomIdent => {
            if count != 1 || first.kind != Kind::Ident {
                return false;
            }
            let got = ident_unescape(input.text, first.start, first.end);
            let ok = if kind == SynKind::Ident {
                ident.is_some_and(|ident| until_nul(&got) == ident)
            } else {
                !reserved_ident(&got)
            };
            if ok && let Some(emit) = emit {
                emit.out.extend_from_slice(until_nul(&got));
            }
            return ok;
        }
        SynKind::String => {
            if count != 1 || first.kind != Kind::String {
                return false;
            }
            if let Some(emit) = emit {
                append_string(emit.out, input.value(first).unwrap_or_default());
            }
            return true;
        }
        SynKind::Url | SynKind::Image => {
            if count != 1 || first.kind != Kind::Function {
                return false;
            }
            let Some(name) = input.value(first) else {
                return false;
            };
            let ok = if kind == SynKind::Url {
                caseless(name, "url") || caseless(name, "src")
            } else {
                image_function(name)
            };
            if ok && let Some(emit) = emit {
                emit.out.extend_from_slice(input.text_of(first));
            }
            return ok;
        }
        SynKind::Color => {
            if count != 1 {
                return false;
            }
            if first.kind == Kind::Function && !input.value(first).is_some_and(color_function) {
                return false;
            }
            if !matches!(first.kind, Kind::Function | Kind::Ident | Kind::Hash) {
                return false;
            }
            let text = input.text_of(first);
            let current = caseless(text, "currentcolor");
            let parsed = if current {
                None
            } else {
                ffi::parse_color(text)
            };
            let ok = current || parsed.is_some() || first.kind == Kind::Function;
            if ok && let Some(emit) = emit {
                match (current, emit.ctx.and_then(|ctx| ctx.current_color), parsed) {
                    (true, Some(color), _) => emit.out.extend_from_slice(color),
                    (_, _, Some(rgba)) => append_color(emit.out, rgba),
                    _ => emit.out.extend_from_slice(text),
                }
            }
            return ok;
        }
        SynKind::TransformFunction => {
            if count != 1 || first.kind != Kind::Function {
                return false;
            }
            let children = first.children.as_ref();
            let ranges = split_args(children);
            let mut argc = 0;
            let mut ok = true;
            let mut args = emit.as_ref().map(|_| Vec::new());
            for &(lo, hi) in &ranges {
                let mut arg = Chunk {
                    items: children.map_or(&[][..], Vec::as_slice),
                    lo,
                    hi,
                };
                arg.trim();
                if arg.lo >= arg.hi {
                    if ranges.len() != 1 {
                        ok = false;
                    }
                    break;
                }
                argc += 1;
                if arg.hi - arg.lo != 1 {
                    ok = false;
                    break;
                }
                let Some(v) = eval_component(input, &arg.items[arg.lo]) else {
                    ok = false;
                    break;
                };
                if independent && v.font_relative() {
                    ok = false;
                    break;
                }
                let Some(args) = args.as_mut() else {
                    continue;
                };
                if argc > 1 {
                    args.extend_from_slice(b", ");
                }
                let arg_kind = if v.kind == MathKind::Angle {
                    SynKind::Angle
                } else if v.is_plain_number() {
                    SynKind::Number
                } else {
                    SynKind::LengthPercentage
                };
                append_math(args, arg_kind, &v, emit.as_ref().and_then(|emit| emit.ctx));
            }
            let name = input.value(first);
            ok = ok && argc > 0 && transform_function(name.unwrap_or_default(), argc);
            if ok && let (Some(emit), Some(args)) = (emit.as_mut(), args) {
                emit.out.extend_from_slice(name.unwrap_or(b"(null)"));
                emit.out.push(b'(');
                emit.out.extend_from_slice(&args);
                emit.out.push(b')');
            }
            return ok;
        }
        _ => {}
    }
    let Some(v) = match_numeric(input, kind, &chunk, independent) else {
        return false;
    };
    if let Some(emit) = emit.as_mut() {
        append_math(emit.out, kind, &v, emit.ctx);
    }
    true
}

fn match_space_list(
    input: &Input<'_>,
    kind: SynKind,
    ident: Option<&[u8]>,
    mut chunk: Chunk<'_>,
    independent: bool,
    mut emit: Option<&mut Emit<'_, '_>>,
) -> bool {
    chunk.trim();
    if chunk.lo >= chunk.hi {
        return false;
    }
    let mut start = chunk.lo;
    let mut count = 0;
    for i in chunk.lo..=chunk.hi {
        if i != chunk.hi {
            let c = &chunk.items[i];
            if c.kind == Kind::Comma {
                return false;
            }
            if !is_ws(c) {
                continue;
            }
        }
        let mut piece = Chunk {
            items: chunk.items,
            lo: start,
            hi: i,
        };
        piece.trim();
        if piece.lo < piece.hi {
            if let Some(emit) = emit.as_mut()
                && count > 0
            {
                emit.out.push(b' ');
            }
            if !match_single(input, kind, ident, piece, independent, emit.as_deref_mut()) {
                return false;
            }
            count += 1;
        }
        start = i + 1;
    }
    count > 0
}

fn match_component(
    input: &Input<'_>,
    comp: &SynComponent,
    mut chunk: Chunk<'_>,
    independent: bool,
    mut emit: Option<&mut Emit<'_, '_>>,
) -> bool {
    chunk.trim();
    if chunk.lo >= chunk.hi {
        return false;
    }
    let ident = comp.ident.as_deref();
    if comp.kind == SynKind::TransformList {
        return match_space_list(
            input,
            SynKind::TransformFunction,
            None,
            chunk,
            independent,
            emit,
        );
    }
    match comp.mult {
        Mult::Space => match_space_list(input, comp.kind, ident, chunk, independent, emit),
        Mult::Comma => {
            let mut start = chunk.lo;
            let mut count = 0;
            for i in chunk.lo..=chunk.hi {
                if i != chunk.hi && chunk.items[i].kind != Kind::Comma {
                    continue;
                }
                let piece = Chunk {
                    items: chunk.items,
                    lo: start,
                    hi: i,
                };
                if let Some(emit) = emit.as_mut()
                    && count > 0
                {
                    emit.out.extend_from_slice(b", ");
                }
                if !match_single(
                    input,
                    comp.kind,
                    ident,
                    piece,
                    independent,
                    emit.as_deref_mut(),
                ) {
                    return false;
                }
                count += 1;
                start = i + 1;
            }
            count > 0
        }
        Mult::None => match_single(input, comp.kind, ident, chunk, independent, emit),
    }
}

fn value_tokens_ok(input: &Input<'_>, items: &[Component]) -> bool {
    if items
        .iter()
        .any(|c| c.kind == Kind::Semicolon || (c.kind == Kind::Delim && c.delimiter == b'!'))
    {
        return false;
    }
    let mut queue = std::collections::VecDeque::from([items]);
    while let Some(level) = queue.pop_front() {
        for c in level {
            if c.kind == Kind::Function
                && input.value(c).is_some_and(|name| {
                    caseless(name, "var") || caseless(name, "env") || caseless(name, "attr")
                })
            {
                return false;
            }
            if let Some(children) = &c.children {
                queue.push_back(children);
            }
        }
    }
    true
}

fn bad_url(text: &[u8]) -> bool {
    let mut p = 0;
    while let Some(found) = text[p..].windows(4).position(|w| w == b"url(") {
        let at = p + found;
        if at != 0 {
            let prev = text[at - 1];
            if prev.is_ascii_alphanumeric() || prev == b'-' || prev == b'_' {
                p = at + 4;
                continue;
            }
        }
        let mut q = at + 4;
        while q < text.len() && matches!(text[q], b' ' | b'\t' | b'\n' | b'\r') {
            q += 1;
        }
        if q < text.len() && (text[q] == b'"' || text[q] == b'\'') {
            p = q;
            continue;
        }
        while q < text.len() && text[q] != b')' {
            if matches!(text[q], b'"' | b'\'' | b'(') {
                return true;
            }
            q += 1;
        }
        p = if q < text.len() { q + 1 } else { q };
    }
    false
}

fn match_value(
    syntax: &SyntaxDef,
    value: &[u8],
    independent: bool,
    ctx: Option<&Context<'_>>,
    mut computed: Option<&mut Vec<u8>>,
) -> bool {
    let input = Input { text: value };
    let (items, tokens_valid) = css::parse(value);
    let mut ok = tokens_valid && value_tokens_ok(&input, &items) && !bad_url(value);
    let mut chunk = Chunk {
        items: &items,
        lo: 0,
        hi: items.len(),
    };
    chunk.trim();
    if ok && chunk.lo >= chunk.hi {
        ok = false;
    }
    if ok && chunk.hi - chunk.lo == 1 {
        let c = &items[chunk.lo];
        if c.kind == Kind::Ident && css_wide_keyword(input.text_of(c)) {
            ok = false;
        }
    }
    if ok && !syntax.universal {
        let mut matched = false;
        for comp in &syntax.components {
            if matched {
                break;
            }
            matched = match computed.as_deref_mut() {
                Some(out) => {
                    out.clear();
                    let mut emit = Emit { out, ctx };
                    match_component(&input, comp, chunk, independent, Some(&mut emit))
                }
                None => match_component(&input, comp, chunk, independent, None),
            };
        }
        ok = matched;
    }
    ok
}

pub fn matches(syntax: &SyntaxDef, value: &[u8]) -> bool {
    match_value(syntax, value, false, None, None)
}

pub fn initial_valid(syntax: &SyntaxDef, value: &[u8]) -> bool {
    match_value(syntax, value, true, None, None)
}

pub fn compute(syntax: &SyntaxDef, value: &[u8], ctx: Option<&Context<'_>>) -> Option<Vec<u8>> {
    if syntax.universal {
        return None;
    }
    let mut out = Vec::new();
    match_value(syntax, value, false, ctx, Some(&mut out)).then_some(out)
}
