//! Southstar — naming downloaded files: the suggested or URL-derived name made safe for one path component, its numbered alternatives, and which file types wait for the user before they are saved.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub const FALLBACK_NAME: &[u8] = b"download";
pub const MAX_ALTERNATIVES: u32 = 1000;
pub const RECENT_SCAN_LIMIT: usize = 200;
pub const RECENT_SHOWN_LIMIT: usize = 25;
pub const MAX_BYTES: u64 = 1 << 30;
const MAX_NAME_LEN: usize = 200;

const RESERVED_DEVICE_NAMES: [&[u8]; 7] = [
    b"con", b"prn", b"aux", b"nul", b"conin$", b"conout$", b"clock$",
];

const CONSENT_EXTENSIONS: [&[u8]; 82] = [
    b"exe",
    b"msi",
    b"msix",
    b"msixbundle",
    b"appx",
    b"appxbundle",
    b"msp",
    b"mst",
    b"bat",
    b"cmd",
    b"com",
    b"scr",
    b"pif",
    b"cpl",
    b"msc",
    b"ps1",
    b"psm1",
    b"psd1",
    b"ps1xml",
    b"vb",
    b"vbs",
    b"vbe",
    b"js",
    b"jse",
    b"wsf",
    b"wsh",
    b"wsc",
    b"sct",
    b"hta",
    b"chm",
    b"hlp",
    b"inf",
    b"ins",
    b"isp",
    b"scf",
    b"lnk",
    b"url",
    b"reg",
    b"dll",
    b"ocx",
    b"sys",
    b"xll",
    b"jar",
    b"jnlp",
    b"application",
    b"appref-ms",
    b"gadget",
    b"settingcontent-ms",
    b"library-ms",
    b"search-ms",
    b"searchconnector-ms",
    b"diagcab",
    b"cab",
    b"vhd",
    b"vhdx",
    b"img",
    b"iso",
    b"sh",
    b"bash",
    b"csh",
    b"ksh",
    b"zsh",
    b"command",
    b"tool",
    b"terminal",
    b"workflow",
    b"run",
    b"desktop",
    b"appimage",
    b"flatpakref",
    b"snap",
    b"deb",
    b"rpm",
    b"apk",
    b"dmg",
    b"pkg",
    b"mpkg",
    b"app",
    b"py",
    b"pyw",
    b"pl",
    b"rb",
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
    let lowered = stem.trim_ascii_end().to_ascii_lowercase();
    let reserved = RESERVED_DEVICE_NAMES.contains(&&lowered[..])
        || ((lowered.starts_with(b"com") || lowered.starts_with(b"lpt"))
            && is_port_number(&lowered[3..]));
    (!out.is_empty() && !reserved).then_some(out)
}

fn is_port_number(rest: &[u8]) -> bool {
    matches!(rest, [b'0'..=b'9'])
        || ["\u{b9}", "\u{b2}", "\u{b3}"]
            .iter()
            .any(|d| rest == d.as_bytes())
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
