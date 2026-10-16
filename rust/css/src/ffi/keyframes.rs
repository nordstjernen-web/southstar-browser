//! Southstar — a @keyframes rule copied out of a style sheet, with var() in its stops resolved against an element's custom properties.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::computed_units::RawStyle;
use super::sheet::{RawKeyframes, RawStop, text_at};
use crate::keyframes;
use crate::transform::Transform;

#[derive(Clone)]
pub struct KeyframeStop {
    pct: f64,
    transform: Option<Transform>,
    raw_props: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct Keyframes {
    name: Vec<u8>,
    stops: Vec<KeyframeStop>,
}

fn raw_stops(raw: &RawKeyframes) -> &[RawStop] {
    let n = usize::try_from(raw.n_stops).unwrap_or(0);
    if raw.stops.is_null() || n == 0 {
        return &[];
    }
    // SAFETY: a keyframes rule owns n_stops stops at stops, freed with its sheet.
    unsafe { core::slice::from_raw_parts(raw.stops, n) }
}

fn until_nul(mut text: Vec<u8>) -> Vec<u8> {
    if let Some(nul) = text.iter().position(|&c| c == 0) {
        text.truncate(nul);
    }
    text
}

impl KeyframeStop {
    fn of(raw: &RawStop) -> KeyframeStop {
        KeyframeStop {
            pct: raw.pct,
            transform: (raw.has_transform != 0).then_some(raw.transform),
            raw_props: text_at(raw.raw_props).map(|text| text.to_bytes().to_vec()),
        }
    }

    pub fn pct(&self) -> f64 {
        self.pct
    }

    pub fn transform(&self) -> Option<&Transform> {
        self.transform.as_ref()
    }

    pub fn raw_props(&self) -> Option<&[u8]> {
        self.raw_props.as_deref()
    }

    fn resolve(&self, style: &RawStyle) -> KeyframeStop {
        let resolved = self
            .raw_props
            .as_deref()
            .and_then(|raw| style.resolve_vars(raw))
            .map(until_nul);
        let merged = resolved.as_deref().and_then(|text| {
            keyframes::resolved_transform(text, self.transform.unwrap_or_default())
        });
        KeyframeStop {
            pct: self.pct,
            transform: merged.or(self.transform),
            raw_props: resolved,
        }
    }
}

impl Keyframes {
    pub(super) fn of(raw: &RawKeyframes) -> Option<Keyframes> {
        Some(Keyframes {
            name: text_at(raw.name)?.to_bytes().to_vec(),
            stops: raw_stops(raw).iter().map(KeyframeStop::of).collect(),
        })
    }

    pub fn name(&self) -> &[u8] {
        &self.name
    }

    pub fn stops(&self) -> &[KeyframeStop] {
        &self.stops
    }

    pub fn resolve(&self, style: &RawStyle) -> Option<Keyframes> {
        if self.stops.iter().all(|stop| stop.raw_props.is_none()) {
            return None;
        }
        Some(Keyframes {
            name: self.name.clone(),
            stops: self.stops.iter().map(|stop| stop.resolve(style)).collect(),
        })
    }
}
