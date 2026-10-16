//! Southstar — CSS Color 4/5 values: parsing, colour spaces, color-mix(), relative colours and serialization.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

mod calc;
mod color;
mod mix;
mod named;
mod number;
mod parse;
mod space;
mod tokens;

#[cfg(test)]
mod tests;

pub use color::{Color, legacy_text};
pub use mix::{HueMethod, Interpolation, interpolate};
pub use named::{NAMED, named};
pub use number::alpha_byte_text;
pub use parse::{Context, Specified, parse};
pub use space::Space;

pub fn specified_text(input: &[u8]) -> Option<String> {
    parse(input).map(|value| value.serialize())
}

pub fn computed(input: &[u8], ctx: &Context) -> Option<Color> {
    parse(input)?.resolve(ctx)
}

pub fn computed_text(input: &[u8], ctx: &Context) -> Option<String> {
    computed(input, ctx).map(|c| c.serialize())
}

pub fn rgba8(input: &[u8], ctx: &Context) -> Option<[u8; 4]> {
    computed(input, ctx).map(|c| c.to_rgba8())
}
