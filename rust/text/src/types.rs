//! Southstar text — the units, rectangles, enumerations and per-character break flags every text module shares.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

pub const SCALE: i32 = 1024;
pub const SCALE_F: f64 = 1024.0;

pub type Glyph = u32;
pub type GlyphUnit = i32;

pub const GLYPH_EMPTY: Glyph = 0x0FFF_FFFF;
pub const GLYPH_INVALID_INPUT: Glyph = 0xFFFF_FFFF;
pub const GLYPH_UNKNOWN_FLAG: Glyph = 0x1000_0000;

pub const fn get_unknown_glyph(wc: u32) -> Glyph {
    wc | GLYPH_UNKNOWN_FLAG
}

pub const fn pixels(d: i32) -> i32 {
    (d + 512) >> 10
}

pub const fn pixels_floor(d: i32) -> i32 {
    d >> 10
}

pub const fn pixels_ceil(d: i32) -> i32 {
    (d + 1023) >> 10
}

pub const fn units_floor(d: i32) -> i32 {
    d & !(SCALE - 1)
}

pub const fn units_ceil(d: i32) -> i32 {
    (d + (SCALE - 1)) & !(SCALE - 1)
}

pub const fn units_round(d: i32) -> i32 {
    (d + (SCALE >> 1)) & !(SCALE - 1)
}

pub fn units_from_double(d: f64) -> i32 {
    (d * SCALE_F + 0.5).floor() as i32
}

pub fn units_to_double(i: i32) -> f64 {
    f64::from(i) / SCALE_F
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Hash)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rectangle {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Rectangle {
        Rectangle {
            x,
            y,
            width,
            height,
        }
    }

    pub const fn ascent(&self) -> i32 {
        -self.y
    }

    pub const fn descent(&self) -> i32 {
        self.y + self.height
    }

    pub const fn lbearing(&self) -> i32 {
        self.x
    }

    pub const fn rbearing(&self) -> i32 {
        self.x + self.width
    }
}

pub fn extents_to_pixels(inclusive: Option<&mut Rectangle>, nearest: Option<&mut Rectangle>) {
    if let Some(r) = inclusive {
        let orig_x = r.x;
        let orig_y = r.y;
        r.x = pixels_floor(r.x);
        r.y = pixels_floor(r.y);
        r.width = pixels_ceil(orig_x + r.width) - r.x;
        r.height = pixels_ceil(orig_y + r.height) - r.y;
    }
    if let Some(r) = nearest {
        let orig_x = r.x;
        let orig_y = r.y;
        r.x = pixels(r.x);
        r.y = pixels(r.y);
        r.width = pixels(orig_x + r.width) - r.x;
        r.height = pixels(orig_y + r.height) - r.y;
    }
}

macro_rules! c_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $value:expr),+ $(,)? }) => {
        $(#[$m])*
        #[repr(i32)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name {
            $($variant = $value),+
        }

        impl $name {
            pub fn from_i32(value: i32) -> Option<$name> {
                match value {
                    $(v if v == $value => Some($name::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

c_enum!(Direction {
    Ltr = 0,
    Rtl = 1,
    TtbLtr = 2,
    TtbRtl = 3,
    WeakLtr = 4,
    WeakRtl = 5,
    Neutral = 6,
});

c_enum!(Gravity {
    South = 0,
    East = 1,
    North = 2,
    West = 3,
    Auto = 4,
});

impl Gravity {
    pub fn is_vertical(self) -> bool {
        matches!(self, Gravity::East | Gravity::West)
    }

    pub fn is_improper(self) -> bool {
        matches!(self, Gravity::West | Gravity::North)
    }
}

c_enum!(GravityHint {
    Natural = 0,
    Strong = 1,
    Line = 2,
});

c_enum!(Style {
    Normal = 0,
    Oblique = 1,
    Italic = 2,
});

c_enum!(Variant {
    Normal = 0,
    SmallCaps = 1,
    AllSmallCaps = 2,
    PetiteCaps = 3,
    AllPetiteCaps = 4,
    Unicase = 5,
    TitleCaps = 6,
});

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Weight(pub i32);

impl Weight {
    pub const THIN: Weight = Weight(100);
    pub const ULTRALIGHT: Weight = Weight(200);
    pub const LIGHT: Weight = Weight(300);
    pub const SEMILIGHT: Weight = Weight(350);
    pub const BOOK: Weight = Weight(380);
    pub const NORMAL: Weight = Weight(400);
    pub const MEDIUM: Weight = Weight(500);
    pub const SEMIBOLD: Weight = Weight(600);
    pub const BOLD: Weight = Weight(700);
    pub const ULTRABOLD: Weight = Weight(800);
    pub const HEAVY: Weight = Weight(900);
    pub const ULTRAHEAVY: Weight = Weight(1000);
}

c_enum!(Stretch {
    UltraCondensed = 0,
    ExtraCondensed = 1,
    Condensed = 2,
    SemiCondensed = 3,
    Normal = 4,
    SemiExpanded = 5,
    Expanded = 6,
    ExtraExpanded = 7,
    UltraExpanded = 8,
});

pub mod font_mask {
    pub const FAMILY: u32 = 1 << 0;
    pub const STYLE: u32 = 1 << 1;
    pub const VARIANT: u32 = 1 << 2;
    pub const WEIGHT: u32 = 1 << 3;
    pub const STRETCH: u32 = 1 << 4;
    pub const SIZE: u32 = 1 << 5;
    pub const GRAVITY: u32 = 1 << 6;
    pub const VARIATIONS: u32 = 1 << 7;
    pub const FEATURES: u32 = 1 << 8;
    pub const COLOR: u32 = 1 << 9;
}

c_enum!(FontColor {
    Forbidden = 0,
    Required = 1,
    DontCare = 2,
});

c_enum!(Alignment {
    Left = 0,
    Center = 1,
    Right = 2,
});

c_enum!(WrapMode {
    Word = 0,
    Char = 1,
    WordChar = 2,
});

c_enum!(EllipsizeMode {
    None = 0,
    Start = 1,
    Middle = 2,
    End = 3,
});

c_enum!(Underline {
    None = 0,
    Single = 1,
    Double = 2,
    Low = 3,
    Error = 4,
    SingleLine = 5,
    DoubleLine = 6,
    ErrorLine = 7,
});

c_enum!(Overline {
    None = 0,
    Single = 1,
});

c_enum!(TextTransform {
    None = 0,
    Lowercase = 1,
    Uppercase = 2,
    Capitalize = 3,
});

c_enum!(BaselineShift {
    None = 0,
    Superscript = 1,
    Subscript = 2,
});

c_enum!(FontScale {
    None = 0,
    Superscript = 1,
    Subscript = 2,
    SmallCaps = 3,
});

c_enum!(TabAlign {
    Left = 0,
    Right = 1,
    Center = 2,
    Decimal = 3,
});

c_enum!(CoverageLevel {
    None = 0,
    Fallback = 1,
    Approximate = 2,
    Exact = 3,
});

c_enum!(RenderPart {
    Foreground = 0,
    Background = 1,
    Underline = 2,
    Strikethrough = 3,
    Overline = 4,
});

pub mod show_flags {
    pub const NONE: u32 = 0;
    pub const SPACES: u32 = 1 << 0;
    pub const LINE_BREAKS: u32 = 1 << 1;
    pub const IGNORABLES: u32 = 1 << 2;
}

pub mod shape_flags {
    pub const NONE: u32 = 0;
    pub const ROUND_POSITIONS: u32 = 1 << 0;
}

macro_rules! log_attr_bits {
    ($($get:ident, $set:ident = $bit:expr;)+) => {
        impl LogAttr {
            $(
                pub const fn $get(self) -> bool {
                    self.0 & (1 << $bit) != 0
                }

                pub fn $set(&mut self, value: bool) {
                    if value {
                        self.0 |= 1 << $bit;
                    } else {
                        self.0 &= !(1 << $bit);
                    }
                }
            )+
        }
    };
}

#[repr(transparent)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct LogAttr(pub u32);

log_attr_bits! {
    is_line_break, set_is_line_break = 0;
    is_mandatory_break, set_is_mandatory_break = 1;
    is_char_break, set_is_char_break = 2;
    is_white, set_is_white = 3;
    is_cursor_position, set_is_cursor_position = 4;
    is_word_start, set_is_word_start = 5;
    is_word_end, set_is_word_end = 6;
    is_sentence_boundary, set_is_sentence_boundary = 7;
    is_sentence_start, set_is_sentence_start = 8;
    is_sentence_end, set_is_sentence_end = 9;
    backspace_deletes_character, set_backspace_deletes_character = 10;
    is_expandable_space, set_is_expandable_space = 11;
    is_word_boundary, set_is_word_boundary = 12;
    break_inserts_hyphen, set_break_inserts_hyphen = 13;
    break_removes_preceding, set_break_removes_preceding = 14;
}

impl core::fmt::Debug for LogAttr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "LogAttr({:#06x})", self.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub red: u16,
    pub green: u16,
    pub blue: u16,
}
