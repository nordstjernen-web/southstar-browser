//! Southstar — byte-string helpers matching GLib's ASCII whitespace stripping and splitting.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

pub fn strip(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&b| !is_space(b)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&b| !is_space(b))
        .map_or(start, |i| i + 1);
    &s[start..end]
}

pub fn truncated(mut s: Vec<u8>, cap: usize) -> Vec<u8> {
    s.truncate(cap.saturating_sub(1));
    s
}
