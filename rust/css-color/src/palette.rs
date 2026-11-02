//! Southstar — palette-mix() for font-palette: its palettes, interpolation method and percentages, serialized in canonical form.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_css_syntax::{Component, Kind};

use crate::calc;
use crate::mix::{self, HueMethod, Interpolation};
use crate::number::number;
use crate::parse::{MixPercent, interpolation, mix_percent, mix_percent_value};
use crate::tokens::Tokens;

const MAX_NESTING: u32 = 16;

pub fn palette_mix_text(input: &[u8]) -> Option<String> {
    palette_mix(input, false)
}

pub fn palette_mix_specified(input: &[u8]) -> Option<String> {
    palette_mix(input, true)
}

fn palette_mix(input: &[u8], specified: bool) -> Option<String> {
    let (items, valid) = southstar_css_syntax::parse(input);
    if !valid {
        return None;
    }
    let mut t = Tokens::new(input, &items);
    let c = t.next()?;
    let text = palette_function(&t, c, 0, specified)?;
    t.at_end().then_some(text)
}

fn palette_function(t: &Tokens<'_>, c: &Component, depth: u32, specified: bool) -> Option<String> {
    if depth > MAX_NESTING || c.kind != Kind::Function || t.name(c) != "palette-mix" {
        return None;
    }
    let args = t.children(c);
    let mut parts = args.split_commas();
    let mut method = None;
    if let Some(first) = parts.first_mut()
        && first.eat_ident("in")
    {
        method = Some(interpolation(first)?);
        parts.remove(0);
    }
    if parts.is_empty() {
        return None;
    }
    let mut palettes = Vec::with_capacity(parts.len());
    let mut percents = Vec::with_capacity(parts.len());
    for mut part in parts {
        let mut percent = mix_percent(&mut part)?;
        let c = part.next()?;
        let palette = palette(&part, c, depth + 1, specified)?;
        if percent.is_none() {
            percent = mix_percent(&mut part)?;
        }
        if !part.at_end() {
            return None;
        }
        if let Some(p) = &percent {
            mix_percent_value(p)?;
        }
        palettes.push(palette);
        percents.push(percent);
    }
    Some(serialize(method, &palettes, &percents, specified))
}

fn palette(t: &Tokens<'_>, c: &Component, depth: u32, specified: bool) -> Option<String> {
    match c.kind {
        Kind::Ident => {
            let raw = &t.input[c.value.clone()?];
            if raw.starts_with(b"--") && raw.len() > 2 {
                return core::str::from_utf8(raw).ok().map(str::to_owned);
            }
            let name = t.name(c);
            matches!(name.as_str(), "normal" | "light" | "dark").then_some(name)
        }
        Kind::Function => palette_function(t, c, depth, specified),
        _ => None,
    }
}

fn serialize(
    method: Option<Interpolation>,
    palettes: &[String],
    given: &[Option<MixPercent>],
    specified: bool,
) -> String {
    let mut out = String::from("palette-mix(");
    if let Some(method) = method.filter(|m| *m != Interpolation::default()) {
        out.push_str("in ");
        out.push_str(method.space.name());
        if method.hue != HueMethod::Shorter {
            out.push(' ');
            out.push_str(method.hue.name());
            out.push_str(" hue");
        }
        out.push_str(", ");
    }
    let percents: Vec<Option<f64>> = given
        .iter()
        .map(|p| p.as_ref().and_then(mix_percent_value))
        .collect();
    if specified && given.iter().any(|p| matches!(p, Some(MixPercent::Calc(_)))) {
        let texts: Vec<String> = palettes
            .iter()
            .zip(given)
            .map(|(palette, percent)| match percent {
                Some(MixPercent::Value(v)) => format!("{palette} {}%", number(*v)),
                Some(MixPercent::Calc(node)) => {
                    format!("{palette} {}", calc::serialize_root(&calc::simplify(node)))
                }
                None => palette.clone(),
            })
            .collect();
        out.push_str(&texts.join(", "));
        out.push(')');
        return out;
    }
    let any_given = percents.iter().any(Option::is_some);
    let filled = mix::filled_percents(&percents);
    let even = 100.0 / palettes.len() as f64;
    let omit = !any_given || filled.iter().all(|p| (p - even).abs() < 1e-9);
    let texts: Vec<String> = palettes
        .iter()
        .zip(&filled)
        .map(|(palette, fill)| {
            if omit {
                palette.clone()
            } else {
                format!("{palette} {}%", number(*fill))
            }
        })
        .collect();
    out.push_str(&texts.join(", "));
    out.push(')');
    out
}
