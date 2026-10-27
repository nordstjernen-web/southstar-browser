//! Southstar — naming downloaded files: the suggested or URL-derived name made safe for one path component, its numbered alternatives, and which file types wait for the user before they are saved.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const FALLBACK_NAME: &[u8] = b"download";
pub const MAX_ALTERNATIVES: u32 = 1000;
pub const RECENT_SCAN_LIMIT: usize = 200;
pub const RECENT_SHOWN_LIMIT: usize = 25;
pub const MAX_BYTES: u64 = 1 << 30;
const MAX_NAME_LEN: usize = 200;

const RESERVED_DEVICE_NAMES: [&[u8]; 4] = [b"con", b"prn", b"aux", b"nul"];

const CONSENT_EXTENSIONS: [&[u8]; 36] = [
    b"exe",
    b"msi",
    b"msix",
    b"bat",
    b"cmd",
    b"com",
    b"scr",
    b"pif",
    b"ps1",
    b"vbs",
    b"vbe",
    b"js",
    b"jse",
    b"wsf",
    b"wsh",
    b"hta",
    b"lnk",
    b"url",
    b"reg",
    b"dll",
    b"jar",
    b"sh",
    b"bash",
    b"run",
    b"desktop",
    b"appimage",
    b"deb",
    b"rpm",
    b"apk",
    b"dmg",
    b"pkg",
    b"app",
    b"py",
    b"pl",
    b"rb",
    b"iso",
];

pub fn safe_name(name: &[u8]) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = name
        .iter()
        .copied()
        .filter(|&c| {
            c >= 0x20
                && c != 0x7f
                && !matches!(
                    c,
                    b'/' | b'\\' | b':' | b'<' | b'>' | b'"' | b'|' | b'?' | b'*'
                )
        })
        .collect();
    while out.last().is_some_and(|&c| c == b' ' || c == b'.') {
        out.pop();
    }
    while out.first().is_some_and(|&c| c == b' ' || c == b'.') {
        out.remove(0);
    }
    if out.len() > MAX_NAME_LEN {
        out.truncate(MAX_NAME_LEN);
        while !out.is_empty() && core::str::from_utf8(&out).is_err() {
            out.pop();
        }
    }
    let stem = out
        .iter()
        .position(|&c| c == b'.')
        .map_or(&out[..], |dot| &out[..dot]);
    let lowered = stem.to_ascii_lowercase();
    let reserved = RESERVED_DEVICE_NAMES.contains(&&lowered[..])
        || ((lowered.starts_with(b"com") || lowered.starts_with(b"lpt"))
            && lowered.len() == 4
            && lowered[3].is_ascii_digit());
    (!out.is_empty() && !reserved).then_some(out)
}

pub fn strip_query(base: &[u8]) -> &[u8] {
    base.iter()
        .position(|&c| c == b'?' || c == b'#')
        .map_or(base, |q| &base[..q])
}

pub fn extension(name: &[u8]) -> Option<Vec<u8>> {
    let dot = name.iter().rposition(|&c| c == b'.')?;
    (dot > 0 && dot + 1 < name.len()).then(|| name[dot + 1..].to_ascii_lowercase())
}

pub fn needs_consent(name: &[u8]) -> bool {
    extension(name).is_some_and(|ext| CONSENT_EXTENSIONS.contains(&&ext[..]))
}

pub fn numbered(name: &[u8], n: u32) -> Vec<u8> {
    let mut out = name.to_vec();
    out.extend_from_slice(format!(".{n}").as_bytes());
    out
}

pub fn finished_label(name: &[u8], detail: &[u8]) -> Vec<u8> {
    [name, " — ".as_bytes(), detail].concat()
}

pub fn consent_label(name: &[u8], host: &[u8]) -> Vec<u8> {
    [name, " — ".as_bytes(), host].concat()
}

pub fn listed(name: &[u8]) -> bool {
    name.first() != Some(&b'.')
}
