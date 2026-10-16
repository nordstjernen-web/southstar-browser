//! Southstar — the <color> grammar parsed into a specified value that serializes, and resolves to a computed colour.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_css_syntax::{Component, Kind};

use crate::calc::{self, Node, Unit};
use crate::color::{Color, clamp_or_zero};
use crate::mix::{self, HueMethod, Interpolation};
use crate::named::named;
use crate::number::number;
use crate::space::{Space, normalize_hue};
use crate::tokens::Tokens;

const MAX_NESTING: u32 = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FnKind {
    Rgb,
    Hsl,
    Hwb,
    Lab,
    Lch,
    Oklab,
    Oklch,
    Color,
    Alpha,
}

impl FnKind {
    fn from_name(name: &str) -> Option<FnKind> {
        Some(match name {
            "rgb" | "rgba" => FnKind::Rgb,
            "hsl" | "hsla" => FnKind::Hsl,
            "hwb" => FnKind::Hwb,
            "lab" => FnKind::Lab,
            "lch" => FnKind::Lch,
            "oklab" => FnKind::Oklab,
            "oklch" => FnKind::Oklch,
            "color" => FnKind::Color,
            "alpha" => FnKind::Alpha,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            FnKind::Rgb => "rgb",
            FnKind::Hsl => "hsl",
            FnKind::Hwb => "hwb",
            FnKind::Lab => "lab",
            FnKind::Lch => "lch",
            FnKind::Oklab => "oklab",
            FnKind::Oklch => "oklch",
            FnKind::Color => "color",
            FnKind::Alpha => "alpha",
        }
    }

    fn space(self) -> Option<Space> {
        Some(match self {
            FnKind::Rgb => Space::Srgb,
            FnKind::Hsl => Space::Hsl,
            FnKind::Hwb => Space::Hwb,
            FnKind::Lab => Space::Lab,
            FnKind::Lch => Space::Lch,
            FnKind::Oklab => Space::Oklab,
            FnKind::Oklch => Space::Oklch,
            FnKind::Color | FnKind::Alpha => return None,
        })
    }
}

fn channel_names(space: Space) -> [&'static str; 3] {
    match space {
        Space::Hsl => ["h", "s", "l"],
        Space::Hwb => ["h", "w", "b"],
        Space::Lab | Space::Oklab => ["l", "a", "b"],
        Space::Lch | Space::Oklch => ["l", "c", "h"],
        Space::XyzD50 | Space::XyzD65 => ["x", "y", "z"],
        _ => ["r", "g", "b"],
    }
}

fn percent_reference(kind: FnKind, space: Space, index: usize) -> f64 {
    match (kind, space, index) {
        (FnKind::Rgb, ..) => 255.0,
        (_, Space::Hsl | Space::Hwb, _) => 100.0,
        (_, Space::Lab, 0) => 100.0,
        (_, Space::Lab, _) => 125.0,
        (_, Space::Oklab, 0) | (_, Space::Oklch, 0) => 1.0,
        (_, Space::Oklab, _) | (_, Space::Oklch, _) => 0.4,
        (_, Space::Lch, 0) => 100.0,
        (_, Space::Lch, _) => 150.0,
        _ => 1.0,
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Comp {
    None,
    Number(f64),
    Percent(f64),
    Angle(f64, String),
    Channel(String),
    Calc(Node),
}

#[derive(Clone, PartialEq, Debug)]
pub struct ColorFunction {
    pub kind: FnKind,
    pub space: Space,
    pub origin: Option<Specified>,
    pub comps: [Comp; 3],
    pub alpha: Option<Comp>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum MixPercent {
    Value(f64),
    Calc(Node),
}

#[derive(Clone, PartialEq, Debug)]
pub struct Mix {
    pub method: Option<Interpolation>,
    pub items: Vec<(Specified, Option<MixPercent>)>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Specified {
    Absolute(Color),
    Keyword(String, Color),
    CurrentColor,
    Function(Box<ColorFunction>),
    Mix(Box<Mix>),
    LightDark(Box<Specified>, Box<Specified>),
    Contrast(Box<Specified>),
}

#[derive(Clone, Copy, Default)]
pub struct Context {
    pub current: Option<Color>,
    pub dark: bool,
}

pub fn parse(input: &[u8]) -> Option<Specified> {
    let trimmed = input.trim_ascii();
    if let Some(digits) = trimmed.strip_prefix(b"#")
        && digits.iter().all(u8::is_ascii_alphanumeric)
    {
        return hex(core::str::from_utf8(digits).ok()?).map(Specified::Absolute);
    }
    if trimmed.first().is_some_and(u8::is_ascii_alphabetic)
        && trimmed
            .iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'-')
    {
        return keyword(&core::str::from_utf8(trimmed).ok()?.to_ascii_lowercase());
    }
    let (items, valid) = southstar_css_syntax::parse(input);
    if !valid {
        return None;
    }
    let mut t = Tokens::new(input, &items);
    let c = t.next()?;
    let value = parse_component(&t, c, 0)?;
    t.at_end().then_some(value)
}

fn parse_component(t: &Tokens<'_>, c: &Component, depth: u32) -> Option<Specified> {
    if depth > MAX_NESTING {
        return None;
    }
    match c.kind {
        Kind::Hash => hex(&t.name(c)).map(Specified::Absolute),
        Kind::Ident => keyword(&t.name(c)),
        Kind::Function => function(t, c, depth),
        _ => None,
    }
}

fn keyword(name: &str) -> Option<Specified> {
    match name {
        "currentcolor" => Some(Specified::CurrentColor),
        "transparent" => Some(Specified::Keyword(name.into(), Color::transparent())),
        _ => {
            let [r, g, b] = named(name)?;
            Some(Specified::Keyword(
                name.into(),
                Color::from_rgba8([r, g, b, 255]),
            ))
        }
    }
}

fn hex(digits: &str) -> Option<Color> {
    let d: Vec<u8> = digits
        .chars()
        .map(|c| c.to_digit(16).map(|v| v as u8))
        .collect::<Option<_>>()?;
    let rgba = match d.len() {
        3 => [d[0] * 17, d[1] * 17, d[2] * 17, 255],
        4 => [d[0] * 17, d[1] * 17, d[2] * 17, d[3] * 17],
        6 => [d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5], 255],
        8 => [
            d[0] * 16 + d[1],
            d[2] * 16 + d[3],
            d[4] * 16 + d[5],
            d[6] * 16 + d[7],
        ],
        _ => return None,
    };
    Some(Color::from_rgba8(rgba))
}

fn function(t: &Tokens<'_>, c: &Component, depth: u32) -> Option<Specified> {
    let name = t.name(c);
    let mut args = t.children(c);
    match name.as_str() {
        "color-mix" => return color_mix(&args, depth),
        "light-dark" => {
            let parts = args.split_commas();
            if parts.len() != 2 {
                return None;
            }
            let light = single_color(parts[0], depth)?;
            let dark = single_color(parts[1], depth)?;
            return Some(Specified::LightDark(Box::new(light), Box::new(dark)));
        }
        "contrast-color" => {
            return Some(Specified::Contrast(Box::new(single_color(args, depth)?)));
        }
        _ => {}
    }
    let kind = FnKind::from_name(&name)?;
    if args.eat_ident("from") {
        return relative(kind, args, depth).map(|f| Specified::Function(Box::new(f)));
    }
    match kind {
        FnKind::Rgb | FnKind::Hsl | FnKind::Hwb => legacy_function(kind, args),
        FnKind::Alpha => None,
        _ => modern_function(kind, args),
    }
}

fn single_color(mut t: Tokens<'_>, depth: u32) -> Option<Specified> {
    let c = t.next()?;
    let value = parse_component(&t, c, depth + 1)?;
    t.at_end().then_some(value)
}

fn literal(t: &mut Tokens<'_>, allow_none: bool) -> Option<Option<(f64, Unit)>> {
    let c = t.next()?;
    Some(match c.kind {
        Kind::Number => Some((c.number, Unit::Number)),
        Kind::Percentage => Some((c.number, Unit::Percent)),
        Kind::Dimension => Some((calc::angle_degrees(c.number, &t.unit(c))?, Unit::Angle)),
        Kind::Ident if allow_none && t.name(c) == "none" => None,
        Kind::Function => {
            let node = calc::Parser { channels: &[] }.function(t, c)?;
            Some(calc::eval(&node, &|_| None)?)
        }
        _ => return None,
    })
}

fn alpha_unit(v: (f64, Unit)) -> Option<f64> {
    match v.1 {
        Unit::Number => Some(v.0),
        Unit::Percent => Some(v.0 / 100.0),
        Unit::Angle => None,
    }
}

fn hue_degrees(v: (f64, Unit)) -> Option<f64> {
    (v.1 != Unit::Percent).then_some(v.0)
}

fn legacy_function(kind: FnKind, args: Tokens<'_>) -> Option<Specified> {
    let (values, alpha) = if args.has_comma() {
        if kind == FnKind::Hwb {
            return None;
        }
        let parts = args.split_commas();
        if parts.len() != 3 && parts.len() != 4 {
            return None;
        }
        let mut values = Vec::new();
        for mut part in parts {
            let v = literal(&mut part, false)??;
            if !part.at_end() {
                return None;
            }
            values.push(Some(v));
        }
        let alpha = values.get(3).copied().flatten();
        values.truncate(3);
        let units: Vec<Unit> = values.iter().flatten().map(|v| v.1).collect();
        let ok = match kind {
            FnKind::Rgb => units.iter().all(|&u| u == units[0] && u != Unit::Angle),
            _ => units[1] == Unit::Percent && units[2] == Unit::Percent,
        };
        if !ok {
            return None;
        }
        (values, alpha.map(Some))
    } else {
        let mut args = args;
        let mut values = Vec::new();
        for _ in 0..3 {
            values.push(literal(&mut args, true)?);
        }
        let alpha = if args.eat_delim(b'/') {
            Some(literal(&mut args, true)?)
        } else {
            None
        };
        if !args.at_end() {
            return None;
        }
        (values, alpha)
    };
    let alpha = match alpha {
        None => Some(1.0),
        Some(None) => None,
        Some(Some(v)) => Some(alpha_unit(v)?),
    };
    let missing = values.iter().any(Option::is_none) || alpha.is_none();
    match kind {
        FnKind::Rgb => {
            let mut rgb = [None; 3];
            for (slot, v) in rgb.iter_mut().zip(&values) {
                *slot = match v {
                    None => None,
                    Some((x, Unit::Number)) => Some(*x),
                    Some((x, Unit::Percent)) => Some(x * 255.0 / 100.0),
                    Some(_) => return None,
                };
            }
            let unit = rgb.map(|c| c.map(|v| clamp_or_zero(v, 0.0, 255.0) / 255.0));
            Some(Specified::Absolute(legacy(
                Space::Srgb,
                unit,
                alpha,
                missing,
            )))
        }
        _ => {
            let hue = match values[0] {
                None => None,
                Some(v) => Some(normalize_hue(hue_degrees(v)?)),
            };
            let mut parts = [None; 2];
            for (slot, v) in parts.iter_mut().zip(&values[1..]) {
                *slot = match v {
                    None => None,
                    Some((x, Unit::Number | Unit::Percent)) if missing || kind == FnKind::Hwb => {
                        Some(*x)
                    }
                    Some((x, Unit::Number | Unit::Percent)) => Some(clamp_or_zero(*x, 0.0, 100.0)),
                    Some(_) => return None,
                };
            }
            let space = if kind == FnKind::Hsl {
                Space::Hsl
            } else {
                Space::Hwb
            };
            Some(Specified::Absolute(legacy(
                space,
                [hue, parts[0], parts[1]],
                alpha,
                missing,
            )))
        }
    }
}

fn legacy(space: Space, components: [Option<f64>; 3], alpha: Option<f64>, missing: bool) -> Color {
    Color {
        space,
        components,
        alpha: alpha.map(|a| clamp_or_zero(a, 0.0, 1.0)),
        legacy: !missing,
    }
}

fn component(t: &mut Tokens<'_>, channels: &[&str], hue: bool) -> Option<Comp> {
    let c = t.next()?;
    Some(match c.kind {
        Kind::Number => Comp::Number(c.number),
        Kind::Percentage if !hue => Comp::Percent(c.number),
        Kind::Dimension if hue => {
            let unit = t.unit(c);
            calc::angle_degrees(c.number, &unit)?;
            Comp::Angle(c.number, unit)
        }
        Kind::Ident => {
            let name = t.name(c);
            if name == "none" {
                Comp::None
            } else if channels.contains(&name.as_str()) {
                Comp::Channel(name)
            } else {
                return None;
            }
        }
        Kind::Function => {
            let node = calc::Parser { channels }.function(t, c)?;
            let (_, unit) = calc::eval(&node, &|_| Some(1.0))?;
            let ok = if hue {
                unit != Unit::Percent
            } else {
                unit != Unit::Angle
            };
            if !ok {
                return None;
            }
            Comp::Calc(node)
        }
        _ => return None,
    })
}

fn components(
    t: &mut Tokens<'_>,
    space: Space,
    channels: &[&str],
) -> Option<([Comp; 3], Option<Comp>)> {
    let hue = space.hue_index();
    let a = component(t, channels, hue == Some(0))?;
    let b = component(t, channels, false)?;
    let c = component(t, channels, hue == Some(2))?;
    let alpha = if t.eat_delim(b'/') {
        Some(component(t, channels, false)?)
    } else {
        None
    };
    t.at_end().then_some(([a, b, c], alpha))
}

fn modern_function(kind: FnKind, mut args: Tokens<'_>) -> Option<Specified> {
    if args.has_comma() {
        return None;
    }
    let space = match kind.space() {
        Some(space) => space,
        None => {
            let c = args.next()?;
            if c.kind != Kind::Ident {
                return None;
            }
            Space::from_predefined(&args.name(c))?
        }
    };
    let (comps, alpha) = components(&mut args, space, &[])?;
    let has_calc = comps
        .iter()
        .chain(alpha.iter())
        .any(|c| matches!(c, Comp::Calc(_)));
    let resolved = |i: usize, comp: &Comp| -> Comp {
        match comp {
            Comp::Percent(p) => Comp::Number(p / 100.0 * percent_reference(kind, space, i)),
            Comp::Angle(v, unit) => Comp::Number(calc::angle_degrees(*v, unit).unwrap_or(0.0)),
            other => other.clone(),
        }
    };
    let comps = [
        resolved(0, &comps[0]),
        resolved(1, &comps[1]),
        resolved(2, &comps[2]),
    ];
    let alpha = alpha.map(|a| match a {
        Comp::Percent(p) => Comp::Number(p / 100.0),
        other => other,
    });
    let function = ColorFunction {
        kind,
        space,
        origin: None,
        comps,
        alpha,
    };
    if has_calc {
        return Some(Specified::Function(Box::new(clamp_literals(function))));
    }
    function
        .resolve(&Context::default())
        .map(Specified::Absolute)
}

fn clamp_literals(mut f: ColorFunction) -> ColorFunction {
    let probe = |i: usize, comps: &[Comp; 3]| -> Option<f64> {
        let mut values = [Some(0.0); 3];
        match &comps[i] {
            Comp::Number(v) => values[i] = Some(*v),
            _ => return None,
        }
        let color = Color::new(f.space, values, Some(1.0)).clamped();
        color.components[i]
    };
    for i in 0..3 {
        if let Some(v) = probe(i, &f.comps) {
            f.comps[i] = Comp::Number(v);
        }
    }
    if let Some(Comp::Number(a)) = f.alpha {
        f.alpha = Some(Comp::Number(clamp_or_zero(a, 0.0, 1.0)));
    }
    f
}

fn relative(kind: FnKind, mut args: Tokens<'_>, depth: u32) -> Option<ColorFunction> {
    let origin_component = args.next()?;
    let origin = parse_component(&args, origin_component, depth + 1)?;
    let space = match kind {
        FnKind::Color => {
            let c = args.next()?;
            if c.kind != Kind::Ident {
                return None;
            }
            Space::from_predefined(&args.name(c))?
        }
        FnKind::Alpha => Space::Srgb,
        _ => kind.space()?,
    };
    if kind == FnKind::Alpha {
        if !args.eat_delim(b'/') {
            return None;
        }
        let alpha = Some(component(&mut args, &["alpha"], false)?);
        if !args.at_end() {
            return None;
        }
        return Some(ColorFunction {
            kind,
            space,
            origin: Some(origin),
            comps: [Comp::None, Comp::None, Comp::None],
            alpha,
        });
    }
    let [a, b, c] = channel_names(space);
    let channels = [a, b, c, "alpha"];
    let (comps, alpha) = components(&mut args, space, &channels)?;
    Some(ColorFunction {
        kind,
        space,
        origin: Some(origin),
        comps,
        alpha,
    })
}

fn mix_percent(t: &mut Tokens<'_>) -> Option<Option<MixPercent>> {
    let save = t.pos;
    let Some(c) = t.peek() else {
        return Some(None);
    };
    let value = match c.kind {
        Kind::Percentage => MixPercent::Value(c.number),
        Kind::Function if calc::is_math_function(&t.name(c)) => {
            let node = calc::Parser { channels: &[] }.function(t, c)?;
            let (v, unit) = calc::eval(&node, &|_| None)?;
            if unit != Unit::Percent {
                return None;
            }
            if !(0.0..=100.0).contains(&v) {
                return None;
            }
            MixPercent::Calc(node)
        }
        _ => {
            t.pos = save;
            return Some(None);
        }
    };
    if let MixPercent::Value(v) = value
        && !(0.0..=100.0).contains(&v)
    {
        return None;
    }
    t.next();
    Some(Some(value))
}

fn interpolation(t: &mut Tokens<'_>) -> Option<Interpolation> {
    let c = t.next()?;
    if c.kind != Kind::Ident {
        return None;
    }
    let space = Space::from_interpolation(&t.name(c))?;
    let mut method = Interpolation {
        space,
        hue: HueMethod::Shorter,
    };
    if let Some(c) = t.peek()
        && c.kind == Kind::Ident
    {
        space.hue_index()?;
        method.hue = HueMethod::parse(&t.name(c))?;
        t.next();
        if !t.eat_ident("hue") {
            return None;
        }
    }
    t.at_end().then_some(method)
}

fn color_mix(args: &Tokens<'_>, depth: u32) -> Option<Specified> {
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
    let mut items = Vec::new();
    for mut part in parts {
        let mut percent = mix_percent(&mut part)?;
        let c = part.next()?;
        let color = parse_component(&part, c, depth + 1)?;
        if percent.is_none() {
            percent = mix_percent(&mut part)?;
        }
        if !part.at_end() {
            return None;
        }
        items.push((color, percent));
    }
    Some(Specified::Mix(Box::new(Mix { method, items })))
}

impl ColorFunction {
    fn channel_values(&self, origin: &Color) -> (Color, [Option<f64>; 4]) {
        let converted = match self.kind {
            FnKind::Alpha => *origin,
            _ => origin.to_space(self.space),
        };
        let mut values = [None; 4];
        for (i, slot) in values.iter_mut().take(3).enumerate() {
            *slot = converted.components[i].map(|v| {
                if self.kind == FnKind::Rgb {
                    v * 255.0
                } else {
                    v
                }
            });
        }
        values[3] = converted.alpha;
        (converted, values)
    }

    pub fn resolve(&self, ctx: &Context) -> Option<Color> {
        let origin = match &self.origin {
            Some(origin) => Some(origin.resolve(ctx)?),
            None => None,
        };
        let names = channel_names(self.space);
        let (converted, values) = match &origin {
            Some(o) => self.channel_values(o),
            None => (Color::new(self.space, [None; 3], Some(1.0)), [None; 4]),
        };
        let lookup = |name: &str| -> Option<Option<f64>> {
            if name == "alpha" {
                return Some(values[3]);
            }
            names.iter().position(|n| *n == name).map(|i| values[i])
        };
        let calc_lookup = |name: &str| lookup(name).map(|v| v.unwrap_or(0.0));
        let value = |comp: &Comp, reference: f64| -> Option<Option<f64>> {
            Some(match comp {
                Comp::None => None,
                Comp::Number(v) => Some(*v),
                Comp::Percent(p) => Some(p / 100.0 * reference),
                Comp::Angle(v, unit) => Some(calc::angle_degrees(*v, unit)?),
                Comp::Channel(name) => lookup(name)?,
                Comp::Calc(node) => {
                    let (v, unit) = calc::eval(node, &calc_lookup)?;
                    Some(match unit {
                        Unit::Percent => v / 100.0 * reference,
                        _ => v,
                    })
                }
            })
        };
        if self.kind == FnKind::Alpha {
            let origin = origin?;
            let alpha = match &self.alpha {
                Some(comp) => value(comp, 1.0)?,
                None => origin.alpha,
            };
            let mut result = Color::new(converted.space, converted.components, alpha);
            result.alpha = result.alpha.map(|a| clamp_or_zero(a, 0.0, 1.0));
            return Some(result);
        }
        let mut comps = [None; 3];
        for (i, slot) in comps.iter_mut().enumerate() {
            let v = value(&self.comps[i], percent_reference(self.kind, self.space, i))?;
            *slot = if self.kind == FnKind::Rgb {
                v.map(|x| x / 255.0)
            } else {
                v
            };
        }
        let alpha = match &self.alpha {
            Some(comp) => value(comp, 1.0)?,
            None if origin.is_some() => converted.alpha,
            None => Some(1.0),
        };
        Some(Color::new(self.space, comps, alpha).clamped())
    }

    fn serialize(&self) -> String {
        let comp = |c: &Comp| match c {
            Comp::None => "none".to_string(),
            Comp::Number(v) => number(*v),
            Comp::Percent(p) => format!("{}%", number(*p)),
            Comp::Angle(v, unit) => format!("{}{unit}", number(*v)),
            Comp::Channel(name) => name.clone(),
            Comp::Calc(node) => calc::serialize_root(&calc::simplify(node)),
        };
        let mut out = format!("{}(", self.kind.name());
        if let Some(origin) = &self.origin {
            out.push_str("from ");
            out.push_str(&origin.serialize());
            out.push(' ');
        }
        if self.kind == FnKind::Color {
            out.push_str(self.space.name());
            out.push(' ');
        }
        if self.kind != FnKind::Alpha {
            let parts: Vec<String> = self.comps.iter().map(comp).collect();
            out.push_str(&parts.join(" "));
        } else if out.ends_with(' ') {
            out.pop();
        }
        match &self.alpha {
            Some(Comp::Number(a)) if self.origin.is_none() && *a >= 1.0 => {}
            Some(a) => {
                out.push_str(" / ");
                out.push_str(&comp(a));
            }
            None => {}
        }
        out.push(')');
        out
    }
}

fn mix_percent_value(p: &MixPercent) -> Option<f64> {
    match p {
        MixPercent::Value(v) => Some(*v),
        MixPercent::Calc(node) => calc::eval(node, &|_| None).map(|v| v.0),
    }
}

impl Mix {
    fn resolve(&self, ctx: &Context) -> Option<Color> {
        let mut items = Vec::new();
        for (color, percent) in &self.items {
            let p = match percent {
                Some(p) => Some(mix_percent_value(p)?),
                None => None,
            };
            items.push((color.resolve(ctx)?, p));
        }
        mix::mix(&items, self.method.unwrap_or_default())
    }

    fn serialize(&self) -> String {
        let mut out = String::from("color-mix(");
        if let Some(method) = self.method.filter(|m| *m != Interpolation::default()) {
            out.push_str("in ");
            out.push_str(method.space.name());
            if method.hue != HueMethod::Shorter {
                out.push(' ');
                out.push_str(method.hue.name());
                out.push_str(" hue");
            }
            out.push_str(", ");
        }
        let any_calc = self
            .items
            .iter()
            .any(|(_, p)| matches!(p, Some(MixPercent::Calc(_))));
        let percents: Vec<Option<f64>> = self
            .items
            .iter()
            .map(|(_, p)| match p {
                Some(MixPercent::Value(v)) => Some(*v),
                _ => None,
            })
            .collect();
        let any_given = percents.iter().any(Option::is_some);
        let filled = mix::filled_percents(&percents);
        let even = 100.0 / self.items.len() as f64;
        let omit = !any_given || filled.iter().all(|p| (p - even).abs() < 1e-9);
        let texts: Vec<String> = self
            .items
            .iter()
            .zip(&filled)
            .map(|((color, percent), fill)| {
                let mut s = color.serialize();
                if any_calc {
                    match percent {
                        Some(MixPercent::Value(v)) => s.push_str(&format!(" {}%", number(*v))),
                        Some(MixPercent::Calc(node)) => {
                            s.push(' ');
                            s.push_str(&calc::serialize_root(&calc::simplify(node)));
                        }
                        None => {}
                    }
                } else if !omit {
                    s.push_str(&format!(" {}%", number(*fill)));
                }
                s
            })
            .collect();
        out.push_str(&texts.join(", "));
        out.push(')');
        out
    }
}

fn contrast(color: &Color) -> Color {
    let luminance = |rgb: [f64; 3]| {
        let lin = rgb.map(|c| crate::space::srgb_to_linear(c.clamp(0.0, 1.0)));
        0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]
    };
    let y = luminance(color.to_srgb());
    let with_white = 1.05 / (y + 0.05);
    let with_black = (y + 0.05) / 0.05;
    if with_white > with_black {
        Color::from_rgba8([255, 255, 255, 255])
    } else {
        Color::from_rgba8([0, 0, 0, 255])
    }
}

impl Specified {
    pub fn resolve(&self, ctx: &Context) -> Option<Color> {
        match self {
            Specified::Absolute(c) | Specified::Keyword(_, c) => Some(*c),
            Specified::CurrentColor => ctx.current,
            Specified::Function(f) => f.resolve(ctx),
            Specified::Mix(m) => m.resolve(ctx),
            Specified::LightDark(light, dark) => {
                if ctx.dark {
                    dark.resolve(ctx)
                } else {
                    light.resolve(ctx)
                }
            }
            Specified::Contrast(inner) => Some(contrast(&inner.resolve(ctx)?)),
        }
    }

    pub fn uses_current_color(&self) -> bool {
        match self {
            Specified::CurrentColor => true,
            Specified::Absolute(_) | Specified::Keyword(..) => false,
            Specified::Function(f) => f.origin.as_ref().is_some_and(Specified::uses_current_color),
            Specified::Mix(m) => m.items.iter().any(|(c, _)| c.uses_current_color()),
            Specified::LightDark(a, b) => a.uses_current_color() || b.uses_current_color(),
            Specified::Contrast(c) => c.uses_current_color(),
        }
    }

    pub fn serialize(&self) -> String {
        match self {
            Specified::Absolute(c) => c.serialize(),
            Specified::Keyword(name, _) => name.clone(),
            Specified::CurrentColor => "currentcolor".into(),
            Specified::Function(f) => f.serialize(),
            Specified::Mix(m) => m.serialize(),
            Specified::LightDark(a, b) => {
                format!("light-dark({}, {})", a.serialize(), b.serialize())
            }
            Specified::Contrast(c) => format!("contrast-color({})", c.serialize()),
        }
    }
}
