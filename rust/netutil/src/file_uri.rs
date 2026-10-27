//! Southstar — local file names and file: URIs with GLib's rules: canonical absolute names, URI escaping and the conversions both ways.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_config::util;

const HEX: &[u8; 16] = b"0123456789ABCDEF";
const SEPARATOR: u8 = if cfg!(windows) { b'\\' } else { b'/' };

fn until_nul(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}

fn is_separator(byte: u8) -> bool {
    byte == b'/' || (cfg!(windows) && byte == b'\\')
}

fn has_drive_root(path: &[u8]) -> bool {
    cfg!(windows)
        && path.len() >= 3
        && path[0].is_ascii_alphabetic()
        && path[1] == b':'
        && is_separator(path[2])
}

fn push_escaped(out: &mut Vec<u8>, byte: u8) {
    out.extend_from_slice(&[
        b'%',
        HEX[usize::from(byte >> 4)],
        HEX[usize::from(byte & 15)],
    ]);
}

pub fn uri_escape(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for chunk in until_nul(text).utf8_chunks() {
        for c in chunk.valid().chars() {
            let mut buf = [0; 4];
            let encoded = c.encode_utf8(&mut buf).as_bytes();
            match encoded {
                [b] if b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'.' | b'_' | b'~') => {
                    out.push(*b)
                }
                [b] => push_escaped(&mut out, *b),
                _ => out.extend_from_slice(encoded),
            }
        }
        for &b in chunk.invalid() {
            push_escaped(&mut out, b);
        }
    }
    out
}

pub fn is_absolute(path: &[u8]) -> bool {
    path.first().is_some_and(|&b| is_separator(b)) || has_drive_root(path)
}

fn skip_unc_root(path: &[u8]) -> Option<usize> {
    if !cfg!(windows)
        || path.len() < 3
        || !is_separator(path[0])
        || !is_separator(path[1])
        || is_separator(path[2])
    {
        return None;
    }
    let server_end = 2 + path[2..].iter().position(|&b| is_separator(b))?;
    if server_end + 1 >= path.len() {
        return None;
    }
    let mut end = server_end + 1;
    while end < path.len() && !is_separator(path[end]) {
        end += 1;
    }
    if end < path.len() {
        end += 1;
    }
    Some(end)
}

fn skip_root(path: &[u8]) -> Option<usize> {
    if let Some(end) = skip_unc_root(path) {
        return Some(end);
    }
    if path.first().is_some_and(|&b| is_separator(b)) {
        return Some(path.iter().take_while(|&&b| is_separator(b)).count());
    }
    has_drive_root(path).then_some(3)
}

fn current_dir() -> Vec<u8> {
    std::env::current_dir()
        .map(|dir| util::path_bytes(&dir))
        .unwrap_or_else(|_| vec![SEPARATOR])
}

pub fn canonicalize_filename(path: &[u8]) -> Vec<u8> {
    let path = until_nul(path);
    let absolute = if is_absolute(path) {
        path.to_vec()
    } else {
        util::build_filename(&[&current_dir(), path])
    };
    let Some(after_root) = skip_root(&absolute) else {
        return util::build_filename(&[&[SEPARATOR], path]);
    };
    let run_start = absolute[..after_root]
        .iter()
        .rposition(|&b| !is_separator(b))
        .map_or(0, |i| i + 1);
    let mut out = absolute[..run_start].to_vec();
    match after_root - run_start {
        0 => {}
        2 => out.extend_from_slice(&[SEPARATOR, SEPARATOR]),
        _ => out.push(SEPARATOR),
    }
    let mut components: Vec<&[u8]> = Vec::new();
    for component in absolute[after_root..].split(|&b| is_separator(b)) {
        match component {
            b"" | b"." => {}
            b".." => {
                components.pop();
            }
            _ => components.push(component),
        }
    }
    for (i, component) in components.iter().enumerate() {
        if i > 0 {
            out.push(SEPARATOR);
        }
        out.extend_from_slice(component);
    }
    out
}

fn path_byte_is_safe(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!$&'()*+,-./:=@_~".contains(&byte)
}

pub fn filename_to_uri(path: &[u8]) -> Option<Vec<u8>> {
    let path = until_nul(path);
    if !is_absolute(path) {
        return None;
    }
    let mut out = b"file://".to_vec();
    let first = if cfg!(windows) && path[0] == b'\\' {
        b'/'
    } else {
        path[0]
    };
    if first != b'/' {
        out.push(b'/');
    }
    for &byte in path {
        let byte = if cfg!(windows) && byte == b'\\' {
            b'/'
        } else {
            byte
        };
        if path_byte_is_safe(byte) {
            out.push(byte);
        } else {
            push_escaped(&mut out, byte);
        }
    }
    Some(out)
}

fn hex_value(byte: u8) -> Option<u8> {
    char::from(byte)
        .to_digit(16)
        .and_then(|digit| u8::try_from(digit).ok())
}

fn unescape_segment(segment: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(segment.len());
    let mut i = 0;
    while i < segment.len() {
        if segment[i] != b'%' {
            out.push(segment[i]);
            i += 1;
            continue;
        }
        let high = hex_value(*segment.get(i + 1)?)?;
        let low = hex_value(*segment.get(i + 2)?)?;
        let byte = high << 4 | low;
        if byte == 0 || byte == b'/' {
            return None;
        }
        out.push(byte);
        i += 3;
    }
    Some(out)
}

fn hostname_is_valid(host: &[u8]) -> bool {
    host.is_empty()
        || host.split(|&b| b == b'.').all(|label| {
            label.first().is_some_and(u8::is_ascii_alphanumeric)
                && label.last().is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .iter()
                    .all(|&b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

pub fn filename_from_uri(uri: &[u8]) -> Option<Vec<u8>> {
    let uri = until_nul(uri);
    if uri.len() < 6 || !uri[..6].eq_ignore_ascii_case(b"file:/") || uri.contains(&b'#') {
        return None;
    }
    let mut path = &uri[5..];
    if path.starts_with(b"///") {
        path = &path[2..];
    } else if path.starts_with(b"//") {
        let rest = &path[2..];
        let slash = rest.iter().position(|&b| b == b'/')?;
        let host = unescape_segment(&rest[..slash])?;
        if !hostname_is_valid(&host) {
            return None;
        }
        path = &rest[slash..];
    }
    let mut filename = unescape_segment(path)?;
    if cfg!(windows) {
        for byte in &mut filename {
            if *byte == b'/' {
                *byte = b'\\';
            }
        }
        if filename.len() > 2 && filename[1].is_ascii_alphabetic() {
            if filename[2] == b'|' {
                filename[2] = b':';
            }
            if filename[2] == b':' {
                filename.remove(0);
            }
        }
    }
    Some(filename)
}
