//! Southstar text — items: runs of text with one font, direction, script and language, and the attributes that ride along with them.
//! Copyright 2000 Red Hat Software, 2021 Matthias Clasen; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

use crate::attrs::{AttrIterator, AttrValue, Attribute};
use crate::ffi::harfbuzz::{self, Feature, tag};
use crate::font::Font;
use crate::language::Language;
use crate::script::Script;
use crate::types::Gravity;

pub mod analysis_flags {
    pub const CENTERED_BASELINE: u8 = 1 << 0;
    pub const IS_ELLIPSIS: u8 = 1 << 1;
    pub const NEED_HYPHEN: u8 = 1 << 2;
    pub const HAS_CHAR_OFFSET: u8 = 1 << 7;
}

#[derive(Clone, Debug)]
pub struct Analysis {
    pub font: Option<Font>,
    pub size_font: Option<Font>,
    pub strut_font: Option<Font>,
    pub level: u8,
    pub gravity: Gravity,
    pub flags: u8,
    pub script: Script,
    pub language: Option<Language>,
    pub extra_attrs: Vec<Attribute>,
}

impl Default for Analysis {
    fn default() -> Analysis {
        Analysis {
            font: None,
            size_font: None,
            strut_font: None,
            level: 0,
            gravity: Gravity::South,
            flags: 0,
            script: Script::COMMON,
            language: None,
            extra_attrs: Vec::new(),
        }
    }
}

const LETTER_SPACING_DISABLED_FEATURES: [u32; 4] =
    [tag(b"liga"), tag(b"clig"), tag(b"dlig"), tag(b"hlig")];

impl Analysis {
    pub fn collect_features(&self, features: &mut Vec<Feature>, length: usize) {
        if let Some(font) = self.font.as_ref() {
            font.features(features, length);
        }

        for attr in &self.extra_attrs {
            if features.len() >= length {
                break;
            }
            let AttrValue::FontFeatures(list) = &attr.value else {
                continue;
            };
            let bytes = list.as_bytes();
            let bytes = &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())];
            let mut rest = bytes;
            while features.len() < length {
                let comma = rest.iter().position(|&b| b == b',');
                let current = comma.map_or(rest, |end| &rest[..end]);
                if let Some(mut feature) = harfbuzz::feature_from_string(current) {
                    feature.start = attr.start_index;
                    feature.end = attr.end_index;
                    features.push(feature);
                }
                let Some(end) = comma else {
                    break;
                };
                rest = &rest[end + 1..];
            }
        }

        for attr in &self.extra_attrs {
            if features.len() >= length {
                break;
            }
            if !matches!(attr.value, AttrValue::LetterSpacing(_)) {
                continue;
            }
            for feature_tag in LETTER_SPACING_DISABLED_FEATURES {
                features.push(Feature {
                    tag: feature_tag,
                    value: 0,
                    start: attr.start_index,
                    end: attr.end_index,
                });
            }
        }
    }

    pub fn size_font(&self) -> Option<&Font> {
        self.size_font.as_ref()
    }

    pub fn set_size_font(&mut self, font: Option<Font>) {
        self.size_font = font;
    }

    pub fn strut_font(&self) -> Option<&Font> {
        self.strut_font.as_ref()
    }

    pub fn set_strut_font(&mut self, font: Option<Font>) {
        self.strut_font = font;
    }
}

#[derive(Clone, Debug)]
pub struct Item {
    pub offset: i32,
    pub length: i32,
    pub num_chars: i32,
    pub char_offset: i32,
    pub analysis: Analysis,
}

impl Default for Item {
    fn default() -> Item {
        Item::new()
    }
}

fn same_attribute(a: &Attribute, b: &Attribute) -> bool {
    a.equal(b) && a.start_index == b.start_index && a.end_index == b.end_index
}

impl Item {
    pub fn new() -> Item {
        Item {
            offset: 0,
            length: 0,
            num_chars: 0,
            char_offset: 0,
            analysis: Analysis {
                flags: analysis_flags::HAS_CHAR_OFFSET,
                ..Analysis::default()
            },
        }
    }

    fn has_char_offset(&self) -> bool {
        self.analysis.flags & analysis_flags::HAS_CHAR_OFFSET != 0
    }

    pub fn char_offset(&self) -> i32 {
        self.char_offset
    }

    pub fn split(&mut self, split_index: i32, split_offset: i32) -> Option<Item> {
        if split_index <= 0
            || split_index >= self.length
            || split_offset <= 0
            || split_offset >= self.num_chars
        {
            return None;
        }

        let mut new_item = self.clone();
        new_item.length = split_index;
        new_item.num_chars = split_offset;
        if !self.has_char_offset() {
            new_item.char_offset = 0;
        }

        self.offset += split_index;
        self.length -= split_index;
        self.num_chars -= split_offset;
        if self.has_char_offset() {
            self.char_offset += split_offset;
        }

        Some(new_item)
    }

    pub fn unsplit(&mut self, split_index: i32, split_offset: i32) {
        self.offset -= split_index;
        self.length += split_index;
        self.num_chars += split_offset;
        if self.has_char_offset() {
            self.char_offset -= split_offset;
        }
    }

    pub fn apply_attrs(&mut self, iter: &mut AttrIterator<'_>) {
        let mut attrs: Vec<Attribute> = Vec::new();
        let item_end = self.offset + self.length;

        loop {
            let (start, end) = iter.range();

            if start >= item_end {
                break;
            }

            if end >= self.offset {
                for attr in iter.get_attrs() {
                    if !attrs.iter().any(|seen| same_attribute(seen, &attr)) {
                        attrs.push(attr);
                    }
                }
            }

            if end >= item_end {
                break;
            }

            if !iter.next() {
                break;
            }
        }

        attrs.reverse();
        self.analysis.extra_attrs.extend(attrs);
    }
}
