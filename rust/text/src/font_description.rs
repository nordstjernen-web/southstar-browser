//! Southstar text — font descriptions: the family, style, weight, width, size and features a font is asked for, and their string form.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use core::ffi::{c_char, c_long};
use core::fmt;

use crate::ffi::harfbuzz_strings::{normalize_feature, normalize_variation};
use crate::types::{FontColor, Gravity, SCALE, Stretch, Style, Variant, Weight, font_mask};

#[cfg(test)]
#[path = "tests/font_description.rs"]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Width(pub i32);

impl Width {
    pub const ULTRA_CONDENSED: Width = Width(500);
    pub const EXTRA_CONDENSED: Width = Width(625);
    pub const CONDENSED: Width = Width(750);
    pub const SEMI_CONDENSED: Width = Width(875);
    pub const NORMAL: Width = Width(1000);
    pub const SEMI_EXPANDED: Width = Width(1125);
    pub const EXPANDED: Width = Width(1250);
    pub const EXTRA_EXPANDED: Width = Width(1500);
    pub const ULTRA_EXPANDED: Width = Width(2000);

    pub fn from_stretch(stretch: Stretch) -> Width {
        match stretch {
            Stretch::UltraCondensed => Width::ULTRA_CONDENSED,
            Stretch::ExtraCondensed => Width::EXTRA_CONDENSED,
            Stretch::Condensed => Width::CONDENSED,
            Stretch::SemiCondensed => Width::SEMI_CONDENSED,
            Stretch::Normal => Width::NORMAL,
            Stretch::SemiExpanded => Width::SEMI_EXPANDED,
            Stretch::Expanded => Width::EXPANDED,
            Stretch::ExtraExpanded => Width::EXTRA_EXPANDED,
            Stretch::UltraExpanded => Width::ULTRA_EXPANDED,
        }
    }
}

pub const FONT_MASK_WIDTH: u32 = font_mask::STRETCH;

#[derive(Clone, Debug)]
pub struct FontDescription {
    family_name: Option<String>,
    style: i32,
    variant: i32,
    weight: i32,
    width: i32,
    gravity: i32,
    color: i32,
    variations: Option<String>,
    features: Option<String>,
    mask: u16,
    size_is_absolute: bool,
    size: i32,
}

impl Default for FontDescription {
    fn default() -> FontDescription {
        FontDescription::new()
    }
}

pub(crate) fn c_double_to_int(value: f64) -> i32 {
    if cfg!(any(target_arch = "x86", target_arch = "x86_64")) {
        if value.is_nan() || value >= 2147483648.0 || value <= -2147483649.0 {
            i32::MIN
        } else {
            value as i32
        }
    } else {
        value as i32
    }
}

impl FontDescription {
    pub const fn new() -> FontDescription {
        FontDescription {
            family_name: None,
            style: Style::Normal as i32,
            variant: Variant::Normal as i32,
            weight: Weight::NORMAL.0,
            width: Width::NORMAL.0,
            gravity: Gravity::South as i32,
            color: FontColor::DontCare as i32,
            variations: None,
            features: None,
            mask: 0,
            size_is_absolute: false,
            size: 0,
        }
    }

    pub fn set_family(&mut self, family: Option<&str>) {
        match family {
            Some(family) => {
                self.family_name = Some(family.to_owned());
                self.mask |= font_mask::FAMILY as u16;
            }
            None => {
                self.family_name = None;
                self.mask &= !(font_mask::FAMILY as u16);
            }
        }
    }

    pub fn family(&self) -> Option<&str> {
        self.family_name.as_deref()
    }

    pub fn set_style(&mut self, style: Style) {
        self.set_style_value(style as i32);
    }

    pub fn set_style_value(&mut self, style: i32) {
        self.style = style;
        self.mask |= font_mask::STYLE as u16;
    }

    pub fn style(&self) -> Style {
        Style::from_i32(self.style).unwrap_or(Style::Normal)
    }

    pub fn style_value(&self) -> i32 {
        self.style
    }

    pub fn set_variant(&mut self, variant: Variant) {
        self.set_variant_value(variant as i32);
    }

    pub fn set_variant_value(&mut self, variant: i32) {
        self.variant = variant;
        self.mask |= font_mask::VARIANT as u16;
    }

    pub fn variant(&self) -> Variant {
        Variant::from_i32(self.variant).unwrap_or(Variant::Normal)
    }

    pub fn variant_value(&self) -> i32 {
        self.variant
    }

    pub fn set_weight(&mut self, weight: Weight) {
        self.weight = weight.0;
        self.mask |= font_mask::WEIGHT as u16;
    }

    pub fn weight(&self) -> Weight {
        Weight(self.weight)
    }

    pub fn set_stretch(&mut self, stretch: Stretch) {
        self.width = Width::from_stretch(stretch).0;
        self.mask |= font_mask::STRETCH as u16;
    }

    pub fn set_width(&mut self, width: Width) {
        self.width = width.0;
        self.mask |= FONT_MASK_WIDTH as u16;
    }

    pub fn width(&self) -> Width {
        Width(self.width)
    }

    pub fn set_size(&mut self, size: i32) {
        if size < 0 {
            return;
        }
        self.size = size;
        self.size_is_absolute = false;
        self.mask |= font_mask::SIZE as u16;
    }

    pub fn size(&self) -> i32 {
        self.size
    }

    pub fn set_absolute_size(&mut self, size: f64) {
        if size.is_nan() || size < 0.0 {
            return;
        }
        self.size = c_double_to_int(size);
        self.size_is_absolute = true;
        self.mask |= font_mask::SIZE as u16;
    }

    pub fn size_is_absolute(&self) -> bool {
        self.size_is_absolute
    }

    pub fn set_gravity(&mut self, gravity: Gravity) {
        if gravity == Gravity::Auto {
            self.unset_fields(font_mask::GRAVITY);
            return;
        }
        self.gravity = gravity as i32;
        self.mask |= font_mask::GRAVITY as u16;
    }

    pub fn gravity(&self) -> Gravity {
        Gravity::from_i32(self.gravity).unwrap_or(Gravity::South)
    }

    pub fn gravity_value(&self) -> i32 {
        self.gravity
    }

    pub fn set_variations(&mut self, variations: Option<&str>) {
        match variations {
            Some(variations) => {
                self.variations = Some(variations.to_owned());
                self.mask |= font_mask::VARIATIONS as u16;
            }
            None => {
                self.variations = None;
                self.mask &= !(font_mask::VARIATIONS as u16);
            }
        }
    }

    pub fn variations(&self) -> Option<&str> {
        self.variations.as_deref()
    }

    pub fn set_features(&mut self, features: Option<&str>) {
        match features {
            Some(features) => {
                self.features = Some(features.to_owned());
                self.mask |= font_mask::FEATURES as u16;
            }
            None => {
                self.features = None;
                self.mask &= !(font_mask::FEATURES as u16);
            }
        }
    }

    pub fn features(&self) -> Option<&str> {
        self.features.as_deref()
    }

    pub fn set_color(&mut self, color: FontColor) {
        if self.color == color as i32 {
            return;
        }
        self.color = color as i32;
        self.mask |= font_mask::COLOR as u16;
    }

    pub fn color(&self) -> FontColor {
        FontColor::from_i32(self.color).unwrap_or(FontColor::DontCare)
    }

    pub fn color_value(&self) -> i32 {
        self.color
    }

    pub fn set_fields(&self) -> u32 {
        u32::from(self.mask)
    }

    pub fn unset_fields(&mut self, to_unset: u32) {
        let mut unset_desc = FontDescription::new();
        unset_desc.mask = to_unset as u16;
        self.merge(Some(&unset_desc), true);
        self.mask &= !(to_unset as u16);
    }

    pub fn merge(&mut self, desc_to_merge: Option<&FontDescription>, replace_existing: bool) {
        let Some(other) = desc_to_merge else {
            return;
        };
        let new_mask = if replace_existing {
            other.mask
        } else {
            other.mask & !self.mask
        };
        let has = |bit: u32| new_mask & bit as u16 != 0;

        if has(font_mask::FAMILY) {
            self.set_family(other.family_name.as_deref());
        }
        if has(font_mask::STYLE) {
            self.style = other.style;
        }
        if has(font_mask::VARIANT) {
            self.variant = other.variant;
        }
        if has(font_mask::WEIGHT) {
            self.weight = other.weight;
        }
        if has(FONT_MASK_WIDTH) {
            self.width = other.width;
        }
        if has(font_mask::SIZE) {
            self.size = other.size;
            self.size_is_absolute = other.size_is_absolute;
        }
        if has(font_mask::GRAVITY) {
            self.gravity = other.gravity;
        }
        if has(font_mask::COLOR) {
            self.color = other.color;
        }
        if has(font_mask::VARIATIONS) {
            self.set_variations(other.variations.as_deref());
        }
        if has(font_mask::FEATURES) {
            self.set_features(other.features.as_deref());
        }

        self.mask |= new_mask;
    }

    pub fn equal(&self, other: &FontDescription) -> bool {
        self.style == other.style
            && self.variant == other.variant
            && self.weight == other.weight
            && self.width == other.width
            && self.size == other.size
            && self.size_is_absolute == other.size_is_absolute
            && self.gravity == other.gravity
            && self.color == other.color
            && match (&self.family_name, &other.family_name) {
                (None, None) => true,
                (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
                _ => false,
            }
            && self.variations == other.variations
            && self.features == other.features
    }

    pub fn hash(&self) -> u32 {
        let mut hash = 0u32;
        if let Some(family) = &self.family_name {
            hash = case_insensitive_hash(family.as_bytes());
        }
        if let Some(variations) = &self.variations {
            hash ^= str_hash(variations.as_bytes());
        }
        if let Some(features) = &self.features {
            hash ^= str_hash(features.as_bytes());
        }
        hash ^= self.size as u32;
        hash ^= if self.size_is_absolute {
            0xc33c_a55a
        } else {
            0
        };
        hash ^= (self.style as u32).wrapping_shl(16);
        hash ^= (self.variant as u32).wrapping_shl(18);
        hash ^= (self.weight as u32).wrapping_shl(16);
        hash ^= (self.width as u32).wrapping_shl(22);
        hash ^= (self.gravity as u32).wrapping_shl(28);
        hash ^= (self.color as u32).wrapping_shl(29);
        hash
    }

    pub fn from_string(text: &str) -> FontDescription {
        parse_description(until_nul(text.as_bytes()))
    }
}

impl PartialEq for FontDescription {
    fn eq(&self, other: &FontDescription) -> bool {
        self.equal(other)
    }
}

impl Eq for FontDescription {}

impl core::hash::Hash for FontDescription {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        state.write_u32(FontDescription::hash(self));
    }
}

fn until_nul(bytes: &[u8]) -> &[u8] {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    &bytes[..end]
}

fn signed_char(b: u8) -> u32 {
    (b as c_char) as i32 as u32
}

fn case_insensitive_hash(key: &[u8]) -> u32 {
    let lower = |b: u8| signed_char(b.to_ascii_lowercase());
    let Some((&first, rest)) = key.split_first() else {
        return 0;
    };
    let mut h = lower(first);
    for &b in rest {
        h = (h << 5).wrapping_sub(h).wrapping_add(lower(b));
    }
    h
}

fn str_hash(key: &[u8]) -> u32 {
    let mut h = 5381u32;
    for &b in key {
        h = (h << 5)
            .wrapping_add(h)
            .wrapping_add((b as i8) as i32 as u32);
    }
    h
}

struct FieldMap {
    value: i32,
    name: &'static str,
}

const fn field(value: i32, name: &'static str) -> FieldMap {
    FieldMap { value, name }
}

static STYLE_MAP: [FieldMap; 4] = [
    field(Style::Normal as i32, ""),
    field(Style::Normal as i32, "Roman"),
    field(Style::Oblique as i32, "Oblique"),
    field(Style::Italic as i32, "Italic"),
];

static VARIANT_MAP: [FieldMap; 7] = [
    field(Variant::Normal as i32, ""),
    field(Variant::SmallCaps as i32, "Small-Caps"),
    field(Variant::AllSmallCaps as i32, "All-Small-Caps"),
    field(Variant::PetiteCaps as i32, "Petite-Caps"),
    field(Variant::AllPetiteCaps as i32, "All-Petite-Caps"),
    field(Variant::Unicase as i32, "Unicase"),
    field(Variant::TitleCaps as i32, "Title-Caps"),
];

static WEIGHT_MAP: [FieldMap; 21] = [
    field(Weight::THIN.0, "Thin"),
    field(Weight::ULTRALIGHT.0, "Ultra-Light"),
    field(Weight::ULTRALIGHT.0, "Extra-Light"),
    field(Weight::LIGHT.0, "Light"),
    field(Weight::SEMILIGHT.0, "Semi-Light"),
    field(Weight::SEMILIGHT.0, "Demi-Light"),
    field(Weight::BOOK.0, "Book"),
    field(Weight::NORMAL.0, ""),
    field(Weight::NORMAL.0, "Regular"),
    field(Weight::MEDIUM.0, "Medium"),
    field(Weight::SEMIBOLD.0, "Semi-Bold"),
    field(Weight::SEMIBOLD.0, "Demi-Bold"),
    field(Weight::BOLD.0, "Bold"),
    field(Weight::ULTRABOLD.0, "Ultra-Bold"),
    field(Weight::ULTRABOLD.0, "Extra-Bold"),
    field(Weight::HEAVY.0, "Heavy"),
    field(Weight::HEAVY.0, "Black"),
    field(Weight::ULTRAHEAVY.0, "Ultra-Heavy"),
    field(Weight::ULTRAHEAVY.0, "Extra-Heavy"),
    field(Weight::ULTRAHEAVY.0, "Ultra-Black"),
    field(Weight::ULTRAHEAVY.0, "Extra-Black"),
];

static WIDTH_MAP: [FieldMap; 9] = [
    field(Width::ULTRA_CONDENSED.0, "Ultra-Condensed"),
    field(Width::EXTRA_CONDENSED.0, "Extra-Condensed"),
    field(Width::CONDENSED.0, "Condensed"),
    field(Width::SEMI_CONDENSED.0, "Semi-Condensed"),
    field(Width::NORMAL.0, ""),
    field(Width::SEMI_EXPANDED.0, "Semi-Expanded"),
    field(Width::EXPANDED.0, "Expanded"),
    field(Width::EXTRA_EXPANDED.0, "Extra-Expanded"),
    field(Width::ULTRA_EXPANDED.0, "Ultra-Expanded"),
];

static GRAVITY_MAP: [FieldMap; 8] = [
    field(Gravity::South as i32, "Not-Rotated"),
    field(Gravity::South as i32, "South"),
    field(Gravity::North as i32, "Upside-Down"),
    field(Gravity::North as i32, "North"),
    field(Gravity::East as i32, "Rotated-Left"),
    field(Gravity::East as i32, "East"),
    field(Gravity::West as i32, "Rotated-Right"),
    field(Gravity::West as i32, "West"),
];

static COLOR_MAP: [FieldMap; 2] = [
    field(FontColor::Required as i32, "With-Color"),
    field(FontColor::Forbidden as i32, "Without-Color"),
];

#[derive(Clone, Copy)]
enum FieldKind {
    Weight,
    Style,
    Width,
    Variant,
    Gravity,
    Color,
}

impl FieldKind {
    const ALL: [FieldKind; 6] = [
        FieldKind::Weight,
        FieldKind::Style,
        FieldKind::Width,
        FieldKind::Variant,
        FieldKind::Gravity,
        FieldKind::Color,
    ];

    fn name(self) -> &'static str {
        match self {
            FieldKind::Weight => "weight",
            FieldKind::Style => "style",
            FieldKind::Width => "width",
            FieldKind::Variant => "variant",
            FieldKind::Gravity => "gravity",
            FieldKind::Color => "color",
        }
    }

    fn map(self) -> &'static [FieldMap] {
        match self {
            FieldKind::Weight => &WEIGHT_MAP,
            FieldKind::Style => &STYLE_MAP,
            FieldKind::Width => &WIDTH_MAP,
            FieldKind::Variant => &VARIANT_MAP,
            FieldKind::Gravity => &GRAVITY_MAP,
            FieldKind::Color => &COLOR_MAP,
        }
    }

    fn mask(self) -> u32 {
        match self {
            FieldKind::Weight => font_mask::WEIGHT,
            FieldKind::Style => font_mask::STYLE,
            FieldKind::Width => FONT_MASK_WIDTH,
            FieldKind::Variant => font_mask::VARIANT,
            FieldKind::Gravity => font_mask::GRAVITY,
            FieldKind::Color => font_mask::COLOR,
        }
    }

    fn value(self, desc: &FontDescription) -> i32 {
        match self {
            FieldKind::Weight => desc.weight,
            FieldKind::Style => desc.style,
            FieldKind::Width => desc.width,
            FieldKind::Variant => desc.variant,
            FieldKind::Gravity => desc.gravity,
            FieldKind::Color => desc.color,
        }
    }

    fn store(self, desc: &mut FontDescription, value: i32) {
        match self {
            FieldKind::Weight => desc.weight = value,
            FieldKind::Style => desc.style = value,
            FieldKind::Width => desc.width = value,
            FieldKind::Variant => desc.variant = value,
            FieldKind::Gravity => desc.gravity = value,
            FieldKind::Color => desc.color = value,
        }
    }
}

fn is_c_space(b: u8) -> bool {
    is_ascii_space(b) || b == 0x0b
}

fn is_ascii_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn field_matches(name: &[u8], word: &[u8]) -> bool {
    let mut i = 0;
    let mut j = 0;
    let mut n = word.len();
    while n > 0 && i < name.len() && j < word.len() {
        let c1 = name[i].to_ascii_lowercase();
        let c2 = word[j].to_ascii_lowercase();
        if c1 != c2 {
            if c1 == b'-' {
                i += 1;
                continue;
            }
            return false;
        }
        i += 1;
        j += 1;
        n -= 1;
    }
    n == 0 && i == name.len()
}

fn parse_int(word: &[u8]) -> Option<i32> {
    let mut pos = 0;
    while pos < word.len() && is_c_space(word[pos]) {
        pos += 1;
    }
    let negative = match word.get(pos) {
        Some(b'-') => {
            pos += 1;
            true
        }
        Some(b'+') => {
            pos += 1;
            false
        }
        _ => false,
    };
    let digits_start = pos;
    let mut magnitude: i128 = 0;
    while pos < word.len() && word[pos].is_ascii_digit() {
        magnitude = (magnitude * 10 + i128::from(word[pos] - b'0')).min(i128::from(u64::MAX));
        pos += 1;
    }
    if pos == digits_start || pos != word.len() {
        return None;
    }
    let signed = if negative { -magnitude } else { magnitude };
    let val = signed.clamp(i128::from(c_long::MIN), i128::from(c_long::MAX));
    (val >= 0 && val <= i128::from(i32::MAX)).then_some(val as i32)
}

fn find_field(what: Option<&str>, map: &[FieldMap], word: &[u8]) -> Option<i32> {
    let mut text = word;
    let mut had_prefix = false;
    if let Some(what) = what {
        let what = what.as_bytes();
        let i = what.len();
        if text.len() > i && text.starts_with(what) && text[i] == b'=' {
            text = &text[i + 1..];
            had_prefix = true;
        }
    }

    for entry in map {
        if !entry.name.is_empty() && field_matches(entry.name.as_bytes(), text) {
            return Some(entry.value);
        }
    }

    if what.is_none() || had_prefix {
        return parse_int(text);
    }
    None
}

fn find_field_any(word: &[u8], mut desc: Option<&mut FontDescription>) -> bool {
    if field_matches(b"Normal", word) {
        return true;
    }
    for kind in FieldKind::ALL {
        if let Some(value) = find_field(Some(kind.name()), kind.map(), word) {
            if let Some(desc) = desc.as_deref_mut() {
                kind.store(desc, value);
                desc.mask |= kind.mask() as u16;
            }
            return true;
        }
    }
    false
}

fn getword(text: &[u8], mut last: usize, stop: &[u8]) -> (usize, usize) {
    while last > 0 && is_ascii_space(text[last - 1]) {
        last -= 1;
    }
    let mut result = last;
    while result > 0 && !is_ascii_space(text[result - 1]) && !stop.contains(&text[result - 1]) {
        result -= 1;
    }
    (result, last - result)
}

fn scan_strtod(word: &[u8]) -> Option<(f64, usize)> {
    let mut pos = 0;
    while pos < word.len() && is_c_space(word[pos]) {
        pos += 1;
    }
    let negative = match word.get(pos) {
        Some(b'-') => {
            pos += 1;
            true
        }
        Some(b'+') => {
            pos += 1;
            false
        }
        _ => false,
    };
    let sign = if negative { -1.0 } else { 1.0 };

    if word.len() >= pos + 2
        && word[pos] == b'0'
        && (word[pos + 1] | 0x20) == b'x'
        && let Some((value, end)) = scan_hex(word, pos + 2)
    {
        return Some((sign * value, end));
    }

    let mantissa_start = pos;
    while pos < word.len() && word[pos].is_ascii_digit() {
        pos += 1;
    }
    let mut digits = pos - mantissa_start;
    if pos < word.len() && word[pos] == b'.' {
        let fraction_start = pos + 1;
        let mut end = fraction_start;
        while end < word.len() && word[end].is_ascii_digit() {
            end += 1;
        }
        digits += end - fraction_start;
        if digits > 0 {
            pos = end;
        }
    }
    if digits == 0 {
        return None;
    }
    if pos < word.len() && (word[pos] | 0x20) == b'e' {
        let mut end = pos + 1;
        if end < word.len() && (word[end] == b'+' || word[end] == b'-') {
            end += 1;
        }
        let exponent_digits = end;
        while end < word.len() && word[end].is_ascii_digit() {
            end += 1;
        }
        if end > exponent_digits {
            pos = end;
        }
    }
    let text = core::str::from_utf8(&word[mantissa_start..pos]).ok()?;
    let value: f64 = text.parse().ok()?;
    Some((sign * value, pos))
}

fn scan_hex(word: &[u8], start: usize) -> Option<(f64, usize)> {
    let mut pos = start;
    let mut value = 0.0f64;
    let mut digits = 0;
    while pos < word.len() && word[pos].is_ascii_hexdigit() {
        value = value * 16.0 + f64::from((word[pos] as char).to_digit(16)?);
        pos += 1;
        digits += 1;
    }
    let mut scale = 0i32;
    if pos < word.len() && word[pos] == b'.' {
        let mut end = pos + 1;
        let mut fraction_digits = 0;
        while end < word.len() && word[end].is_ascii_hexdigit() {
            value = value * 16.0 + f64::from((word[end] as char).to_digit(16)?);
            scale -= 4;
            end += 1;
            fraction_digits += 1;
        }
        if digits + fraction_digits > 0 {
            pos = end;
            digits += fraction_digits;
        }
    }
    if digits == 0 {
        return None;
    }
    if pos < word.len() && (word[pos] | 0x20) == b'p' {
        let mut end = pos + 1;
        let negative = match word.get(end) {
            Some(b'-') => {
                end += 1;
                true
            }
            Some(b'+') => {
                end += 1;
                false
            }
            _ => false,
        };
        let exponent_start = end;
        let mut exponent = 0i32;
        while end < word.len() && word[end].is_ascii_digit() {
            exponent = exponent
                .saturating_mul(10)
                .saturating_add(i32::from(word[end] - b'0'));
            end += 1;
        }
        if end > exponent_start {
            pos = end;
            scale = scale.saturating_add(if negative { -exponent } else { exponent });
        }
    }
    Some((value * 2f64.powi(scale), pos))
}

fn parse_size(word: &[u8]) -> Option<(i32, bool)> {
    let (size, end) = scan_strtod(word)?;
    let whole = end == word.len();
    let pixels = end + 2 == word.len() && &word[end..] == b"px";
    if (whole || pixels) && (0.0..=1_000_000.0).contains(&size) {
        Some(((size * f64::from(SCALE) + 0.5) as i32, end < word.len()))
    } else {
        None
    }
}

fn word_text(word: &[u8]) -> String {
    String::from_utf8_lossy(word).into_owned()
}

fn strip_ascii_space(mut s: &[u8]) -> &[u8] {
    while let Some((&first, rest)) = s.split_first() {
        if !is_ascii_space(first) {
            break;
        }
        s = rest;
    }
    while let Some((&last, rest)) = s.split_last() {
        if !is_ascii_space(last) {
            break;
        }
        s = rest;
    }
    s
}

fn parse_description(text: &[u8]) -> FontDescription {
    let mut desc = FontDescription::new();
    desc.mask =
        (font_mask::STYLE | font_mask::WEIGHT | font_mask::VARIANT | FONT_MASK_WIDTH) as u16;

    let mut last = text.len();
    let (p, len) = getword(text, last, b"");
    if len != 0 {
        let word = &text[p..p + len];
        if word[0] == b'#' {
            desc.features = Some(word_text(&word[1..]));
            desc.mask |= font_mask::FEATURES as u16;
            last = p;
        } else if word[0] == b'@' {
            desc.variations = Some(word_text(&word[1..]));
            desc.mask |= font_mask::VARIATIONS as u16;
            last = p;
        }
    }

    let (p, len) = getword(text, last, b",");
    if len != 0 {
        let word = &text[p..p + len];
        if desc.mask & font_mask::FEATURES as u16 == 0 && word[0] == b'#' {
            desc.features = Some(word_text(&word[1..]));
            desc.mask |= font_mask::FEATURES as u16;
            last = p;
        } else if desc.mask & font_mask::VARIATIONS as u16 == 0 && word[0] == b'@' {
            desc.variations = Some(word_text(&word[1..]));
            desc.mask |= font_mask::VARIATIONS as u16;
            last = p;
        }
    }

    let (p, len) = getword(text, last, b",");
    if len != 0
        && let Some((size, size_is_absolute)) = parse_size(&text[p..p + len])
    {
        desc.size = size;
        desc.size_is_absolute = size_is_absolute;
        desc.mask |= font_mask::SIZE as u16;
        last = p;
    }

    let (mut p, mut len) = getword(text, last, b",");
    while len != 0 {
        if !find_field_any(&text[p..p + len], Some(&mut desc)) {
            break;
        }
        last = p;
        (p, len) = getword(text, last, b",");
    }

    while last > 0 && is_ascii_space(text[last - 1]) {
        last -= 1;
    }
    if last > 0 && text[last - 1] == b',' {
        last -= 1;
    }
    while last > 0 && is_ascii_space(text[last - 1]) {
        last -= 1;
    }
    let mut start = 0;
    while last > start && is_ascii_space(text[start]) {
        start += 1;
    }

    if start != last {
        let families: Vec<&[u8]> = text[start..last]
            .split(|&b| b == b',')
            .map(strip_ascii_space)
            .collect();
        desc.family_name = Some(word_text(&families.join(&b","[..])));
        desc.mask |= font_mask::FAMILY as u16;
    }

    desc
}

fn append_space(out: &mut String) {
    if !out.is_empty() && !out.ends_with(' ') {
        out.push(' ');
    }
}

fn append_field(out: &mut String, kind: FieldKind, value: i32) {
    if let Some(entry) = kind.map().iter().find(|entry| entry.value == value) {
        if !entry.name.is_empty() {
            append_space(out);
            out.push_str(entry.name);
        }
        return;
    }
    append_space(out);
    out.push_str(kind.name());
    out.push('=');
    out.push_str(&value.to_string());
}

fn format_nearest_multiple(value: f64, factor: f64) -> String {
    let value = (value / factor).round() * factor;
    let eps = 0.5 * factor;
    let lo = value - eps;
    let hi = value + eps;

    if lo.floor() != hi.floor() {
        return (value.round() as i32).to_string();
    }

    let buf1 = format!("{lo:.8}");
    let buf2 = format!("{hi:.8}");
    let i = buf1
        .bytes()
        .zip(buf2.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or(buf1.len());
    let j = buf1.find('.').unwrap_or(0);
    format!("{value:.prec$}", prec = i.saturating_sub(j))
}

fn append_normalized(out: &mut String, list: &str, normalize: fn(&[u8]) -> Option<String>) {
    if !list.contains(' ') {
        out.push_str(list);
        return;
    }
    let bytes = list.as_bytes();
    let mut p = 0;
    while p < bytes.len() {
        let end = bytes[p..].iter().position(|&b| b == b',').map(|i| p + i);
        let item = &bytes[p..end.unwrap_or(bytes.len())];
        if let Some(text) = normalize(item) {
            if p > 0 {
                out.push(',');
            }
            out.push_str(&text);
        }
        match end {
            Some(end) => p = end + 1,
            None => break,
        }
    }
}

impl fmt::Display for FontDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut result = String::new();

        if let Some(family) = self
            .family_name
            .as_deref()
            .filter(|_| self.mask & font_mask::FAMILY as u16 != 0)
        {
            let family = until_nul(family.as_bytes());
            result.push_str(&String::from_utf8_lossy(family));
            let (p, len) = getword(family, family.len(), b",");
            if len != 0 {
                let word = &family[p..p + len];
                if find_field_any(word, None)
                    || (parse_size(word).is_some()
                        && self.weight == Weight::NORMAL.0
                        && self.style == Style::Normal as i32
                        && self.width == Width::NORMAL.0
                        && self.variant == Variant::Normal as i32
                        && self.mask & (font_mask::GRAVITY | font_mask::SIZE) as u16 == 0)
                {
                    result.push(',');
                }
            }
        }

        for kind in FieldKind::ALL {
            let always_printed = !matches!(kind, FieldKind::Gravity | FieldKind::Color);
            if always_printed || self.mask & kind.mask() as u16 != 0 {
                append_field(&mut result, kind, kind.value(self));
            }
        }

        if result.is_empty() {
            result.push_str("Normal");
        }

        if self.mask & font_mask::SIZE as u16 != 0 {
            result.push(' ');
            result.push_str(&format_nearest_multiple(
                f64::from(self.size) / f64::from(SCALE),
                1.0 / f64::from(SCALE),
            ));
            if self.size_is_absolute {
                result.push_str("px");
            }
        }

        if let Some(variations) = self
            .variations
            .as_deref()
            .filter(|v| self.mask & font_mask::VARIATIONS as u16 != 0 && !v.is_empty())
        {
            result.push_str(" @");
            append_normalized(&mut result, variations, normalize_variation);
        }

        if let Some(features) = self
            .features
            .as_deref()
            .filter(|v| self.mask & font_mask::FEATURES as u16 != 0 && !v.is_empty())
        {
            result.push_str(" #");
            append_normalized(&mut result, features, normalize_feature);
        }

        f.write_str(&result)
    }
}
