//! Southstar — whether a style pass may reuse the previous pass's styles: the signature of the sheets it cascades, and the document, container sizes and interaction state both passes must share.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

pub(crate) fn sheet_signature(ua: u64, authors: &[u64], registered: u64) -> u64 {
    [ua, authors.len() as u64, registered]
        .into_iter()
        .chain(authors.iter().copied())
        .fold(FNV_OFFSET, |h, v| (h ^ v).wrapping_mul(FNV_PRIME))
}

pub(crate) fn zoom_allows_reuse(zoom: f64) -> bool {
    (zoom - 1.0).abs() <= 0.001
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct PassKey {
    pub doc: usize,
    pub sheets: u64,
    pub containers: u64,
    pub focus: usize,
    pub hover: usize,
    pub active: usize,
    pub fullscreen: usize,
}
