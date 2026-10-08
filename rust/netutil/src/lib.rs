//! Southstar — curl-free network helpers shared by the engine and the shells: Accept-Language, search URLs, local paths, proxy masking.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod ffi;

use std::collections::HashSet;

const DEFAULT_SEARCH_ENGINE: &[u8] = b"https://duckduckgo.com/?q=%s";

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

pub fn accept_language_for_windows_locale(locale: &[u8]) -> Vec<u8> {
    let base = locale
        .iter()
        .position(|&c| c == b'-')
        .map(|dash| &locale[..dash]);
    let is = |name: &str| base.is_some_and(|b| b.eq_ignore_ascii_case(name.as_bytes()));
    let mut out = locale.to_vec();
    if let Some(base) = base.filter(|b| !b.eq_ignore_ascii_case(locale)) {
        out.extend_from_slice(b",");
        out.extend_from_slice(base);
        out.extend_from_slice(b";q=0.9");
    }
    if is("nb") {
        out.extend_from_slice(b",no;q=0.8,nn;q=0.7");
    } else if is("nn") {
        out.extend_from_slice(b",no;q=0.8,nb;q=0.7");
    } else if is("no") {
        out.extend_from_slice(b",nb;q=0.8,nn;q=0.7");
    }
    if !locale
        .get(..2)
        .is_some_and(|p| p.eq_ignore_ascii_case(b"en"))
    {
        out.extend_from_slice(if is("nb") || is("nn") || is("no") {
            b",en-US;q=0.6,en;q=0.5"
        } else {
            b",en-US;q=0.8,en;q=0.7"
        });
    }
    out
}

pub fn accept_language_from_language_names(names: &[Vec<u8>]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut n = 0;
    for name in names {
        if n >= 6 {
            break;
        }
        let mut tag = name.as_slice();
        if let Some(dot) = tag.iter().position(|&c| c == b'.') {
            tag = &tag[..dot];
        }
        if let Some(at) = tag.iter().position(|&c| c == b'@') {
            tag = &tag[..at];
        }
        let tag: Vec<u8> = tag
            .iter()
            .map(|&c| if c == b'_' { b'-' } else { c })
            .collect();
        if tag.is_empty() || tag.eq_ignore_ascii_case(b"C") || tag.eq_ignore_ascii_case(b"POSIX") {
            continue;
        }
        if !seen.insert(tag.to_ascii_lowercase()) {
            continue;
        }
        if n == 0 {
            out.extend_from_slice(&tag);
        } else {
            let q = (1.0 - f64::from(n) * 0.1).max(0.1);
            out.push(b',');
            out.extend_from_slice(&tag);
            out.extend_from_slice(format!(";q={q:.1}").as_bytes());
        }
        n += 1;
    }
    (n > 0).then_some(out)
}

pub fn navigator_languages(accept_language: &[u8]) -> Vec<Vec<u8>> {
    let mut langs = Vec::new();
    let mut seen = HashSet::new();
    for part in accept_language.split(|&c| c == b',') {
        if langs.len() >= 20 {
            break;
        }
        let mut part = part.trim_ascii();
        if let Some(semi) = part.iter().position(|&c| c == b';') {
            part = part[..semi].trim_ascii();
        }
        if part.is_empty() || part == b"*" {
            continue;
        }
        if !part.iter().all(|&c| c.is_ascii_alphanumeric() || c == b'-') {
            continue;
        }
        if !seen.insert(part.to_ascii_lowercase()) {
            continue;
        }
        langs.push(part.to_vec());
    }
    if langs.is_empty() {
        langs.push(b"en-US".to_vec());
    }
    langs
}

fn special_scheme(s: &[u8]) -> bool {
    s.starts_with(b"about:")
        || s.starts_with(b"file:")
        || s.starts_with(b"data:")
        || contains(s, b"://")
}

pub fn address_is_search(s: &[u8]) -> bool {
    if s.is_empty() || special_scheme(s) {
        return false;
    }
    if s.iter().any(|&c| c == b' ' || c == b'\t') || contains(s, b"\xe3\x80\x80") {
        return true;
    }
    if s.starts_with(b"localhost") && matches!(s.get(9), None | Some(b':') | Some(b'/')) {
        return false;
    }
    if url_from_local_path(s).is_some() {
        return false;
    }
    !(s.contains(&b'.') || s.contains(&b':'))
}

pub fn search_url_for(query: &[u8]) -> Vec<u8> {
    let configured = ffi::configured_search_engine().filter(|e| !e.is_empty());
    let engine = configured.as_deref().unwrap_or(DEFAULT_SEARCH_ENGINE);
    let encoded = ffi::uri_escape(query);
    match find(engine, b"%s") {
        Some(pct) => [&engine[..pct], &encoded, &engine[pct + 2..]].concat(),
        None => [engine, &encoded].concat(),
    }
}

pub fn url_from_local_path(path: &[u8]) -> Option<Vec<u8>> {
    if path.is_empty() || special_scheme(path) {
        return None;
    }
    let mut candidate = None;
    if cfg!(windows) && path.len() == 2 && path[0].is_ascii_alphabetic() && path[1] == b':' {
        candidate = Some(vec![path[0], b':', b'\\']);
    }
    let mut candidate = match candidate {
        Some(root) => root,
        None => {
            if !ffi::file_exists(path) {
                return None;
            }
            ffi::canonicalize_filename(path)?
        }
    };
    let separator = if cfg!(windows) { b'\\' } else { b'/' };
    if ffi::is_dir(&candidate) && candidate.last() != Some(&separator) {
        candidate.push(separator);
    }
    ffi::filename_to_uri(&candidate)
}

pub fn proxy_mask(proxy_url: &[u8]) -> Vec<u8> {
    if proxy_url.is_empty() {
        return Vec::new();
    }
    let cursor = find(proxy_url, b"://").map_or(0, |sep| sep + 3);
    let Some(at) = proxy_url[cursor..]
        .iter()
        .position(|&c| c == b'@')
        .map(|i| cursor + i)
    else {
        return proxy_url.to_vec();
    };
    let Some(colon) = proxy_url[cursor..at]
        .iter()
        .position(|&c| c == b':')
        .map(|i| cursor + i)
    else {
        return proxy_url.to_vec();
    };
    [&proxy_url[..colon], b":***", &proxy_url[at..]].concat()
}
