//! Southstar — FTP directory listings (Unix ls -l and DOS dir formats) turned into an "Index of" page.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi::sys;
use crate::listing::{self, Entry};

struct Parsed {
    name: Vec<u8>,
    date: Vec<u8>,
    size: Option<u64>,
    is_dir: bool,
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn strip(s: &[u8]) -> &[u8] {
    let start = s.iter().position(|&c| !is_space(c)).unwrap_or(s.len());
    let end = s
        .iter()
        .rposition(|&c| !is_space(c))
        .map_or(start, |i| i + 1);
    &s[start..end.max(start)]
}

fn split_fields(line: &[u8], max_fields: usize) -> Vec<&[u8]> {
    let mut parts = Vec::new();
    let mut p = 0;
    while p < line.len() && parts.len() + 1 < max_fields {
        while p < line.len() && is_space(line[p]) {
            p += 1;
        }
        if p == line.len() {
            break;
        }
        let start = p;
        while p < line.len() && !is_space(line[p]) {
            p += 1;
        }
        parts.push(&line[start..p]);
    }
    while p < line.len() && is_space(line[p]) {
        p += 1;
    }
    if p < line.len() {
        parts.push(&line[p..]);
    }
    parts
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn real_name(name: &[u8]) -> Option<Vec<u8>> {
    let name = strip(name);
    (!name.is_empty() && name != b"." && name != b"..").then(|| name.to_vec())
}

fn join(parts: &[&[u8]]) -> Vec<u8> {
    parts.join(&b' ')
}

fn parse_unix(line: &[u8]) -> Option<Parsed> {
    let kind = *line.first()?;
    if !matches!(kind, b'd' | b'-' | b'l') {
        return None;
    }
    let tokens = split_fields(line, 9);
    if tokens.len() < 9 || tokens[8].is_empty() {
        return None;
    }
    let mut name = tokens[8];
    if kind == b'l' {
        if let Some(arrow) = find(name, b" -> ") {
            name = &name[..arrow];
        }
    }
    Some(Parsed {
        name: real_name(name)?,
        date: join(&tokens[5..8]),
        size: Some(sys::ascii_strtoull(tokens[4])),
        is_dir: kind == b'd',
    })
}

fn parse_dos(line: &[u8]) -> Option<Parsed> {
    let tokens = split_fields(line, 4);
    if tokens.len() < 4 || tokens[3].is_empty() {
        return None;
    }
    if !tokens[0].contains(&b'-') || !tokens[1].contains(&b':') {
        return None;
    }
    let is_dir = tokens[2].eq_ignore_ascii_case(b"<DIR>");
    Some(Parsed {
        name: real_name(tokens[3])?,
        date: join(&tokens[..2]),
        size: (!is_dir).then(|| sys::ascii_strtoull(tokens[2])),
        is_dir,
    })
}

fn parse_line(line: &[u8]) -> Option<Parsed> {
    let line = strip(line);
    if line.is_empty() {
        return None;
    }
    Some(
        parse_unix(line)
            .or_else(|| parse_dos(line))
            .unwrap_or_else(|| Parsed {
                name: line.to_vec(),
                date: Vec::new(),
                size: None,
                is_dir: false,
            }),
    )
}

fn without_query_and_fragment(url: &[u8]) -> &[u8] {
    let url = &url[..url.iter().position(|&c| c == b'#').unwrap_or(url.len())];
    &url[..url.iter().position(|&c| c == b'?').unwrap_or(url.len())]
}

fn escape_segment(name: &[u8], out: &mut Vec<u8>) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for &c in name {
        if c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_' | b'~') {
            out.push(c);
        } else {
            out.extend_from_slice(&[b'%', HEX[usize::from(c >> 4)], HEX[usize::from(c & 15)]]);
        }
    }
}

fn child_uri(base_url: &[u8], name: &[u8], is_dir: bool) -> Option<Vec<u8>> {
    if base_url.is_empty() || name.is_empty() {
        return None;
    }
    let mut out = without_query_and_fragment(base_url).to_vec();
    if !out.ends_with(b"/") {
        out.push(b'/');
    }
    escape_segment(name, &mut out);
    if is_dir {
        out.push(b'/');
    }
    Some(out)
}

fn parent_uri(url: &[u8]) -> Option<Vec<u8>> {
    if url.is_empty() {
        return None;
    }
    let mut base = without_query_and_fragment(url);
    while let [rest @ .., b'/'] = base {
        base = rest;
    }
    let path = find(base, b"://").and_then(|scheme| {
        let host = scheme + 3;
        base[host..]
            .iter()
            .position(|&c| c == b'/')
            .map(|i| host + i)
    })?;
    if path + 1 >= base.len() {
        return None;
    }
    let cut = match base[path + 1..].iter().rposition(|&c| c == b'/') {
        Some(last) => path + 1 + last + 1,
        None => path + 1,
    };
    Some(base[..cut].to_vec())
}

pub fn looks_like_directory(url: Option<&[u8]>) -> bool {
    let Some(url) = url.filter(|u| u.starts_with(b"ftp://")) else {
        return false;
    };
    match sys::url_pathname(url) {
        Some(path) => path.is_empty() || path.ends_with(b"/"),
        None => url.ends_with(b"/"),
    }
}

pub fn directory_page(url: Option<&[u8]>, body: &[u8]) -> Vec<u8> {
    let listing = &body[..body.iter().position(|&c| c == 0).unwrap_or(body.len())];
    let mut entries = Vec::new();
    for line in listing.split(|&c| c == b'\n') {
        let Some(parsed) = parse_line(line) else {
            continue;
        };
        let Some(uri) = child_uri(url.unwrap_or_default(), &parsed.name, parsed.is_dir) else {
            continue;
        };
        let size = match parsed.size {
            Some(size) if !parsed.is_dir => listing::size_label(size),
            _ => Vec::new(),
        };
        entries.push(Entry {
            display: parsed.name,
            uri,
            size,
            date: parsed.date,
            is_dir: parsed.is_dir,
        });
    }
    listing::sort(&mut entries);
    let parent = url.and_then(parent_uri);
    listing::page(url.unwrap_or(b"ftp://"), parent.as_deref(), &entries)
}

pub fn guess_path(url: &[u8]) -> Vec<u8> {
    sys::url_pathname(url).unwrap_or_else(|| url.to_vec())
}
