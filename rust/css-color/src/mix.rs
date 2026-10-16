//! Southstar — colour interpolation: missing components, premultiplied alpha, hue paths and N-colour color-mix().
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::color::Color;
use crate::space::{Space, normalize_hue};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HueMethod {
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

impl HueMethod {
    pub fn parse(name: &str) -> Option<HueMethod> {
        Some(match name {
            "shorter" => HueMethod::Shorter,
            "longer" => HueMethod::Longer,
            "increasing" => HueMethod::Increasing,
            "decreasing" => HueMethod::Decreasing,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            HueMethod::Shorter => "shorter",
            HueMethod::Longer => "longer",
            HueMethod::Increasing => "increasing",
            HueMethod::Decreasing => "decreasing",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Interpolation {
    pub space: Space,
    pub hue: HueMethod,
}

impl Default for Interpolation {
    fn default() -> Self {
        Interpolation {
            space: Space::Oklab,
            hue: HueMethod::Shorter,
        }
    }
}

fn fix_hues(h1: f64, h2: f64, method: HueMethod) -> (f64, f64) {
    let d = h2 - h1;
    match method {
        HueMethod::Shorter if d > 180.0 => (h1 + 360.0, h2),
        HueMethod::Shorter if d < -180.0 => (h1, h2 + 360.0),
        HueMethod::Longer if d > 0.0 && d < 180.0 => (h1 + 360.0, h2),
        HueMethod::Longer if d > -180.0 && d <= 0.0 => (h1, h2 + 360.0),
        HueMethod::Increasing if d < 0.0 => (h1, h2 + 360.0),
        HueMethod::Decreasing if d > 0.0 => (h1 + 360.0, h2),
        _ => (h1, h2),
    }
}

pub fn interpolate(a: &Color, b: &Color, t: f64, method: Interpolation) -> Color {
    let space = method.space;
    let a = a.to_space(space);
    let b = b.to_space(space);
    let hue = space.hue_index();
    let pick = |x: Option<f64>, y: Option<f64>| (x.or(y), y.or(x));
    let (alpha_a, alpha_b) = pick(a.alpha, b.alpha);
    let alpha = match (alpha_a, alpha_b) {
        (Some(x), Some(y)) => Some(x * (1.0 - t) + y * t),
        _ => None,
    };
    let (wa, wb) = (alpha_a.unwrap_or(1.0), alpha_b.unwrap_or(1.0));
    let result_alpha = alpha.unwrap_or(1.0);
    let mut components = [None; 3];
    for (i, slot) in components.iter_mut().enumerate() {
        let (x, y) = pick(a.components[i], b.components[i]);
        let (Some(x), Some(y)) = (x, y) else {
            continue;
        };
        *slot = Some(if Some(i) == hue {
            let (x, y) = fix_hues(x, y, method.hue);
            normalize_hue(x * (1.0 - t) + y * t)
        } else if result_alpha == 0.0 {
            0.0
        } else {
            (x * wa * (1.0 - t) + y * wb * t) / result_alpha
        });
    }
    Color::new(space, components, alpha)
}

pub fn mix(items: &[(Color, Option<f64>)], method: Interpolation) -> Option<Color> {
    let (weights, alpha_multiplier) = normalize(items.iter().map(|i| i.1).collect())?;
    let mut iter = items.iter().zip(weights);
    let ((first, _), first_weight) = iter.next()?;
    let mut acc = first.to_space(method.space);
    let mut acc_weight = first_weight;
    for ((color, _), weight) in iter {
        let total = acc_weight + weight;
        let t = if total == 0.0 { 0.5 } else { weight / total };
        acc = interpolate(&acc, color, t, method);
        acc_weight = total;
    }
    acc.alpha = acc.alpha.map(|a| a * alpha_multiplier);
    Some(acc)
}

pub(crate) fn normalize(percents: Vec<Option<f64>>) -> Option<(Vec<f64>, f64)> {
    let specified: f64 = percents.iter().flatten().sum();
    let omitted = percents.iter().filter(|p| p.is_none()).count();
    let share = if omitted > 0 {
        (100.0 - specified).max(0.0) / omitted as f64
    } else {
        0.0
    };
    let filled: Vec<f64> = percents.iter().map(|p| p.unwrap_or(share)).collect();
    let total: f64 = filled.iter().sum();
    if !total.is_finite() {
        return None;
    }
    if total == 0.0 {
        return Some((filled, 0.0));
    }
    let scaled = filled.iter().map(|p| p * 100.0 / total).collect();
    Some((scaled, (total / 100.0).min(1.0)))
}

pub(crate) fn filled_percents(percents: &[Option<f64>]) -> Vec<f64> {
    let specified: f64 = percents.iter().flatten().sum();
    let omitted = percents.iter().filter(|p| p.is_none()).count();
    let share = if omitted > 0 {
        (100.0 - specified).max(0.0) / omitted as f64
    } else {
        0.0
    };
    percents.iter().map(|p| p.unwrap_or(share)).collect()
}
