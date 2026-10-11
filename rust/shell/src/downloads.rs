//! Southstar — naming downloaded files: the suggested or URL-derived name and its numbered alternatives.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const FALLBACK_NAME: &[u8] = b"download";
pub const MAX_ALTERNATIVES: u32 = 1000;
pub const RECENT_SCAN_LIMIT: usize = 200;
pub const RECENT_SHOWN_LIMIT: usize = 25;

pub fn usable_name(name: &[u8]) -> bool {
    !name.is_empty() && name != b"." && name != b"/"
}

pub fn strip_query(base: &[u8]) -> &[u8] {
    base.iter()
        .position(|&c| c == b'?')
        .map_or(base, |q| &base[..q])
}

pub fn numbered(name: &[u8], n: u32) -> Vec<u8> {
    let mut out = name.to_vec();
    out.extend_from_slice(format!(".{n}").as_bytes());
    out
}

pub fn finished_label(name: &[u8], detail: &[u8]) -> Vec<u8> {
    [name, " — ".as_bytes(), detail].concat()
}

pub fn listed(name: &[u8]) -> bool {
    name.first() != Some(&b'.')
}
