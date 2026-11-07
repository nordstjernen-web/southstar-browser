//! Southstar — g_ascii_strtoll's decimal parsing for the cookie rules.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub fn strtoll(text: &[u8]) -> (i64, usize) {
    let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
    let mut i = text
        .iter()
        .position(|&c| !matches!(c, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r'))
        .unwrap_or(text.len());
    let negative = text.get(i) == Some(&b'-');
    if matches!(text.get(i), Some(b'-' | b'+')) {
        i += 1;
    }
    let digits_start = i;
    let mut magnitude: u64 = 0;
    let mut overflow = false;
    while let Some(&d) = text.get(i).filter(|c| c.is_ascii_digit()) {
        match magnitude
            .checked_mul(10)
            .and_then(|m| m.checked_add(u64::from(d - b'0')))
        {
            Some(m) => magnitude = m,
            None => overflow = true,
        }
        i += 1;
    }
    if i == digits_start {
        return (0, 0);
    }
    let value = match (negative, overflow) {
        (false, true) => i64::MAX,
        (true, true) => i64::MIN,
        (false, false) => i64::try_from(magnitude).unwrap_or(i64::MAX),
        (true, false) => 0i64.checked_sub_unsigned(magnitude).unwrap_or(i64::MIN),
    };
    (value, i)
}
