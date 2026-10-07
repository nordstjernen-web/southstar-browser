//! Southstar — bookmarks storage: parsing and writing the tab-separated bookmarks file.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

fn strip(line: &[u8]) -> &[u8] {
    let start = line
        .iter()
        .position(|c| !c.is_ascii_whitespace())
        .unwrap_or(line.len());
    let end = line
        .iter()
        .rposition(|c| !c.is_ascii_whitespace())
        .map_or(start, |i| i + 1);
    &line[start..end.max(start)]
}

pub fn parse(contents: &[u8]) -> Vec<(&[u8], &[u8])> {
    let text = contents.split(|&c| c == 0).next().unwrap_or_default();
    text.split(|&c| c == b'\n')
        .map(strip)
        .filter(|line| !line.is_empty() && line[0] != b'#')
        .map(|line| match line.iter().position(|&c| c == b'\t') {
            Some(tab) => (&line[..tab], &line[tab + 1..]),
            None => (line, line),
        })
        .collect()
}

fn append_sanitized(out: &mut Vec<u8>, field: &[u8]) {
    out.extend(field.iter().map(|&c| match c {
        b'\t' | b'\n' | b'\r' => b' ',
        c => c,
    }));
}

pub fn serialize<'a>(items: impl Iterator<Item = (Option<&'a [u8]>, Option<&'a [u8]>)>) -> Vec<u8> {
    let mut out = Vec::new();
    for (url, title) in items {
        append_sanitized(&mut out, url.unwrap_or_default());
        out.push(b'\t');
        append_sanitized(&mut out, title.unwrap_or_default());
        out.push(b'\n');
    }
    out
}
