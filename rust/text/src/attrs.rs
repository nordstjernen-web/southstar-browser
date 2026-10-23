//! Southstar text — text attributes over byte ranges, sorted attribute lists, their string form and the iterator that merges them into fonts.
//! Copyright 2000-2002 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::fmt;

use crate::font_description::{FontDescription, Width, c_double_to_int};
use crate::language::Language;
use crate::types::{
    BaselineShift, Color, FontScale, Gravity, GravityHint, Overline, Rectangle, Stretch, Style,
    TextTransform, Underline, Variant, Weight, font_mask,
};

#[cfg(test)]
#[path = "tests/attrs.rs"]
mod tests;

pub const ATTR_INDEX_FROM_TEXT_BEGINNING: u32 = 0;
pub const ATTR_INDEX_TO_TEXT_END: u32 = u32::MAX;

const NULL_STRING: &str = if cfg!(windows) { "(NULL)" } else { "(null)" };

#[repr(i32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AttrType {
    Invalid = 0,
    Language = 1,
    Family = 2,
    Style = 3,
    Weight = 4,
    Variant = 5,
    Stretch = 6,
    Size = 7,
    FontDesc = 8,
    Foreground = 9,
    Background = 10,
    Underline = 11,
    Strikethrough = 12,
    Rise = 13,
    Shape = 14,
    Scale = 15,
    Fallback = 16,
    LetterSpacing = 17,
    UnderlineColor = 18,
    StrikethroughColor = 19,
    AbsoluteSize = 20,
    Gravity = 21,
    GravityHint = 22,
    FontFeatures = 23,
    ForegroundAlpha = 24,
    BackgroundAlpha = 25,
    AllowBreaks = 26,
    Show = 27,
    InsertHyphens = 28,
    Overline = 29,
    OverlineColor = 30,
    LineHeight = 31,
    AbsoluteLineHeight = 32,
    TextTransform = 33,
    Word = 34,
    Sentence = 35,
    BaselineShift = 36,
    FontScale = 37,
    Width = 38,
    WordSpacing = 39,
}

impl AttrType {
    pub fn from_number(number: i32) -> Option<AttrType> {
        const ALL: [AttrType; 40] = [
            AttrType::Invalid,
            AttrType::Language,
            AttrType::Family,
            AttrType::Style,
            AttrType::Weight,
            AttrType::Variant,
            AttrType::Stretch,
            AttrType::Size,
            AttrType::FontDesc,
            AttrType::Foreground,
            AttrType::Background,
            AttrType::Underline,
            AttrType::Strikethrough,
            AttrType::Rise,
            AttrType::Shape,
            AttrType::Scale,
            AttrType::Fallback,
            AttrType::LetterSpacing,
            AttrType::UnderlineColor,
            AttrType::StrikethroughColor,
            AttrType::AbsoluteSize,
            AttrType::Gravity,
            AttrType::GravityHint,
            AttrType::FontFeatures,
            AttrType::ForegroundAlpha,
            AttrType::BackgroundAlpha,
            AttrType::AllowBreaks,
            AttrType::Show,
            AttrType::InsertHyphens,
            AttrType::Overline,
            AttrType::OverlineColor,
            AttrType::LineHeight,
            AttrType::AbsoluteLineHeight,
            AttrType::TextTransform,
            AttrType::Word,
            AttrType::Sentence,
            AttrType::BaselineShift,
            AttrType::FontScale,
            AttrType::Width,
            AttrType::WordSpacing,
        ];
        usize::try_from(number)
            .ok()
            .and_then(|index| ALL.get(index))
            .copied()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShapeAttr {
    pub ink_rect: Rectangle,
    pub logical_rect: Rectangle,
    pub data: usize,
}

#[derive(Clone, Debug)]
pub enum AttrValue {
    Language(Option<Language>),
    Family(String),
    Style(i32),
    Weight(i32),
    Variant(i32),
    Stretch(i32),
    Size(i32),
    FontDesc(FontDescription),
    Foreground(Color),
    Background(Color),
    Underline(i32),
    Strikethrough(i32),
    Rise(i32),
    Shape(ShapeAttr),
    Scale(f64),
    Fallback(i32),
    LetterSpacing(i32),
    UnderlineColor(Color),
    StrikethroughColor(Color),
    AbsoluteSize(i32),
    Gravity(i32),
    GravityHint(i32),
    FontFeatures(String),
    ForegroundAlpha(i32),
    BackgroundAlpha(i32),
    AllowBreaks(i32),
    Show(i32),
    InsertHyphens(i32),
    Overline(i32),
    OverlineColor(Color),
    LineHeight(f64),
    AbsoluteLineHeight(i32),
    TextTransform(i32),
    Word(i32),
    Sentence(i32),
    BaselineShift(i32),
    FontScale(i32),
    Width(i32),
    WordSpacing(i32),
}

impl AttrValue {
    pub fn kind(&self) -> AttrType {
        match self {
            AttrValue::Language(_) => AttrType::Language,
            AttrValue::Family(_) => AttrType::Family,
            AttrValue::Style(_) => AttrType::Style,
            AttrValue::Weight(_) => AttrType::Weight,
            AttrValue::Variant(_) => AttrType::Variant,
            AttrValue::Stretch(_) => AttrType::Stretch,
            AttrValue::Size(_) => AttrType::Size,
            AttrValue::FontDesc(_) => AttrType::FontDesc,
            AttrValue::Foreground(_) => AttrType::Foreground,
            AttrValue::Background(_) => AttrType::Background,
            AttrValue::Underline(_) => AttrType::Underline,
            AttrValue::Strikethrough(_) => AttrType::Strikethrough,
            AttrValue::Rise(_) => AttrType::Rise,
            AttrValue::Shape(_) => AttrType::Shape,
            AttrValue::Scale(_) => AttrType::Scale,
            AttrValue::Fallback(_) => AttrType::Fallback,
            AttrValue::LetterSpacing(_) => AttrType::LetterSpacing,
            AttrValue::UnderlineColor(_) => AttrType::UnderlineColor,
            AttrValue::StrikethroughColor(_) => AttrType::StrikethroughColor,
            AttrValue::AbsoluteSize(_) => AttrType::AbsoluteSize,
            AttrValue::Gravity(_) => AttrType::Gravity,
            AttrValue::GravityHint(_) => AttrType::GravityHint,
            AttrValue::FontFeatures(_) => AttrType::FontFeatures,
            AttrValue::ForegroundAlpha(_) => AttrType::ForegroundAlpha,
            AttrValue::BackgroundAlpha(_) => AttrType::BackgroundAlpha,
            AttrValue::AllowBreaks(_) => AttrType::AllowBreaks,
            AttrValue::Show(_) => AttrType::Show,
            AttrValue::InsertHyphens(_) => AttrType::InsertHyphens,
            AttrValue::Overline(_) => AttrType::Overline,
            AttrValue::OverlineColor(_) => AttrType::OverlineColor,
            AttrValue::LineHeight(_) => AttrType::LineHeight,
            AttrValue::AbsoluteLineHeight(_) => AttrType::AbsoluteLineHeight,
            AttrValue::TextTransform(_) => AttrType::TextTransform,
            AttrValue::Word(_) => AttrType::Word,
            AttrValue::Sentence(_) => AttrType::Sentence,
            AttrValue::BaselineShift(_) => AttrType::BaselineShift,
            AttrValue::FontScale(_) => AttrType::FontScale,
            AttrValue::Width(_) => AttrType::Width,
            AttrValue::WordSpacing(_) => AttrType::WordSpacing,
        }
    }

    pub fn as_int(&self) -> Option<i32> {
        match *self {
            AttrValue::Style(v)
            | AttrValue::Weight(v)
            | AttrValue::Variant(v)
            | AttrValue::Stretch(v)
            | AttrValue::Width(v)
            | AttrValue::Underline(v)
            | AttrValue::Strikethrough(v)
            | AttrValue::Rise(v)
            | AttrValue::Fallback(v)
            | AttrValue::LetterSpacing(v)
            | AttrValue::WordSpacing(v)
            | AttrValue::Gravity(v)
            | AttrValue::GravityHint(v)
            | AttrValue::ForegroundAlpha(v)
            | AttrValue::BackgroundAlpha(v)
            | AttrValue::AllowBreaks(v)
            | AttrValue::Show(v)
            | AttrValue::InsertHyphens(v)
            | AttrValue::Overline(v)
            | AttrValue::AbsoluteLineHeight(v)
            | AttrValue::TextTransform(v)
            | AttrValue::Word(v)
            | AttrValue::Sentence(v)
            | AttrValue::BaselineShift(v)
            | AttrValue::FontScale(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match *self {
            AttrValue::Scale(v) | AttrValue::LineHeight(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            AttrValue::Family(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_language(&self) -> Option<Option<Language>> {
        match *self {
            AttrValue::Language(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_color(&self) -> Option<Color> {
        match *self {
            AttrValue::Foreground(c)
            | AttrValue::Background(c)
            | AttrValue::UnderlineColor(c)
            | AttrValue::StrikethroughColor(c)
            | AttrValue::OverlineColor(c) => Some(c),
            _ => None,
        }
    }

    pub fn as_size(&self) -> Option<(i32, bool)> {
        match *self {
            AttrValue::Size(v) => Some((v, false)),
            AttrValue::AbsoluteSize(v) => Some((v, true)),
            _ => None,
        }
    }

    pub fn as_font_desc(&self) -> Option<&FontDescription> {
        match self {
            AttrValue::FontDesc(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_font_features(&self) -> Option<&str> {
        match self {
            AttrValue::FontFeatures(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_shape(&self) -> Option<&ShapeAttr> {
        match self {
            AttrValue::Shape(v) => Some(v),
            _ => None,
        }
    }

    pub fn equal(&self, other: &AttrValue) -> bool {
        if self.kind() != other.kind() {
            return false;
        }
        match (self, other) {
            (AttrValue::Language(a), AttrValue::Language(b)) => a == b,
            (AttrValue::Family(a), AttrValue::Family(b))
            | (AttrValue::FontFeatures(a), AttrValue::FontFeatures(b)) => a == b,
            (AttrValue::FontDesc(a), AttrValue::FontDesc(b)) => {
                a.set_fields() == b.set_fields() && a.equal(b)
            }
            (AttrValue::Shape(a), AttrValue::Shape(b)) => a == b,
            _ => {
                if let (Some(a), Some(b)) = (self.as_int(), other.as_int()) {
                    a == b
                } else if let (Some(a), Some(b)) = (self.as_float(), other.as_float()) {
                    a == b
                } else if let (Some(a), Some(b)) = (self.as_color(), other.as_color()) {
                    a == b
                } else if let (Some(a), Some(b)) = (self.as_size(), other.as_size()) {
                    a.0 == b.0
                } else {
                    false
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Attribute {
    pub start_index: u32,
    pub end_index: u32,
    pub value: AttrValue,
}

impl Attribute {
    pub fn new(value: AttrValue) -> Attribute {
        Attribute {
            start_index: ATTR_INDEX_FROM_TEXT_BEGINNING,
            end_index: ATTR_INDEX_TO_TEXT_END,
            value,
        }
    }

    pub fn with_range(mut self, start_index: u32, end_index: u32) -> Attribute {
        self.start_index = start_index;
        self.end_index = end_index;
        self
    }

    pub fn kind(&self) -> AttrType {
        self.value.kind()
    }

    pub fn equal(&self, other: &Attribute) -> bool {
        self.value.equal(&other.value)
    }
}

pub fn attr_language_new(language: Option<Language>) -> Attribute {
    Attribute::new(AttrValue::Language(language))
}

pub fn attr_family_new(family: &str) -> Attribute {
    Attribute::new(AttrValue::Family(family.to_owned()))
}

pub fn attr_foreground_new(red: u16, green: u16, blue: u16) -> Attribute {
    Attribute::new(AttrValue::Foreground(Color { red, green, blue }))
}

pub fn attr_background_new(red: u16, green: u16, blue: u16) -> Attribute {
    Attribute::new(AttrValue::Background(Color { red, green, blue }))
}

pub fn attr_size_new(size: i32) -> Attribute {
    Attribute::new(AttrValue::Size(size))
}

pub fn attr_size_new_absolute(size: i32) -> Attribute {
    Attribute::new(AttrValue::AbsoluteSize(size))
}

pub fn attr_scale_new(scale_factor: f64) -> Attribute {
    Attribute::new(AttrValue::Scale(scale_factor))
}

pub fn attr_style_new(style: Style) -> Attribute {
    Attribute::new(AttrValue::Style(style as i32))
}

pub fn attr_weight_new(weight: Weight) -> Attribute {
    Attribute::new(AttrValue::Weight(weight.0))
}

pub fn attr_variant_new(variant: Variant) -> Attribute {
    Attribute::new(AttrValue::Variant(variant as i32))
}

pub fn attr_stretch_new(stretch: Stretch) -> Attribute {
    Attribute::new(AttrValue::Stretch(stretch as i32))
}

pub fn attr_font_desc_new(desc: &FontDescription) -> Attribute {
    Attribute::new(AttrValue::FontDesc(desc.clone()))
}

pub fn attr_underline_new(underline: Underline) -> Attribute {
    Attribute::new(AttrValue::Underline(underline as i32))
}

pub fn attr_underline_color_new(red: u16, green: u16, blue: u16) -> Attribute {
    Attribute::new(AttrValue::UnderlineColor(Color { red, green, blue }))
}

pub fn attr_strikethrough_new(strikethrough: bool) -> Attribute {
    Attribute::new(AttrValue::Strikethrough(i32::from(strikethrough)))
}

pub fn attr_strikethrough_color_new(red: u16, green: u16, blue: u16) -> Attribute {
    Attribute::new(AttrValue::StrikethroughColor(Color { red, green, blue }))
}

pub fn attr_rise_new(rise: i32) -> Attribute {
    Attribute::new(AttrValue::Rise(rise))
}

pub fn attr_font_scale_new(scale: FontScale) -> Attribute {
    Attribute::new(AttrValue::FontScale(scale as i32))
}

pub fn attr_fallback_new(enable_fallback: bool) -> Attribute {
    Attribute::new(AttrValue::Fallback(i32::from(enable_fallback)))
}

pub fn attr_letter_spacing_new(letter_spacing: i32) -> Attribute {
    Attribute::new(AttrValue::LetterSpacing(letter_spacing))
}

pub fn attr_shape_new_with_data(
    ink_rect: &Rectangle,
    logical_rect: &Rectangle,
    data: usize,
) -> Attribute {
    Attribute::new(AttrValue::Shape(ShapeAttr {
        ink_rect: *ink_rect,
        logical_rect: *logical_rect,
        data,
    }))
}

pub fn attr_shape_new(ink_rect: &Rectangle, logical_rect: &Rectangle) -> Attribute {
    attr_shape_new_with_data(ink_rect, logical_rect, 0)
}

pub fn attr_font_features_new(features: &str) -> Attribute {
    Attribute::new(AttrValue::FontFeatures(features.to_owned()))
}

pub fn attr_foreground_alpha_new(alpha: u16) -> Attribute {
    Attribute::new(AttrValue::ForegroundAlpha(i32::from(alpha)))
}

pub fn attr_background_alpha_new(alpha: u16) -> Attribute {
    Attribute::new(AttrValue::BackgroundAlpha(i32::from(alpha)))
}

pub fn attr_allow_breaks_new(allow_breaks: bool) -> Attribute {
    Attribute::new(AttrValue::AllowBreaks(i32::from(allow_breaks)))
}

pub fn attr_insert_hyphens_new(insert_hyphens: bool) -> Attribute {
    Attribute::new(AttrValue::InsertHyphens(i32::from(insert_hyphens)))
}

pub fn attr_show_new(flags: u32) -> Attribute {
    Attribute::new(AttrValue::Show(flags as i32))
}

pub fn attr_overline_new(overline: Overline) -> Attribute {
    Attribute::new(AttrValue::Overline(overline as i32))
}

pub fn attr_overline_color_new(red: u16, green: u16, blue: u16) -> Attribute {
    Attribute::new(AttrValue::OverlineColor(Color { red, green, blue }))
}

pub fn attr_line_height_new(factor: f64) -> Attribute {
    Attribute::new(AttrValue::LineHeight(factor))
}

pub fn attr_line_height_new_absolute(height: i32) -> Attribute {
    Attribute::new(AttrValue::AbsoluteLineHeight(height))
}

pub fn attr_text_transform_new(transform: TextTransform) -> Attribute {
    Attribute::new(AttrValue::TextTransform(transform as i32))
}

pub fn attr_gravity_new(gravity: Gravity) -> Attribute {
    Attribute::new(AttrValue::Gravity(gravity as i32))
}

pub fn attr_gravity_hint_new(hint: GravityHint) -> Attribute {
    Attribute::new(AttrValue::GravityHint(hint as i32))
}

pub fn attr_baseline_shift_new(shift: BaselineShift) -> Attribute {
    Attribute::new(AttrValue::BaselineShift(shift as i32))
}

#[derive(Debug, Default)]
pub struct AttrList {
    attributes: Option<Vec<Attribute>>,
}

impl Clone for AttrList {
    fn clone(&self) -> AttrList {
        AttrList {
            attributes: self
                .attributes
                .as_ref()
                .filter(|attributes| !attributes.is_empty())
                .cloned(),
        }
    }
}

impl AttrList {
    pub fn new() -> AttrList {
        AttrList::default()
    }

    pub fn as_slice(&self) -> &[Attribute] {
        self.attributes.as_deref().unwrap_or(&[])
    }

    fn insert_internal(&mut self, attr: Attribute, before: bool) {
        let start_index = attr.start_index;
        let attributes = self.attributes.get_or_insert_with(Vec::new);

        let Some(last_attr) = attributes.last() else {
            attributes.push(attr);
            return;
        };

        if last_attr.start_index < start_index || (!before && last_attr.start_index == start_index)
        {
            attributes.push(attr);
            return;
        }

        let position = attributes.iter().position(|cur| {
            cur.start_index > start_index || (before && cur.start_index == start_index)
        });
        if let Some(position) = position {
            attributes.insert(position, attr);
        }
    }

    pub fn insert(&mut self, attr: Attribute) {
        self.insert_internal(attr, false);
    }

    pub fn insert_before(&mut self, attr: Attribute) {
        self.insert_internal(attr, true);
    }

    pub fn attributes(&self) -> Vec<Attribute> {
        self.as_slice().to_vec()
    }

    pub fn equal(&self, other: &AttrList) -> bool {
        if core::ptr::eq(self, other) {
            return true;
        }
        let (Some(attrs), Some(other_attrs)) = (&self.attributes, &other.attributes) else {
            return self.attributes.is_none() && other.attributes.is_none();
        };
        if attrs.len() != other_attrs.len() {
            return false;
        }

        let mut skip_bitmask = 0u64;
        for attr in attrs {
            let mut attr_equal = false;
            for (other_index, other_attr) in other_attrs.iter().enumerate() {
                let other_bit = if other_index < 64 {
                    1u64 << other_index
                } else {
                    0
                };
                if skip_bitmask & other_bit != 0 {
                    continue;
                }
                if attr.start_index == other_attr.start_index
                    && attr.end_index == other_attr.end_index
                    && attr.equal(other_attr)
                {
                    skip_bitmask |= other_bit;
                    attr_equal = true;
                    break;
                }
            }
            if !attr_equal {
                return false;
            }
        }
        true
    }

    pub fn filter(&mut self, mut func: impl FnMut(&Attribute) -> bool) -> Option<AttrList> {
        let attributes = self.attributes.as_mut().filter(|a| !a.is_empty())?;
        let mut kept = Vec::with_capacity(attributes.len());
        let mut removed: Option<Vec<Attribute>> = None;
        for attr in attributes.drain(..) {
            if func(&attr) {
                removed.get_or_insert_with(Vec::new).push(attr);
            } else {
                kept.push(attr);
            }
        }
        *attributes = kept;
        removed.map(|attributes| AttrList {
            attributes: Some(attributes),
        })
    }

    pub fn iterator(&self) -> AttrIterator<'_> {
        AttrIterator::new(self)
    }
}

impl PartialEq for AttrList {
    fn eq(&self, other: &AttrList) -> bool {
        self.equal(other)
    }
}

fn push_escaped(out: &mut String, text: &str) {
    for &b in text.as_bytes() {
        match b {
            0 => break,
            0x08 => out.push_str("\\b"),
            0x0c => out.push_str("\\f"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x0b => out.push_str("\\v"),
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b if !(b' '..0o177).contains(&b) => {
                out.push('\\');
                out.push(char::from(b'0' + ((b >> 6) & 7)));
                out.push(char::from(b'0' + ((b >> 3) & 7)));
                out.push(char::from(b'0' + (b & 7)));
            }
            b => out.push(char::from(b)),
        }
    }
}

fn round_digits_half_even(digits: &[u8], keep: usize) -> (Vec<u8>, bool) {
    if digits.len() <= keep {
        let mut kept = digits.to_vec();
        kept.resize(keep, b'0');
        return (kept, false);
    }
    let mut kept = digits[..keep].to_vec();
    let first_dropped = digits[keep];
    let rest_nonzero = digits[keep + 1..].iter().any(|&d| d != b'0');
    let round_up = first_dropped > b'5'
        || (first_dropped == b'5'
            && (rest_nonzero || kept.last().is_some_and(|&d| (d - b'0') % 2 == 1)));
    if !round_up {
        return (kept, false);
    }
    for digit in kept.iter_mut().rev() {
        if *digit == b'9' {
            *digit = b'0';
        } else {
            *digit += 1;
            return (kept, false);
        }
    }
    kept.insert(0, b'1');
    kept.pop();
    (kept, true)
}

pub(crate) fn format_g17(value: f64) -> String {
    const PRECISION: i32 = 17;
    if value.is_nan() {
        return if value.is_sign_negative() && !cfg!(windows) {
            "-nan"
        } else {
            "nan"
        }
        .to_owned();
    }
    if value.is_infinite() {
        return if value < 0.0 { "-inf" } else { "inf" }.to_owned();
    }
    let sign = if value.is_sign_negative() { "-" } else { "" };
    if value == 0.0 {
        return format!("{sign}0");
    }

    let exact = format!("{:.800e}", value.abs());
    let (mantissa, exponent) = exact.split_once('e').unwrap_or((&exact, "0"));
    let mut exponent: i32 = exponent.parse().unwrap_or(0);
    let digits: Vec<u8> = mantissa.bytes().filter(u8::is_ascii_digit).collect();
    let (rounded, carried) = round_digits_half_even(&digits, PRECISION as usize);
    if carried {
        exponent += 1;
    }

    let mut out = String::from(sign);
    if !(-4..PRECISION).contains(&exponent) {
        let mut significant = rounded;
        while significant.len() > 1 && significant.last() == Some(&b'0') {
            significant.pop();
        }
        out.push(char::from(significant[0]));
        if significant.len() > 1 {
            out.push('.');
            out.extend(significant[1..].iter().map(|&d| char::from(d)));
        }
        out.push('e');
        out.push(if exponent < 0 { '-' } else { '+' });
        out.push_str(&format!("{:02}", exponent.unsigned_abs()));
        return out;
    }

    let decimals = (PRECISION - 1 - exponent) as usize;
    let mut text = String::new();
    if exponent >= 0 {
        let integer_len = exponent as usize + 1;
        text.extend(rounded[..integer_len].iter().map(|&d| char::from(d)));
        text.push('.');
        text.extend(rounded[integer_len..].iter().map(|&d| char::from(d)));
    } else {
        text.push_str("0.");
        for _ in 0..(-exponent - 1) {
            text.push('0');
        }
        text.extend(rounded.iter().map(|&d| char::from(d)));
    }
    debug_assert_eq!(text.len() - text.find('.').unwrap_or(0) - 1, decimals);
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    out.push_str(trimmed);
    out
}

fn attr_print(out: &mut String, attr: &Attribute) {
    out.push_str(&format!(
        "{} {} {}",
        attr.start_index,
        attr.end_index,
        attr.kind() as i32
    ));

    let value = &attr.value;
    if attr.kind() == AttrType::Shape {
        out.push_str(" shape");
    } else if let Some(text) = value.as_string() {
        out.push_str(" \"");
        push_escaped(out, text);
        out.push('"');
    } else if let Some(language) = value.as_language() {
        out.push(' ');
        out.push_str(language.map_or(NULL_STRING, Language::as_str));
    } else if let Some(integer) = value.as_int() {
        out.push_str(&format!(" {integer}"));
    } else if let Some(float) = value.as_float() {
        out.push(' ');
        out.push_str(&format_g17(float));
    } else if let Some(desc) = value.as_font_desc() {
        out.push_str(" \"");
        push_escaped(out, &desc.to_string());
        out.push('"');
    } else if let Some(color) = value.as_color() {
        out.push_str(&format!(
            " #{:04x}{:04x}{:04x}",
            color.red, color.green, color.blue
        ));
    } else if let Some((size, _)) = value.as_size() {
        out.push_str(&format!(" {size}"));
    } else if let Some(features) = value.as_font_features() {
        let end = features.find('\0').unwrap_or(features.len());
        out.push_str(" \"");
        out.push_str(&features[..end]);
        out.push('"');
    }
}

impl fmt::Display for AttrList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        for (i, attr) in self.as_slice().iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            attr_print(&mut out, attr);
        }
        f.write_str(&out)
    }
}

#[derive(Clone, Debug)]
pub struct AttrIterator<'a> {
    attrs: &'a [Attribute],
    attribute_stack: Vec<usize>,
    attr_index: usize,
    start_index: u32,
    end_index: u32,
}

impl<'a> AttrIterator<'a> {
    pub fn new(list: &'a AttrList) -> AttrIterator<'a> {
        let mut iterator = AttrIterator {
            attrs: list.as_slice(),
            attribute_stack: Vec::new(),
            attr_index: 0,
            start_index: 0,
            end_index: 0,
        };
        if !iterator.next() {
            iterator.end_index = u32::MAX;
        }
        iterator
    }

    pub fn range(&self) -> (i32, i32) {
        (
            self.start_index.min(i32::MAX as u32) as i32,
            self.end_index.min(i32::MAX as u32) as i32,
        )
    }

    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> bool {
        if self.attr_index >= self.attrs.len() && self.attribute_stack.is_empty() {
            return false;
        }

        self.start_index = self.end_index;
        self.end_index = u32::MAX;

        for i in (0..self.attribute_stack.len()).rev() {
            let attr = &self.attrs[self.attribute_stack[i]];
            if attr.end_index == self.start_index {
                self.attribute_stack.remove(i);
            } else {
                self.end_index = self.end_index.min(attr.end_index);
            }
        }

        while let Some(attr) = self.attrs.get(self.attr_index) {
            if attr.start_index != self.start_index {
                break;
            }
            if attr.end_index > self.start_index {
                self.attribute_stack.push(self.attr_index);
                self.end_index = self.end_index.min(attr.end_index);
            }
            self.attr_index += 1;
        }

        if let Some(attr) = self.attrs.get(self.attr_index) {
            self.end_index = self.end_index.min(attr.start_index);
        }

        true
    }

    fn stack(&self) -> impl DoubleEndedIterator<Item = &'a Attribute> + '_ {
        self.attribute_stack.iter().map(|&index| &self.attrs[index])
    }

    pub fn get(&self, kind: AttrType) -> Option<&'a Attribute> {
        self.stack().rev().find(|attr| attr.kind() == kind)
    }

    pub fn get_font(
        &self,
        desc: &mut FontDescription,
        mut language: Option<&mut Option<Language>>,
        mut extra_attrs: Option<&mut Vec<Attribute>>,
    ) {
        let mut mask = 0u32;
        let mut have_language = false;
        let mut scale = 0.0;
        let mut have_scale = false;

        if let Some(language) = language.as_deref_mut() {
            *language = None;
        }
        if let Some(extra_attrs) = extra_attrs.as_deref_mut() {
            extra_attrs.clear();
        }

        if self.attribute_stack.is_empty() {
            return;
        }

        for attr in self.stack().rev() {
            match &attr.value {
                AttrValue::FontDesc(attr_desc) => {
                    let new_mask = attr_desc.set_fields() & !mask;
                    mask |= new_mask;
                    desc.unset_fields(new_mask);
                    desc.merge(Some(attr_desc), false);
                }
                AttrValue::Family(family) => {
                    if mask & font_mask::FAMILY == 0 {
                        mask |= font_mask::FAMILY;
                        desc.set_family(Some(family));
                    }
                }
                AttrValue::Style(value) => {
                    if mask & font_mask::STYLE == 0 {
                        mask |= font_mask::STYLE;
                        desc.set_style_value(*value);
                    }
                }
                AttrValue::Variant(value) => {
                    if mask & font_mask::VARIANT == 0 {
                        mask |= font_mask::VARIANT;
                        desc.set_variant_value(*value);
                    }
                }
                AttrValue::Weight(value) => {
                    if mask & font_mask::WEIGHT == 0 {
                        mask |= font_mask::WEIGHT;
                        desc.set_weight(Weight(*value));
                    }
                }
                AttrValue::Stretch(value) => {
                    if mask & font_mask::STRETCH == 0 {
                        mask |= font_mask::STRETCH;
                        if let Some(stretch) = Stretch::from_i32(*value) {
                            desc.set_stretch(stretch);
                        }
                    }
                }
                AttrValue::Width(value) => {
                    if mask & font_mask::STRETCH == 0 {
                        mask |= font_mask::STRETCH;
                        desc.set_width(Width(*value));
                    }
                }
                AttrValue::Size(size) => {
                    if mask & font_mask::SIZE == 0 {
                        mask |= font_mask::SIZE;
                        desc.set_size(*size);
                    }
                }
                AttrValue::AbsoluteSize(size) => {
                    if mask & font_mask::SIZE == 0 {
                        mask |= font_mask::SIZE;
                        desc.set_absolute_size(f64::from(*size));
                    }
                }
                AttrValue::Scale(value) => {
                    if !have_scale {
                        have_scale = true;
                        scale = *value;
                    }
                }
                AttrValue::Language(value) => {
                    if let Some(language) = language.as_deref_mut()
                        && !have_language
                    {
                        have_language = true;
                        *language = *value;
                    }
                }
                _ => {
                    if let Some(extra_attrs) = extra_attrs.as_deref_mut() {
                        let kind = attr.kind();
                        let accumulates = matches!(
                            kind,
                            AttrType::FontFeatures | AttrType::BaselineShift | AttrType::FontScale
                        );
                        let found =
                            !accumulates && extra_attrs.iter().any(|old| old.kind() == kind);
                        if !found {
                            extra_attrs.push(attr.clone());
                        }
                    }
                }
            }
        }

        if let Some(extra_attrs) = extra_attrs {
            extra_attrs.reverse();
        }

        if have_scale {
            let size = scale * f64::from(desc.size());
            if desc.size_is_absolute() {
                desc.set_absolute_size(size);
            } else {
                desc.set_size(c_double_to_int(size));
            }
        }
    }

    pub fn get_attrs(&self) -> Vec<Attribute> {
        let mut attrs: Vec<Attribute> = Vec::new();
        for attr in self.stack().rev() {
            let kind = attr.kind();
            let accumulates = matches!(
                kind,
                AttrType::FontDesc | AttrType::BaselineShift | AttrType::FontScale
            );
            let found = !accumulates && attrs.iter().any(|old| old.kind() == kind);
            if !found {
                attrs.push(attr.clone());
            }
        }
        attrs.reverse();
        attrs
    }

    pub fn advance(&mut self, index: i32) -> bool {
        let (_, mut end_range) = self.range();
        while index >= end_range {
            if !self.next() {
                return false;
            }
            end_range = self.range().1;
        }
        true
    }
}
