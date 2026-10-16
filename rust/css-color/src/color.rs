//! Southstar — an absolute colour in its own space, with missing components, conversion and serialization.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::number::{alpha_byte_text, number};
use crate::space::{Analog, Space, hsl_to_srgb, hwb_to_srgb, normalize_hue};

const ROUNDING_SLACK: f64 = 1e-6;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Color {
    pub space: Space,
    pub components: [Option<f64>; 3],
    pub alpha: Option<f64>,
    pub legacy: bool,
}

impl Color {
    pub fn new(space: Space, components: [Option<f64>; 3], alpha: Option<f64>) -> Color {
        Color {
            space,
            components,
            alpha,
            legacy: false,
        }
    }

    pub fn from_rgba8(rgba: [u8; 4]) -> Color {
        let unit = |v: u8| Some(f64::from(v) / 255.0);
        Color {
            space: Space::Srgb,
            components: [unit(rgba[0]), unit(rgba[1]), unit(rgba[2])],
            alpha: unit(rgba[3]),
            legacy: true,
        }
    }

    pub fn transparent() -> Color {
        Color::from_rgba8([0, 0, 0, 0])
    }

    pub fn values(&self) -> [f64; 3] {
        self.components.map(|c| c.unwrap_or(0.0))
    }

    pub fn alpha_value(&self) -> f64 {
        self.alpha.unwrap_or(0.0)
    }

    fn has_missing(&self) -> bool {
        self.components.iter().any(Option::is_none) || self.alpha.is_none()
    }

    pub fn to_space(&self, target: Space) -> Color {
        if self.space == target {
            return Color {
                legacy: false,
                ..*self
            };
        }
        let xyz = self.space.to_xyz_d65(self.values());
        let mut components = target.xyz_d65_into(xyz);
        let from = self.space.analogs();
        let to = target.analogs();
        let all_missing = self.components.iter().all(Option::is_none);
        let colorful_missing = |analogs: &[Analog; 3], comps: &[Option<f64>; 3]| {
            let polar: Vec<_> = (0..3)
                .filter(|&i| matches!(analogs[i], Analog::Colorfulness | Analog::Hue))
                .collect();
            let opponent: Vec<_> = (0..3)
                .filter(|&i| matches!(analogs[i], Analog::OpponentA | Analog::OpponentB))
                .collect();
            let set = if polar.len() == 2 { polar } else { opponent };
            set.len() == 2 && set.iter().all(|&i| comps[i].is_none())
        };
        for (i, slot) in components.iter_mut().enumerate() {
            let carried = to[i] != Analog::Other
                && (0..3).any(|j| from[j] == to[i] && self.components[j].is_none());
            if all_missing || carried {
                *slot = None;
            }
        }
        let source_hue = self.space.hue_index();
        let target_hue = target.hue_index();
        let chroma_missing = (0..3)
            .filter(|&i| Some(i) != source_hue)
            .all(|i| self.components[i].is_none());
        if chroma_missing {
            for (i, slot) in components.iter_mut().enumerate() {
                if Some(i) != target_hue {
                    *slot = None;
                }
            }
        }
        if colorful_missing(&from, &self.components) {
            for (i, slot) in components.iter_mut().enumerate() {
                if matches!(
                    to[i],
                    Analog::Colorfulness | Analog::Hue | Analog::OpponentA | Analog::OpponentB
                ) {
                    *slot = None;
                }
            }
        }
        Color::new(target, components, self.alpha)
    }

    pub fn to_srgb(&self) -> [f64; 3] {
        match self.space {
            Space::Srgb => return self.values(),
            Space::Hsl => return hsl_to_srgb(self.values()),
            Space::Hwb => return hwb_to_srgb(self.values()),
            _ => {}
        }
        let xyz = self.space.to_xyz_d65(self.values());
        Space::Srgb.xyz_d65_into(xyz).map(|c| c.unwrap_or(0.0))
    }

    pub fn to_rgba8(&self) -> [u8; 4] {
        let byte = |v: f64| {
            if v.is_nan() {
                0
            } else {
                ((v * 255.0 + ROUNDING_SLACK).clamp(0.0, 255.0) + 0.5) as u8
            }
        };
        let [r, g, b] = self.to_srgb();
        [byte(r), byte(g), byte(b), byte(self.alpha_value())]
    }

    pub fn serialize(&self) -> String {
        if self.legacy && !self.has_missing() {
            return legacy_text(self.to_rgba8());
        }
        let alpha = match self.alpha {
            Some(a) if a >= 1.0 => String::new(),
            Some(a) => format!(" / {}", number(a)),
            None => " / none".to_string(),
        };
        let part = |i: usize| match self.components[i] {
            Some(v) => number(v),
            None => "none".to_string(),
        };
        let percent = |i: usize| match self.components[i] {
            Some(v) => format!("{}%", number(v)),
            None => "none".to_string(),
        };
        match self.space {
            Space::Hsl | Space::Hwb if !self.has_missing() => {
                Color::new(Space::Srgb, self.to_srgb().map(Some), self.alpha).serialize()
            }
            Space::Hsl | Space::Hwb => format!(
                "{}({} {} {}{alpha})",
                self.space.name(),
                part(0),
                percent(1),
                percent(2)
            ),
            Space::Lab | Space::Lch | Space::Oklab | Space::Oklch => format!(
                "{}({} {} {}{alpha})",
                self.space.name(),
                part(0),
                part(1),
                part(2)
            ),
            space => format!(
                "color({} {} {} {}{alpha})",
                space.name(),
                part(0),
                part(1),
                part(2)
            ),
        }
    }

    pub(crate) fn clamped(mut self) -> Color {
        self.alpha = self.alpha.map(|a| clamp_or_zero(a, 0.0, 1.0));
        match self.space {
            Space::Lab | Space::Lch => {
                self.components[0] = self.components[0].map(|l| clamp_or_zero(l, 0.0, 100.0));
            }
            Space::Oklab | Space::Oklch => {
                self.components[0] = self.components[0].map(|l| clamp_or_zero(l, 0.0, 1.0));
            }
            _ => {}
        }
        if matches!(self.space, Space::Lch | Space::Oklch) {
            self.components[1] = self.components[1].map(|c| clamp_or_zero(c, 0.0, f64::INFINITY));
        }
        if let Some(h) = self.space.hue_index() {
            self.components[h] = self.components[h].map(normalize_hue);
        }
        for c in self.components.iter_mut().flatten() {
            if c.is_nan() {
                *c = 0.0;
            }
        }
        self
    }
}

pub(crate) fn clamp_or_zero(v: f64, low: f64, high: f64) -> f64 {
    if v.is_nan() {
        low.max(0.0).min(high)
    } else {
        v.clamp(low, high)
    }
}

pub fn legacy_text(rgba: [u8; 4]) -> String {
    let [r, g, b, a] = rgba;
    if a == 255 {
        format!("rgb({r}, {g}, {b})")
    } else {
        format!("rgba({r}, {g}, {b}, {})", alpha_byte_text(a))
    }
}
