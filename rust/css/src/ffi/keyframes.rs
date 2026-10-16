//! Southstar — a style sheet's @keyframes rule, with var() in its stops resolved against an element's custom properties.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use super::computed_units::RawStyle;
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

fn until_nul(mut text: Vec<u8>) -> Vec<u8> {
    if let Some(nul) = text.iter().position(|&c| c == 0) {
        text.truncate(nul);
    }
    text
}

impl KeyframeStop {
    pub(super) fn new(
        pct: f64,
        transform: Option<Transform>,
        raw_props: Option<Vec<u8>>,
    ) -> KeyframeStop {
        KeyframeStop {
            pct,
            transform,
            raw_props,
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
    pub(super) fn new(name: Vec<u8>, stops: Vec<KeyframeStop>) -> Keyframes {
        Keyframes { name, stops }
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
