//! Southstar bidi — the Unicode Bidirectional Algorithm (UAX #9): character properties and a paragraph's embedding levels.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod paragraph;
mod tables;

#[cfg(test)]
mod tests;

pub use paragraph::{ParagraphDirection, Resolved, resolve_paragraph};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BidiClass {
    L,
    R,
    AL,
    EN,
    ES,
    ET,
    AN,
    CS,
    NSM,
    BN,
    B,
    S,
    WS,
    ON,
    LRE,
    LRO,
    RLE,
    RLO,
    PDF,
    LRI,
    RLI,
    FSI,
    PDI,
}

const CLASS_BY_INDEX: [BidiClass; 23] = [
    BidiClass::L,
    BidiClass::R,
    BidiClass::AL,
    BidiClass::EN,
    BidiClass::ES,
    BidiClass::ET,
    BidiClass::AN,
    BidiClass::CS,
    BidiClass::NSM,
    BidiClass::BN,
    BidiClass::B,
    BidiClass::S,
    BidiClass::WS,
    BidiClass::ON,
    BidiClass::LRE,
    BidiClass::LRO,
    BidiClass::RLE,
    BidiClass::RLO,
    BidiClass::PDF,
    BidiClass::LRI,
    BidiClass::RLI,
    BidiClass::FSI,
    BidiClass::PDI,
];

impl BidiClass {
    pub fn is_strong(self) -> bool {
        matches!(self, BidiClass::L | BidiClass::R | BidiClass::AL)
    }

    pub fn is_rtl(self) -> bool {
        matches!(self, BidiClass::R | BidiClass::AL)
    }

    pub fn is_isolate_initiator(self) -> bool {
        matches!(self, BidiClass::LRI | BidiClass::RLI | BidiClass::FSI)
    }

    pub fn is_isolate_control(self) -> bool {
        self.is_isolate_initiator() || self == BidiClass::PDI
    }

    pub fn is_embedding_or_override(self) -> bool {
        matches!(
            self,
            BidiClass::LRE | BidiClass::RLE | BidiClass::LRO | BidiClass::RLO
        )
    }

    pub fn is_removed_by_x9(self) -> bool {
        self.is_embedding_or_override() || matches!(self, BidiClass::PDF | BidiClass::BN)
    }

    pub fn is_number(self) -> bool {
        matches!(self, BidiClass::EN | BidiClass::AN)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bracket {
    pub pair: u32,
    pub opening: bool,
}

pub fn class(ch: u32) -> BidiClass {
    let index = tables::CLASS_STARTS.partition_point(|&start| start <= ch);
    let value = tables::CLASS_VALUES[index.saturating_sub(1)];
    CLASS_BY_INDEX[usize::from(value)]
}

pub fn bracket(ch: u32) -> Option<Bracket> {
    let index = tables::BRACKET_KEYS.binary_search(&ch).ok()?;
    let packed = tables::BRACKET_PAIRS[index];
    Some(Bracket {
        pair: packed & 0x7fff_ffff,
        opening: packed & 0x8000_0000 != 0,
    })
}

pub fn mirror(ch: u32) -> Option<u32> {
    let index = tables::MIRROR_KEYS.binary_search(&ch).ok()?;
    Some(tables::MIRROR_VALUES[index])
}
