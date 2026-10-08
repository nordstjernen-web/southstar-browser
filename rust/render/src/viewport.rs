//! Southstar — the layout width a page asks for in its viewport meta tag.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_glib as glib;

const MIN_WIDTH: f64 = 320.0;
const MAX_WIDTH: f64 = 4096.0;

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn trim(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| !is_space(c)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&c| !is_space(c))
        .map_or(start, |i| i + 1);
    &s[start..end.max(start)]
}

pub fn width_hint(content: Option<&[u8]>) -> f64 {
    let Some(content) = content.filter(|c| !c.is_empty()) else {
        return 0.0;
    };
    for part in content.split(|&c| c == b',' || c == b';') {
        let part = trim(part);
        let Some(eq) = part.iter().position(|&c| c == b'=') else {
            continue;
        };
        let (key, value) = (trim(&part[..eq]), trim(&part[eq + 1..]));
        if !key.eq_ignore_ascii_case(b"width") {
            continue;
        }
        if value.eq_ignore_ascii_case(b"device-width") {
            break;
        }
        let (n, consumed) = glib::ascii_strtod_prefix(value);
        if consumed > 0 && (MIN_WIDTH..=MAX_WIDTH).contains(&n) {
            return n;
        }
    }
    0.0
}
