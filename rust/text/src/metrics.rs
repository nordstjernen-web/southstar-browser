//! Southstar text — font metrics: ascent, descent, height, average widths and decoration line positions in Pango units.
//! Copyright 1999 Red Hat Software; 2026 Andreas Røsdal
//! SPDX-License-Identifier: LGPL-2.0-or-later

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FontMetrics {
    pub ascent: i32,
    pub descent: i32,
    pub height: i32,
    pub approximate_char_width: i32,
    pub approximate_digit_width: i32,
    pub underline_position: i32,
    pub underline_thickness: i32,
    pub strikethrough_position: i32,
    pub strikethrough_thickness: i32,
}

impl FontMetrics {
    pub fn new() -> FontMetrics {
        FontMetrics::default()
    }

    pub fn ascent(&self) -> i32 {
        self.ascent
    }

    pub fn descent(&self) -> i32 {
        self.descent
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn approximate_char_width(&self) -> i32 {
        self.approximate_char_width
    }

    pub fn approximate_digit_width(&self) -> i32 {
        self.approximate_digit_width
    }

    pub fn underline_position(&self) -> i32 {
        self.underline_position
    }

    pub fn underline_thickness(&self) -> i32 {
        self.underline_thickness
    }

    pub fn strikethrough_position(&self) -> i32 {
        self.strikethrough_position
    }

    pub fn strikethrough_thickness(&self) -> i32 {
        self.strikethrough_thickness
    }
}
