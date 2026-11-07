//! Southstar — Safe Browsing URL canonicalisation and the host-suffix/path-prefix expressions whose hashes are looked up.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashSet;

struct Canonical {
    host: Vec<u8>,
    path: Vec<u8>,
    query: Option<Vec<u8>>,
    is_ip: bool,
}

fn hex_value(c: u8) -> Option<u32> {
    char::from(c).to_digit(16)
}

fn unescape_once(s: &[u8]) -> (Vec<u8>, bool) {
    let mut out = Vec::with_capacity(s.len());
    let mut changed = false;
    let mut i = 0;
    while i < s.len() {
        if s[i] == b'%'
            && let (Some(hi), Some(lo)) = (
                s.get(i + 1).copied().and_then(hex_value),
                s.get(i + 2).copied().and_then(hex_value),
            )
            && (hi != 0 || lo != 0)
        {
            out.push((hi * 16 + lo) as u8);
            i += 3;
            changed = true;
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    (out, changed)
}

fn escape(s: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for &c in s {
        if c <= 0x20 || c >= 0x7f || c == b'#' || c == b'%' {
            out.extend_from_slice(format!("%{c:02X}").as_bytes());
        } else {
            out.push(c);
        }
    }
    out
}

fn ipv4_part(s: &[u8]) -> Option<u64> {
    let (base, digits) = match s {
        [] => return None,
        [b'0', b'x' | b'X', rest @ ..] => (16, rest),
        [b'0', rest @ ..] if !rest.is_empty() => (8, rest),
        _ => (10, s),
    };
    let mut value = 0u64;
    for &c in digits {
        let digit = hex_value(c).filter(|&d| d < base)?;
        value = value * u64::from(base) + u64::from(digit);
        if value > u64::from(u32::MAX) {
            return None;
        }
    }
    Some(value)
}

fn ipv4_canonical(host: &[u8]) -> Option<Vec<u8>> {
    let parts: Vec<&[u8]> = host.split(|&c| c == b'.').collect();
    if parts.is_empty() || parts.len() > 4 {
        return None;
    }
    let values: Vec<u64> = parts
        .iter()
        .map(|part| ipv4_part(part))
        .collect::<Option<_>>()?;
    let (last, leading) = values.split_last()?;
    let mut addr = 0u64;
    for (i, &value) in leading.iter().enumerate() {
        if value > 255 {
            return None;
        }
        addr |= value << (24 - 8 * i);
    }
    if *last > u64::from(u32::MAX >> (8 * leading.len())) {
        return None;
    }
    addr |= last;
    Some(
        format!(
            "{}.{}.{}.{}",
            (addr >> 24) & 255,
            (addr >> 16) & 255,
            (addr >> 8) & 255,
            addr & 255
        )
        .into_bytes(),
    )
}

fn canonical_host(raw: &[u8]) -> (Vec<u8>, bool) {
    let mut host = Vec::with_capacity(raw.len());
    for &c in raw {
        if c == b'.' && host.last().is_none_or(|&last| last == b'.') {
            continue;
        }
        host.push(c.to_ascii_lowercase());
    }
    while host.last() == Some(&b'.') {
        host.pop();
    }
    match (!host.is_empty()).then(|| ipv4_canonical(&host)).flatten() {
        Some(ip) => (ip, true),
        None => {
            let bracketed = host.first() == Some(&b'[');
            (host, bracketed)
        }
    }
}

fn canonical_path(raw: &[u8]) -> Vec<u8> {
    let segments: Vec<&[u8]> = raw.split(|&c| c == b'/').collect();
    let mut keep: Vec<&[u8]> = Vec::new();
    let mut trailing = false;
    for (i, &segment) in segments.iter().enumerate() {
        let last = i + 1 == segments.len();
        match segment {
            b"" | b"." => {}
            b".." => {
                keep.pop();
            }
            _ => {
                keep.push(segment);
                continue;
            }
        }
        trailing |= last;
    }
    let mut out = b"/".to_vec();
    out.extend_from_slice(&keep.join(&b'/'));
    if trailing && !keep.is_empty() {
        out.push(b'/');
    }
    out
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn canonicalize(url: &[u8]) -> Option<Canonical> {
    let clean: Vec<u8> = url
        .iter()
        .copied()
        .filter(|&c| !matches!(c, b'\t' | b'\r' | b'\n'))
        .collect();
    let clean = clean.trim_ascii();
    let mut current =
        clean[..clean.iter().position(|&c| c == b'#').unwrap_or(clean.len())].to_vec();
    for _ in 0..32 {
        let (next, changed) = unescape_once(&current);
        current = next;
        if !changed {
            break;
        }
    }
    let rest = match find(&current, b"://") {
        Some(at) => &current[at + 3..],
        None => &current[..],
    };
    let auth_len = rest
        .iter()
        .position(|&c| c == b'/' || c == b'?')
        .unwrap_or(rest.len());
    let (auth, after) = rest.split_at(auth_len);
    let hostport = match auth.iter().rposition(|&c| c == b'@') {
        Some(at) => &auth[at + 1..],
        None => auth,
    };
    let hostport = if hostport.first() == Some(&b'[') {
        match hostport.iter().position(|&c| c == b']') {
            Some(close) => &hostport[..=close],
            None => hostport,
        }
    } else {
        match hostport.iter().rposition(|&c| c == b':') {
            Some(colon) => &hostport[..colon],
            None => hostport,
        }
    };
    let (host, is_ip) = canonical_host(hostport);
    if host.is_empty() {
        return None;
    }
    let (raw_path, query) = match after.iter().position(|&c| c == b'?') {
        Some(q) => (&after[..q], Some(escape(&after[q + 1..]))),
        None => (after, None),
    };
    Some(Canonical {
        host: escape(&host),
        path: escape(&canonical_path(raw_path)),
        query,
        is_ip,
    })
}

pub(crate) fn expressions(url: &[u8]) -> Vec<Vec<u8>> {
    let Some(c) = canonicalize(url) else {
        return Vec::new();
    };
    let mut hosts = vec![c.host.clone()];
    if !c.is_ip {
        let parts: Vec<&[u8]> = c.host.split(|&b| b == b'.').collect();
        let start = if parts.len() > 5 { parts.len() - 5 } else { 1 };
        for i in start..parts.len().saturating_sub(1) {
            hosts.push(parts[i..].join(&b'.'));
        }
    }
    let mut paths = Vec::new();
    if let Some(query) = &c.query {
        paths.push([&c.path[..], b"?", query].concat());
    }
    paths.push(c.path.clone());
    let mut prefix = b"/".to_vec();
    let mut rest = &c.path[1..];
    for _ in 0..4 {
        if prefix != c.path {
            paths.push(prefix.clone());
        }
        let Some(slash) = rest.iter().position(|&b| b == b'/') else {
            break;
        };
        prefix.extend_from_slice(&rest[..=slash]);
        rest = &rest[slash + 1..];
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for host in &hosts {
        for path in &paths {
            let expression = [&host[..], path].concat();
            if seen.insert(expression.clone()) {
                out.push(expression);
            }
        }
    }
    out
}
