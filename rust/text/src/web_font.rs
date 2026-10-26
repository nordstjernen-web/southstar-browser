//! Southstar text — web fonts indexed for fontconfig from read-fonts, giving the patterns FcFreeTypeQueryAll gives without FreeType opening the file.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#[cfg(test)]
#[path = "tests/web_font.rs"]
mod tests;

#[path = "web_font_languages.rs"]
mod languages;

use read_fonts::tables::cmap::{Cmap, CmapSubtable};
use read_fonts::tables::name::Name;
use read_fonts::types::{GlyphId, Tag};
use read_fonts::{FileRef, FontRef, TableProvider};

use crate::ffi::fontconfig::{
    CharSetBuilder, FC_CHARSET, FC_COLOR, FC_FAMILY, FC_FILE, FC_FONT_WRAPPER, FC_FONTFORMAT,
    FC_INDEX, FC_LANG, FC_ORDER, FC_SIZE, FC_SLANT, FC_SLANT_ITALIC, FC_SLANT_OBLIQUE,
    FC_SLANT_ROMAN, FC_SPACING, FC_STYLE, FC_VARIABLE, FC_WEIGHT, FC_WIDTH, Pattern,
    PatternBuilder, add_application_fonts, weight_from_opentype,
};

const FC_FAMILYLANG: &core::ffi::CStr = c"familylang";
const FC_STYLELANG: &core::ffi::CStr = c"stylelang";
const FC_FULLNAME: &core::ffi::CStr = c"fullname";
const FC_FULLNAMELANG: &core::ffi::CStr = c"fullnamelang";
const FC_POSTSCRIPT_NAME: &core::ffi::CStr = c"postscriptname";
const FC_FOUNDRY: &core::ffi::CStr = c"foundry";
const FC_FONTVERSION: &core::ffi::CStr = c"fontversion";
const FC_OUTLINE: &core::ffi::CStr = c"outline";
const FC_SCALABLE: &core::ffi::CStr = c"scalable";
const FC_FONT_HAS_HINT: &core::ffi::CStr = c"fonthashint";
const FC_DECORATIVE: &core::ffi::CStr = c"decorative";
const FC_SYMBOL: &core::ffi::CStr = c"symbol";
const FC_CAPABILITY: &core::ffi::CStr = c"capability";
const FC_NAMED_INSTANCE: &core::ffi::CStr = c"namedinstance";
const FC_PIXEL_SIZE: &core::ffi::CStr = c"pixelsize";
const FC_ANTIALIAS: &core::ffi::CStr = c"antialias";

const FC_WEIGHT_MEDIUM: f64 = 100.;
const FC_WEIGHT_BOLD: f64 = 200.;
const FC_WIDTH_NORMAL: f64 = 100.;
const FC_MONO: i32 = 100;
const FC_DUAL: i32 = 90;

const PLATFORM_ORDER: [u16; 4] = [3, 0, 1, 2];
const NAME_ORDER: [u16; 10] = [21, 16, 1, 18, 4, 22, 17, 2, 7, 8];

const WEIGHT_WORDS: [(&str, f64); 23] = [
    ("thin", 0.),
    ("extralight", 40.),
    ("ultralight", 40.),
    ("demilight", 55.),
    ("semilight", 55.),
    ("light", 50.),
    ("book", 75.),
    ("regular", 80.),
    ("normal", 80.),
    ("medium", 100.),
    ("demibold", 180.),
    ("demi", 180.),
    ("semibold", 180.),
    ("extrabold", 205.),
    ("superbold", 205.),
    ("ultrabold", 205.),
    ("bold", 200.),
    ("ultrablack", 215.),
    ("superblack", 215.),
    ("extrablack", 215.),
    ("<ultra", 205.),
    ("black", 210.),
    ("heavy", 210.),
];

const WIDTH_WORDS: [(&str, f64); 10] = [
    ("ultracondensed", 50.),
    ("extracondensed", 63.),
    ("semicondensed", 87.),
    ("condensed", 75.),
    ("normal", 100.),
    ("semiexpanded", 113.),
    ("extraexpanded", 150.),
    ("ultraexpanded", 200.),
    ("expanded", 125.),
    ("extended", 125.),
];

const SLANT_WORDS: [(&str, i32); 3] = [
    ("italic", FC_SLANT_ITALIC),
    ("kursiv", FC_SLANT_ITALIC),
    ("oblique", FC_SLANT_OBLIQUE),
];

const DECORATIVE_WORDS: [&str; 6] = ["shadow", "caps", "antiqua", "romansc", "embosed", "dunhill"];

#[derive(Clone, Copy, Debug, Default)]
pub struct Descriptors {
    pub weight: i32,
    pub slant: i32,
}

fn squash(text: &str) -> String {
    text.chars()
        .filter(|c| *c != ' ')
        .flat_map(char::to_lowercase)
        .collect()
}

fn contains_word(text: &str, word: &str) -> bool {
    let text = text.to_ascii_lowercase();
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|part| part == word)
}

fn style_constant<T: Copy>(style: &str, words: &[(&str, T)]) -> Option<T> {
    let squashed = squash(style);
    words.iter().find_map(|&(word, value)| {
        let found = match word.strip_prefix('<') {
            Some(whole) => contains_word(style, whole),
            None => squashed.contains(word),
        };
        found.then_some(value)
    })
}

#[derive(Default)]
struct Names {
    families: Vec<(String, String)>,
    styles: Vec<(String, String)>,
    full_names: Vec<(String, String)>,
}

fn name_language(platform: u16, language: u16) -> &'static str {
    languages::LANGUAGES
        .iter()
        .find(|&&(p, id, _)| p == platform && (id == 0xFFFF || id == language))
        .map(|&(_, _, tag)| tag)
        .filter(|tag| !tag.is_empty())
        .unwrap_or("und")
}

fn is_english(platform: u16, language: u16) -> bool {
    matches!((platform, language), (1, 0) | (3, 0x0409))
}

fn trim_name(text: &str) -> String {
    text.trim_start_matches(' ')
        .trim_end_matches([' ', '\r', '\n'])
        .to_owned()
}

fn push_unique(list: &mut Vec<(String, String)>, value: String, language: &str) {
    if !list
        .iter()
        .any(|(existing, _)| squash(existing) == squash(&value))
    {
        list.push((value, language.to_owned()));
    }
}

fn read_names(name: Option<&Name<'_>>, variable: bool, instance_style: Option<u16>) -> Names {
    let mut names = Names::default();
    let Some(name) = name else {
        return names;
    };
    let mut records: Vec<_> = name.name_record().iter().enumerate().collect();
    records.sort_by_key(|(index, record)| {
        (
            record.platform_id(),
            record.name_id().to_u16(),
            record.encoding_id(),
            !is_english(record.platform_id(), record.language_id()),
            record.language_id(),
            *index,
        )
    });
    let records: Vec<_> = records.into_iter().map(|(_, record)| record).collect();
    for platform in PLATFORM_ORDER {
        for name_id in NAME_ORDER {
            let mut lookup = name_id;
            if instance_style.is_some() {
                if matches!(name_id, 22 | 17 | 4) {
                    continue;
                }
                if name_id == 2 {
                    lookup = instance_style.unwrap_or(2);
                }
            }
            for record in records.iter().filter(|record| {
                record.platform_id() == platform && record.name_id().to_u16() == lookup
            }) {
                let Ok(raw) = record.string(name.string_data()) else {
                    continue;
                };
                let text: String = raw.chars().collect();
                let text = trim_name(&text);
                if text.is_empty() {
                    continue;
                }
                let language = name_language(record.platform_id(), record.language_id());
                match name_id {
                    21 | 16 | 1 => push_unique(&mut names.families, text, language),
                    18 | 4 if !variable => push_unique(&mut names.full_names, text, language),
                    22 | 17 | 2 if !variable => push_unique(&mut names.styles, text, language),
                    _ => {}
                }
            }
        }
    }
    names
}

fn first_english(list: &[(String, String)]) -> Option<&str> {
    list.iter()
        .find(|(_, language)| language == "en")
        .or_else(|| list.first())
        .map(|(value, _)| value.as_str())
}

struct CharMap {
    points: Vec<(u32, GlyphId)>,
    symbol: bool,
}

fn select_subtable<'a>(cmap: &Cmap<'a>) -> Option<(CmapSubtable<'a>, bool)> {
    const UNICODE: [(u16, u16); 8] = [
        (3, 10),
        (0, 6),
        (0, 4),
        (3, 1),
        (0, 3),
        (0, 2),
        (0, 1),
        (0, 0),
    ];
    let records = cmap.encoding_records();
    for (platform, encoding) in UNICODE {
        if let Some(subtable) = records
            .iter()
            .find(|record| {
                record.platform_id() as u16 == platform && record.encoding_id() == encoding
            })
            .and_then(|record| record.subtable(cmap.offset_data()).ok())
        {
            return Some((subtable, false));
        }
    }
    records
        .iter()
        .find(|record| record.platform_id() as u16 == 3 && record.encoding_id() == 0)
        .and_then(|record| record.subtable(cmap.offset_data()).ok())
        .map(|subtable| (subtable, true))
}

fn char_map(font: &FontRef<'_>) -> CharMap {
    let Some((subtable, symbol)) = font.cmap().ok().as_ref().and_then(select_subtable) else {
        return CharMap {
            points: Vec::new(),
            symbol: false,
        };
    };
    let mut points: Vec<(u32, GlyphId)> = subtable
        .iter()
        .filter(|(_, gid)| gid.to_u32() != 0)
        .collect();
    points.sort_by_key(|(ch, _)| *ch);
    points.dedup_by_key(|(ch, _)| *ch);
    CharMap { points, symbol }
}

fn glyph_has_contours(font: &FontRef<'_>, gid: GlyphId) -> bool {
    use skrifa::MetadataProvider;
    use skrifa::instance::{LocationRef, Size};
    use skrifa::outline::DrawSettings;
    if !has_outlines(font) {
        return false;
    }
    let Some(glyph) = font.outline_glyphs().get(gid) else {
        return false;
    };
    let mut outline = crate::rust_glyphs::Outline::default();
    glyph
        .draw(
            DrawSettings::unhinted(Size::unscaled(), LocationRef::default()),
            &mut outline,
        )
        .is_ok()
        && !outline.is_empty()
}

fn char_set(font: &FontRef<'_>, map: &CharMap) -> Option<CharSetBuilder> {
    let mut charset = CharSetBuilder::new()?;
    let mut has = std::collections::HashSet::new();
    for &(ch, gid) in &map.points {
        if ch <= 0x1F && !glyph_has_contours(font, gid) {
            continue;
        }
        charset.add(ch);
        has.insert(ch);
    }
    if map.symbol {
        for ch in 0xF000..0xF100 {
            if has.contains(&ch) {
                charset.add(ch - 0xF000);
            }
        }
    }
    Some(charset)
}

fn approximately_equal(x: i32, y: i32) -> bool {
    (x - y).abs() * 33 <= x.abs().max(y.abs())
}

fn spacing(font: &FontRef<'_>, map: &CharMap) -> Option<i32> {
    let hmtx = font.hmtx().ok()?;
    let mut advances: Vec<i32> = Vec::new();
    for &(_, gid) in &map.points {
        if advances.len() >= 3 {
            break;
        }
        let advance = i32::from(hmtx.advance(gid).unwrap_or(0));
        if advance != 0 && !advances.iter().any(|&a| approximately_equal(advance, a)) {
            advances.push(advance);
        }
    }
    match advances.as_slice() {
        [] | [_] => Some(FC_MONO),
        [a, b] if approximately_equal(a.min(b) * 2, *a.max(b)) => Some(FC_DUAL),
        _ => None,
    }
}

fn script_tags(font: &FontRef<'_>, tag: &[u8; 4]) -> Vec<u32> {
    let list = match tag {
        b"GSUB" => font.gsub().ok().and_then(|t| t.script_list().ok()),
        _ => font.gpos().ok().and_then(|t| t.script_list().ok()),
    };
    let mut tags: Vec<u32> = list
        .map(|list| {
            list.script_records()
                .iter()
                .map(|record| u32::from_be_bytes(record.script_tag().to_be_bytes()))
                .collect()
        })
        .unwrap_or_default();
    tags.sort_unstable();
    tags
}

fn capability(font: &FontRef<'_>) -> Option<String> {
    let graphite = font.table_data(Tag::new(b"Silf")).is_some();
    let gsub = script_tags(font, b"GSUB");
    let gpos = script_tags(font, b"GPOS");
    if !graphite && gsub.is_empty() && gpos.is_empty() {
        return None;
    }
    let mut merged: Vec<u32> = gsub.iter().chain(gpos.iter()).copied().collect();
    merged.sort_unstable();
    merged.dedup();
    let mut parts: Vec<String> = Vec::new();
    if graphite {
        parts.push("ttable:Silf".into());
    }
    for tag in merged {
        let bytes = tag.to_be_bytes();
        if bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b' ')
        {
            parts.push(format!("otlayout:{}", String::from_utf8_lossy(&bytes)));
        }
    }
    let joined = parts.join(" ");
    (!joined.is_empty() || graphite).then_some(joined)
}

fn has_hints(font: &FontRef<'_>) -> bool {
    font.table_data(Tag::new(b"fpgm")).is_some()
        || font.table_data(Tag::new(b"cvt ")).is_some()
        || font
            .table_data(Tag::new(b"prep"))
            .is_some_and(|prep| prep.len() > 7)
}

fn has_color(font: &FontRef<'_>) -> bool {
    let has = |tag: &[u8; 4]| font.table_data(Tag::new(tag)).is_some();
    (has(b"COLR") && has(b"CPAL")) || (has(b"CBDT") && has(b"CBLC")) || has(b"sbix") || has(b"SVG ")
}

fn has_outlines(font: &FontRef<'_>) -> bool {
    let has = |tag: &[u8; 4]| font.table_data(Tag::new(tag)).is_some();
    !has(b"sbix") && ((has(b"glyf") && has(b"loca")) || has(b"CFF ") || has(b"CFF2"))
}

fn exclusive_language(font: &FontRef<'_>) -> Option<&'static core::ffi::CStr> {
    let os2 = font.os2().ok()?;
    if os2.version() < 1 {
        return None;
    }
    let range = os2.ul_code_page_range_1()?;
    let mut exclusive = None;
    for (bit, language) in [(17, c"ja"), (18, c"zh-cn"), (19, c"ko"), (20, c"zh-tw")] {
        if range & (1 << bit) != 0 {
            if exclusive.is_some() {
                return None;
            }
            exclusive = Some(language);
        }
    }
    exclusive
}

fn width_class(class: u16) -> Option<f64> {
    Some(match class {
        1 => 50.,
        2 => 63.,
        3 => 75.,
        4 => 87.,
        5 => 100.,
        6 => 113.,
        7 => 125.,
        8 => 150.,
        9 => 200.,
        _ => return None,
    })
}

struct Face<'a> {
    font: FontRef<'a>,
    index: u32,
    path: &'a str,
    charset: CharSetBuilder,
    languages: Vec<String>,
    spacing: Option<i32>,
    capability: Option<String>,
    symbol: bool,
}

enum Instance {
    Default,
    Named(u32),
    Variable,
}

fn axis_ranges(font: &FontRef<'_>) -> Vec<(Tag, f64, f64, f64)> {
    let Ok(fvar) = font.fvar() else {
        return Vec::new();
    };
    let Ok(axes) = fvar.axes() else {
        return Vec::new();
    };
    axes.iter()
        .map(|axis| {
            (
                axis.axis_tag(),
                axis.min_value().to_f64(),
                axis.default_value().to_f64(),
                axis.max_value().to_f64(),
            )
        })
        .collect()
}

impl Face<'_> {
    fn pattern(&self, instance: &Instance) -> Option<Pattern> {
        let font = &self.font;
        let mut pattern = PatternBuilder::new();
        let outline = has_outlines(font);
        let color = has_color(font);
        pattern.add_bool(FC_OUTLINE, i32::from(outline));
        pattern.add_bool(FC_COLOR, i32::from(color));
        pattern.add_bool(FC_SCALABLE, i32::from(outline || color));
        let axes = axis_ranges(font);
        let mut variable = false;
        let (mut variable_weight, mut variable_width, mut variable_size) = (false, false, false);
        let (mut weight_mult, mut width_mult) = (1., 1.);
        let mut instance_style = None;
        let mut index = self.index;
        match instance {
            Instance::Variable => {
                for &(tag, min, default, max) in &axes {
                    if min > default || default > max || min == max {
                        continue;
                    }
                    match &tag.to_be_bytes() {
                        b"wght" => {
                            pattern.add_range(
                                FC_WEIGHT,
                                weight_from_opentype(min),
                                weight_from_opentype(max),
                            );
                            variable_weight = true;
                        }
                        b"wdth" => {
                            pattern.add_range(FC_WIDTH, min, max);
                            variable_width = true;
                        }
                        b"opsz" => {
                            pattern.add_range(FC_SIZE, min, max);
                            variable_size = true;
                        }
                        _ => continue,
                    }
                    variable = true;
                }
                if !variable {
                    return None;
                }
            }
            Instance::Named(number) => {
                let fvar = font.fvar().ok()?;
                let named = fvar.instances().ok()?.get(*number as usize).ok()?;
                instance_style = Some(named.subfamily_name_id.to_u16());
                for (&(tag, _, default, _), coord) in axes.iter().zip(named.coordinates.iter()) {
                    let value = coord.get().to_f64();
                    let mult = if default != 0. { value / default } else { 1. };
                    match &tag.to_be_bytes() {
                        b"wght" => weight_mult = mult,
                        b"wdth" => width_mult = mult,
                        b"opsz" => pattern.add_double(FC_SIZE, value),
                        _ => {}
                    }
                }
                index |= (number + 1) << 16;
            }
            Instance::Default => {
                for &(tag, _, default, _) in &axes {
                    if &tag.to_be_bytes() == b"opsz" {
                        pattern.add_double(FC_SIZE, default);
                        variable_size = true;
                    }
                }
            }
        }
        pattern.add_bool(FC_VARIABLE, i32::from(variable));
        let names = read_names(font.name().ok().as_ref(), variable, instance_style);
        let mut families = names.families.clone();
        if families.is_empty() {
            let stem = self.path.rsplit(['/', '\\']).next().unwrap_or(self.path);
            let stem = stem.rsplit_once('.').map_or(stem, |(stem, _)| stem);
            families.push((stem.to_owned(), "en".into()));
        }
        for (family, language) in &families {
            pattern.add_string(FC_FAMILY, family);
            pattern.add_string(FC_FAMILYLANG, language);
        }
        let mut styles = names.styles.clone();
        if !variable && styles.is_empty() {
            styles.push(("Regular".into(), "en".into()));
        }
        for (style, language) in &styles {
            pattern.add_string(FC_STYLE, style);
            pattern.add_string(FC_STYLELANG, language);
        }
        if !variable {
            let mut full_names = names.full_names.clone();
            if full_names.is_empty() {
                let family = first_english(&families).unwrap_or("").trim_end().to_owned();
                let style = first_english(&styles).unwrap_or("").trim_start().to_owned();
                full_names.push((format!("{family} {style}"), "en".into()));
            }
            for (full_name, language) in &full_names {
                pattern.add_string(FC_FULLNAME, full_name);
                pattern.add_string(FC_FULLNAMELANG, language);
            }
            pattern.add_string(
                FC_POSTSCRIPT_NAME,
                &self.postscript_name(&families, instance),
            );
        }
        pattern.add_string(FC_FILE, self.path);
        pattern.add_integer(FC_INDEX, index as i32);
        let version = font.head().map_or(0, |head| head.font_revision().to_bits());
        pattern.add_integer(FC_FONTVERSION, version);
        pattern.add_integer(FC_ORDER, 0);
        let os2 = font.os2().ok();
        let mut weight = os2
            .as_ref()
            .map(|os2| weight_from_opentype(f64::from(os2.us_weight_class()) * weight_mult));
        let mut width = os2
            .as_ref()
            .and_then(|os2| width_class(os2.us_width_class()))
            .map(|width| width * width_mult);
        if let Some(capability) = &self.capability
            && os2.is_some()
        {
            pattern.add_string(FC_CAPABILITY, capability);
        }
        pattern.add_bool(FC_FONT_HAS_HINT, i32::from(has_hints(font)));
        if !variable_size && let Some(os2) = os2.as_ref().filter(|os2| os2.version() >= 5) {
            let lower = f64::from(os2.us_lower_optical_point_size().unwrap_or(0)) / 20.;
            let upper = f64::from(os2.us_upper_optical_point_size().unwrap_or(0)) / 20.;
            if lower == upper {
                pattern.add_double(FC_SIZE, lower);
            } else {
                pattern.add_range(FC_SIZE, lower, upper);
            }
        }
        let mut slant = None;
        let mut decorative = false;
        for (style, _) in &styles {
            if weight.is_none() {
                weight = style_constant(style, &WEIGHT_WORDS);
            }
            if width.is_none() {
                width = style_constant(style, &WIDTH_WORDS);
            }
            if slant.is_none() {
                slant = style_constant(style, &SLANT_WORDS);
            }
            if !decorative {
                let squashed = squash(style);
                decorative = DECORATIVE_WORDS.iter().any(|word| squashed.contains(word));
            }
        }
        let (italic, bold) = style_flags(font);
        let slant = slant.unwrap_or(if italic {
            FC_SLANT_ITALIC
        } else {
            FC_SLANT_ROMAN
        });
        let weight = weight.unwrap_or(if bold {
            FC_WEIGHT_BOLD
        } else {
            FC_WEIGHT_MEDIUM
        });
        pattern.add_integer(FC_SLANT, slant);
        if !variable_weight {
            pattern.add_double(FC_WEIGHT, weight);
        }
        if !variable_width {
            pattern.add_double(FC_WIDTH, width.unwrap_or(FC_WIDTH_NORMAL));
        }
        pattern.add_string(FC_FOUNDRY, &foundry(font));
        pattern.add_bool(FC_DECORATIVE, i32::from(decorative));
        pattern.add_bool(FC_SYMBOL, i32::from(self.symbol));
        pattern.add_charset(FC_CHARSET, &self.charset);
        pattern.add_languages(FC_LANG, &self.languages);
        if let Some(spacing) = self.spacing {
            pattern.add_integer(FC_SPACING, spacing);
        }
        if !outline {
            for ppem in bitmap_sizes(font) {
                pattern.add_double(FC_PIXEL_SIZE, ppem);
            }
            pattern.add_bool(FC_ANTIALIAS, 0);
        }
        let format = if font.table_data(Tag::new(b"CFF ")).is_some()
            || font.table_data(Tag::new(b"CFF2")).is_some()
        {
            "CFF"
        } else {
            "TrueType"
        };
        pattern.add_string(FC_FONTFORMAT, format);
        pattern.add_bool(FC_NAMED_INSTANCE, i32::from(index > 0xFFFF));
        pattern.add_string(FC_FONT_WRAPPER, "SFNT");
        Some(pattern.finish())
    }

    fn postscript_name(&self, families: &[(String, String)], instance: &Instance) -> String {
        let font = &self.font;
        let name = font.name().ok();
        let from_table = match instance {
            Instance::Named(number) => name
                .as_ref()
                .and_then(|name| instance_postscript_name(font, name, *number)),
            _ => name.as_ref().and_then(|name| postscript_record(name, 6)),
        };
        match from_table {
            Some(name) => name.chars().take(255).collect(),
            None => first_english(families)
                .unwrap_or("")
                .chars()
                .take(255)
                .map(|c| {
                    if "\u{4}()/<>[]{}\t\u{c}\r\n ".contains(c) {
                        '-'
                    } else {
                        c
                    }
                })
                .collect(),
        }
    }
}

fn is_postscript_char(c: char) -> bool {
    ('!'..='~').contains(&c) && !"[](){}<>/%".contains(c)
}

fn record_text(
    name: &Name<'_>,
    platform: u16,
    encodings: &[u16],
    language: u16,
    id: u16,
) -> Option<String> {
    name.name_record()
        .iter()
        .filter(|record| {
            record.name_id().to_u16() == id
                && record.platform_id() == platform
                && encodings.contains(&record.encoding_id())
                && record.language_id() == language
                && record.length() > 0
        })
        .find_map(|record| record.string(name.string_data()).ok())
        .map(|text| text.chars().collect())
}

fn postscript_record(name: &Name<'_>, id: u16) -> Option<String> {
    let valid = |text: &String| !text.is_empty() && text.chars().all(is_postscript_char);
    record_text(name, 3, &[1, 0], 0x409, id)
        .filter(valid)
        .or_else(|| record_text(name, 1, &[0], 0, id).filter(valid))
}

fn english_name(name: &Name<'_>, id: u16) -> Option<String> {
    record_text(name, 3, &[1, 0, 10], 0x409, id).or_else(|| record_text(name, 1, &[0], 0, id))
}

fn instance_postscript_name(font: &FontRef<'_>, name: &Name<'_>, number: u32) -> Option<String> {
    let fvar = font.fvar().ok()?;
    let named = fvar.instances().ok()?.get(number as usize).ok()?;
    if let Some(text) = named
        .post_script_name_id
        .and_then(|id| postscript_record(name, id.to_u16()))
    {
        return Some(text);
    }
    let alphanumeric =
        |text: String| -> String { text.chars().filter(char::is_ascii_alphanumeric).collect() };
    let prefix = english_name(name, 25)
        .or_else(|| english_name(name, 16))
        .or_else(|| english_name(name, 1))
        .map(alphanumeric)?;
    let style = english_name(name, named.subfamily_name_id.to_u16()).map(alphanumeric)?;
    Some(format!("{prefix}-{style}"))
}

fn style_flags(font: &FontRef<'_>) -> (bool, bool) {
    match font.os2() {
        Ok(os2) => {
            let selection = os2.fs_selection().bits();
            (selection & 0x201 != 0, selection & 0x20 != 0)
        }
        Err(_) => font.head().map_or((false, false), |head| {
            let style = head.mac_style().bits();
            (style & 2 != 0, style & 1 != 0)
        }),
    }
}

fn foundry(font: &FontRef<'_>) -> String {
    font.os2()
        .ok()
        .filter(|os2| os2.ach_vend_id().to_be_bytes()[0] != 0)
        .map(|os2| {
            os2.ach_vend_id()
                .to_be_bytes()
                .iter()
                .take_while(|&&b| b != 0)
                .map(|&b| char::from(b))
                .collect()
        })
        .unwrap_or_else(|| "unknown".into())
}

fn bitmap_sizes(font: &FontRef<'_>) -> Vec<f64> {
    if let Ok(sbix) = font.sbix() {
        let strikes = sbix.strikes();
        return (0..strikes.len())
            .filter_map(|index| strikes.get(index).ok())
            .map(|strike| f64::from(strike.ppem()))
            .collect();
    }
    font.cblc()
        .map(|cblc| {
            cblc.bitmap_sizes()
                .iter()
                .map(|size| f64::from(size.ppem_y()))
                .collect()
        })
        .unwrap_or_default()
}

fn face_patterns(font: FontRef<'_>, index: u32, path: &str) -> Vec<Pattern> {
    let map = char_map(&font);
    let Some(charset) = char_set(&font, &map) else {
        return Vec::new();
    };
    let languages = if map.symbol {
        Vec::new()
    } else {
        charset.languages(exclusive_language(&font))
    };
    let face = Face {
        spacing: spacing(&font, &map),
        capability: capability(&font),
        symbol: map.symbol,
        font,
        index,
        path,
        charset,
        languages,
    };
    let mut patterns: Vec<Pattern> = face.pattern(&Instance::Default).into_iter().collect();
    let axes = axis_ranges(&face.font);
    if let Ok(instances) = face.font.fvar().and_then(|fvar| fvar.instances()) {
        for number in 0..instances.len() as u32 {
            let Ok(named) = instances.get(number as usize) else {
                continue;
            };
            let at_default = axes
                .iter()
                .zip(named.coordinates.iter())
                .all(|(&(_, _, default, _), coord)| coord.get().to_f64() == default);
            if at_default {
                continue;
            }
            patterns.extend(face.pattern(&Instance::Named(number)));
        }
        patterns.extend(face.pattern(&Instance::Variable));
    }
    patterns
}

pub(crate) fn patterns_for(path: &str, data: &[u8]) -> Vec<Pattern> {
    let Ok(file) = FileRef::new(data) else {
        return Vec::new();
    };
    file.fonts()
        .enumerate()
        .filter_map(|(index, font)| Some((index as u32, font.ok()?)))
        .flat_map(|(index, font)| face_patterns(font, index, path))
        .collect()
}

fn family_of(pattern: &Pattern) -> Option<String> {
    pattern.get_string(FC_FAMILY, 0)
}

fn apply_descriptors(pattern: &Pattern, descriptors: Descriptors) -> Pattern {
    let mut builder = pattern.duplicate();
    let variable = pattern.get_bool(FC_VARIABLE, 0) == Some(true);
    if descriptors.weight > 0 && !variable {
        builder.remove(FC_WEIGHT, 0);
        builder.add_double(
            FC_WEIGHT,
            weight_from_opentype(f64::from(descriptors.weight)),
        );
    }
    let slant = match descriptors.slant {
        0 => None,
        2 => Some(FC_SLANT_ITALIC),
        3 => Some(FC_SLANT_OBLIQUE),
        _ => Some(FC_SLANT_ROMAN),
    };
    if let Some(slant) = slant {
        builder.remove(FC_SLANT, 0);
        builder.add_integer(FC_SLANT, slant);
    }
    builder.add_bool(crate::engine::WEB_FONT_OBJECT, 1);
    builder.finish()
}

fn renamed(pattern: &Pattern, css_family: &str) -> Pattern {
    let mut builder = pattern.duplicate();
    builder.delete(FC_FAMILY);
    builder.delete(FC_FAMILYLANG);
    builder.add_string(FC_FAMILY, css_family);
    builder.finish()
}

pub fn install_web_font(
    path: &str,
    data: &[u8],
    css_family: &str,
    descriptors: Descriptors,
) -> bool {
    let faces = patterns_for(path, data);
    if faces.is_empty() {
        return false;
    }
    let mut installed = Vec::new();
    for face in &faces {
        let same_family =
            family_of(face).is_some_and(|family| family.eq_ignore_ascii_case(css_family));
        if same_family {
            installed.push(apply_descriptors(face, descriptors));
        } else {
            installed.push(apply_descriptors(face, Descriptors::default()));
            installed.push(apply_descriptors(&renamed(face, css_family), descriptors));
        }
    }
    add_application_fonts(installed)
}
