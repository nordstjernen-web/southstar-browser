//! Southstar — Cairo's plain values: its integer constants as enums, status errors, matrices, rectangles, text extents and path segments.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_int;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Format {
    Argb32 = 0,
    Rgb24 = 1,
    A8 = 2,
    A1 = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Content {
    Color = 0x1000,
    Alpha = 0x2000,
    ColorAlpha = 0x3000,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Operator {
    Clear = 0,
    Source = 1,
    Over = 2,
    In = 3,
    Out = 4,
    Atop = 5,
    Dest = 6,
    DestOver = 7,
    DestIn = 8,
    DestOut = 9,
    DestAtop = 10,
    Xor = 11,
    Add = 12,
    Saturate = 13,
    Multiply = 14,
    Screen = 15,
    Overlay = 16,
    Darken = 17,
    Lighten = 18,
    ColorDodge = 19,
    ColorBurn = 20,
    HardLight = 21,
    SoftLight = 22,
    Difference = 23,
    Exclusion = 24,
    HslHue = 25,
    HslSaturation = 26,
    HslColor = 27,
    HslLuminosity = 28,
}

const OPERATORS: [Operator; 29] = [
    Operator::Clear,
    Operator::Source,
    Operator::Over,
    Operator::In,
    Operator::Out,
    Operator::Atop,
    Operator::Dest,
    Operator::DestOver,
    Operator::DestIn,
    Operator::DestOut,
    Operator::DestAtop,
    Operator::Xor,
    Operator::Add,
    Operator::Saturate,
    Operator::Multiply,
    Operator::Screen,
    Operator::Overlay,
    Operator::Darken,
    Operator::Lighten,
    Operator::ColorDodge,
    Operator::ColorBurn,
    Operator::HardLight,
    Operator::SoftLight,
    Operator::Difference,
    Operator::Exclusion,
    Operator::HslHue,
    Operator::HslSaturation,
    Operator::HslColor,
    Operator::HslLuminosity,
];

impl Operator {
    pub(crate) fn from_raw(op: c_int) -> Operator {
        usize::try_from(op)
            .ok()
            .and_then(|i| OPERATORS.get(i).copied())
            .unwrap_or(Operator::Over)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum FillRule {
    Winding = 0,
    EvenOdd = 1,
}

impl FillRule {
    pub(crate) fn from_raw(rule: c_int) -> FillRule {
        if rule == FillRule::EvenOdd as c_int {
            FillRule::EvenOdd
        } else {
            FillRule::Winding
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum LineCap {
    Butt = 0,
    Round = 1,
    Square = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum LineJoin {
    Miter = 0,
    Round = 1,
    Bevel = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Extend {
    None = 0,
    Repeat = 1,
    Reflect = 2,
    Pad = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Filter {
    Fast = 0,
    Good = 1,
    Best = 2,
    Nearest = 3,
    Bilinear = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Antialias {
    Default = 0,
    None = 1,
    Gray = 2,
    Subpixel = 3,
    Fast = 4,
    Good = 5,
    Best = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum SubpixelOrder {
    Default = 0,
    Rgb = 1,
    Bgr = 2,
    Vrgb = 3,
    Vbgr = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum HintStyle {
    Default = 0,
    None = 1,
    Slight = 2,
    Medium = 3,
    Full = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum HintMetrics {
    Default = 0,
    Off = 1,
    On = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum PdfMetadata {
    Title = 0,
    Author = 1,
    Subject = 2,
    Keywords = 3,
    Creator = 4,
    CreateDate = 5,
    ModDate = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Error(pub(crate) c_int);

pub(crate) const STATUS_SUCCESS: c_int = 0;

impl Error {
    pub(crate) fn check(status: c_int) -> Result<(), Error> {
        if status == STATUS_SUCCESS {
            Ok(())
        } else {
            Err(Error(status))
        }
    }

    pub fn code(self) -> i32 {
        self.0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Matrix {
    pub xx: f64,
    pub yx: f64,
    pub xy: f64,
    pub yy: f64,
    pub x0: f64,
    pub y0: f64,
}

impl Matrix {
    pub const fn new(xx: f64, yx: f64, xy: f64, yy: f64, x0: f64, y0: f64) -> Matrix {
        Matrix {
            xx,
            yx,
            xy,
            yy,
            x0,
            y0,
        }
    }

    pub const fn identity() -> Matrix {
        Matrix::new(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
    }

    pub const fn translation(tx: f64, ty: f64) -> Matrix {
        Matrix::new(1.0, 0.0, 0.0, 1.0, tx, ty)
    }

    pub const fn scaling(sx: f64, sy: f64) -> Matrix {
        Matrix::new(sx, 0.0, 0.0, sy, 0.0, 0.0)
    }

    pub const fn to_array(self) -> [f64; 6] {
        [self.xx, self.yx, self.xy, self.yy, self.x0, self.y0]
    }
}

impl From<[f64; 6]> for Matrix {
    fn from([xx, yx, xy, yy, x0, y0]: [f64; 6]) -> Matrix {
        Matrix::new(xx, yx, xy, yy, x0, y0)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rectangle {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextExtents {
    pub x_bearing: f64,
    pub y_bearing: f64,
    pub width: f64,
    pub height: f64,
    pub x_advance: f64,
    pub y_advance: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegment {
    MoveTo(f64, f64),
    LineTo(f64, f64),
    CurveTo([f64; 6]),
    ClosePath,
}
