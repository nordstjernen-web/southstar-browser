//! Southstar — CSS colour values for the style engine, parsed and resolved by southstar-css-color.
//!
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CStr;

use southstar_css_color::{Color, Context, Specified};

use crate::ffi;

pub(crate) type Channels = [Option<u8>; 4];

fn context(current: Option<Color>) -> Context {
    Context {
        current,
        dark: ffi::prefers_dark(),
    }
}

pub(crate) fn parse_value(text: &[u8]) -> Option<Color> {
    southstar_css_color::computed(text, &context(None))
}

pub(crate) fn parse_value_or_current(text: &[u8]) -> Option<Option<Color>> {
    let value = southstar_css_color::parse(text)?;
    if value == Specified::CurrentColor {
        return None;
    }
    if value.uses_current_color() || value.uses_color_scheme() {
        return Some(None);
    }
    value.resolve(&context(None)).map(Some)
}

pub(crate) fn parse_in_scheme(text: &[u8], current: Option<Color>, dark: bool) -> Option<Color> {
    southstar_css_color::computed(text, &Context { current, dark })
}

pub(crate) fn needs_current_color(text: &[u8]) -> bool {
    southstar_css_color::parse(text).is_some_and(|value| {
        value != Specified::CurrentColor
            && ((text.contains(&b'(') && value.uses_current_color()) || value.uses_color_scheme())
    })
}

pub fn scheme_is_dark(color_scheme: Option<&[u8]>) -> bool {
    let Some(words) = color_scheme else {
        return false;
    };
    let has = |w: &[u8]| words.split(u8::is_ascii_whitespace).any(|x| x == w);
    match (has(b"light"), has(b"dark")) {
        (true, true) => ffi::prefers_dark(),
        (false, true) => true,
        _ => false,
    }
}

pub(crate) fn specified_text(text: &[u8]) -> Option<Vec<u8>> {
    southstar_css_color::specified_text(text).map(String::into_bytes)
}

pub fn parse_color(text: &CStr) -> Option<[u8; 4]> {
    parse_value(text.to_bytes()).map(|c| c.to_rgba8())
}

pub(crate) fn parse_into(text: &CStr, out: &mut Channels) -> bool {
    match parse_color(text) {
        Some(rgba) => {
            *out = rgba.map(Some);
            true
        }
        None => {
            out[3] = Some(255);
            false
        }
    }
}

pub(crate) fn color_text(rgba: [u8; 4]) -> Vec<u8> {
    southstar_css_color::legacy_text(rgba).into_bytes()
}
