//! Southstar — the small helpers painting shares: C's MIN and MAX, ASCII spaces, colours from computed values and the style lookups every section makes.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::CStr;

use southstar_layout::BoxRef;
use southstar_style::{PropId, StyleRef, ValueRef};

use crate::ffi::cairo::Cr;

pub const UNIT_PX: u32 = 0;
pub const UNIT_EM: u32 = 1;
pub const UNIT_REM: u32 = 2;
pub const UNIT_PERCENT: u32 = 3;
pub const UNIT_NUMBER: u32 = 4;
pub const UNIT_VW: u32 = 5;
pub const UNIT_VH: u32 = 6;
pub const UNIT_VMIN: u32 = 7;
pub const UNIT_VMAX: u32 = 8;
pub const UNIT_CQW: u32 = 9;
pub const UNIT_CQH: u32 = 10;
pub const UNIT_CQMIN: u32 = 11;
pub const UNIT_CQMAX: u32 = 12;
pub const UNIT_EX: u32 = 13;
pub const UNIT_CH: u32 = 14;
pub const UNIT_CAP: u32 = 15;
pub const UNIT_IC: u32 = 16;
pub const UNIT_LH: u32 = 17;
pub const UNIT_RLH: u32 = 18;
pub const UNIT_REX: u32 = 19;
pub const UNIT_RCH: u32 = 20;
pub const UNIT_RCAP: u32 = 21;
pub const UNIT_RIC: u32 = 22;

pub fn cmin(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

pub fn cmax(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

pub fn is_space(c: u8) -> bool {
    c == b' ' || (b'\t'..=b'\r').contains(&c)
}

pub fn skip_spaces(text: &[u8], mut pos: usize) -> usize {
    while pos < text.len() && is_space(text[pos]) {
        pos += 1;
    }
    pos
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgba {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

impl Rgba {
    pub const fn new(r: f64, g: f64, b: f64, a: f64) -> Rgba {
        Rgba { r, g, b, a }
    }

    pub fn from_bytes(c: [u8; 4]) -> Rgba {
        Rgba {
            r: f64::from(c[0]) / 255.0,
            g: f64::from(c[1]) / 255.0,
            b: f64::from(c[2]) / 255.0,
            a: f64::from(c[3]) / 255.0,
        }
    }

    pub fn set_source(self, cr: Cr) {
        cr.set_source_rgba(self.r, self.g, self.b, self.a);
    }
}

pub fn rgba_of(v: Option<ValueRef<'_>>, fallback: Rgba) -> Rgba {
    v.and_then(ValueRef::color)
        .map_or(fallback, Rgba::from_bytes)
}

pub fn length_or(v: Option<ValueRef<'_>>, fallback: f64) -> f64 {
    v.map_or(fallback, |v| v.length_or(fallback))
}

pub fn keyword_is(v: Option<ValueRef<'_>>, kw: &CStr) -> bool {
    v.is_some_and(|v| v.is_keyword(kw))
}

pub fn keyword(v: Option<ValueRef<'_>>) -> Option<&CStr> {
    v.and_then(ValueRef::keyword_text)
}

pub fn style_of(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    unsafe { StyleRef::from_ptr(b.style()) }
}

pub fn get(s: Option<StyleRef<'_>>, prop: PropId) -> Option<ValueRef<'_>> {
    s.and_then(|s| s.get(prop))
}

pub fn style_keyword(s: Option<StyleRef<'_>>, prop: PropId) -> Option<&CStr> {
    s.and_then(|s| s.keyword_of(prop))
}

pub fn inherited_style(b: BoxRef<'_>) -> Option<StyleRef<'_>> {
    let mut p = b.parent();
    while let Some(parent) = p {
        if let Some(s) = style_of(parent) {
            return Some(s);
        }
        p = parent.parent();
    }
    None
}

pub fn truncate(mut bytes: Vec<u8>, size: usize) -> Vec<u8> {
    bytes.truncate(size.saturating_sub(1));
    bytes
}
