//! Southstar — URL helpers over rust/url's WHATWG parser: resolving, the URL setters, origins, sites, hosts, components, referrers, tracking-parameter stripping, Refresh parsing and the user agent strings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

pub use southstar_url::bytes::{
    Parts, host_from, is_http_or_https, is_valid_absolute, origin_from, origin_from_any, parts,
    resolve, same_origin, set_component, to_ascii,
};

use crate::ffi::host;

const TRACKING_PARAMS: [&str; 41] = [
    "gclid",
    "dclid",
    "gbraid",
    "wbraid",
    "fbclid",
    "msclkid",
    "yclid",
    "twclid",
    "igshid",
    "mc_cid",
    "mc_eid",
    "_hsenc",
    "_hsmi",
    "__hssc",
    "__hstc",
    "__hsfp",
    "hsctatracking",
    "vero_id",
    "vero_conv",
    "oly_anon_id",
    "oly_enc_id",
    "_openstat",
    "wickedid",
    "rb_clickid",
    "s_cid",
    "ml_subscriber",
    "ml_subscriber_hash",
    "mtm_source",
    "mtm_medium",
    "mtm_campaign",
    "mtm_keyword",
    "mtm_cid",
    "mtm_content",
    "mtm_group",
    "mtm_placement",
    "pk_source",
    "pk_medium",
    "pk_campaign",
    "pk_keyword",
    "pk_cid",
    "pk_content",
];

pub const REFERRER_NONE: i32 = 0;
pub const REFERRER_SAME_ORIGIN: i32 = 1;
pub const REFERRER_UNSAFE_URL: i32 = 3;

#[cfg(windows)]
macro_rules! platform {
    (token) => {
        "Windows NT 10.0; Win64; x64"
    };
    (navigator) => {
        "Win32"
    };
    (hint) => {
        "Windows"
    };
}

#[cfg(target_vendor = "apple")]
macro_rules! platform {
    (token) => {
        "Macintosh; Intel Mac OS X 10_15_7"
    };
    (navigator) => {
        "MacIntel"
    };
    (hint) => {
        "macOS"
    };
}

#[cfg(not(any(windows, target_vendor = "apple")))]
macro_rules! platform {
    (token) => {
        "X11; Linux x86_64"
    };
    (navigator) => {
        "Linux x86_64"
    };
    (hint) => {
        "Linux"
    };
}

macro_rules! chrome_ua {
    ($product:expr) => {
        concat!(
            "Mozilla/5.0 (",
            platform!(token),
            ") AppleWebKit/537.36 (KHTML, like Gecko) Chrome/150.0.0.0 Safari/537.36 ",
            $product,
            "\0"
        )
    };
}

pub const USER_AGENT: &str = chrome_ua!("Southstar/1.0");
pub const USER_AGENT_LADYBIRD: &str = chrome_ua!("Ladybird/1.0");
pub const USER_AGENT_FIREFOX: &str = concat!(
    "Mozilla/5.0 (",
    platform!(token),
    "; rv:143.0) Gecko/20100101 Firefox/143.0\0"
);
pub const NAVIGATOR_PLATFORM: &str = concat!(platform!(navigator), "\0");
pub const HINT_PLATFORM: &str = concat!(platform!(hint), "\0");

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

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn is_tracking(key: &[u8]) -> bool {
    if key.is_empty() {
        return false;
    }
    if key.len() >= 4 && key[..4].eq_ignore_ascii_case(b"utm_") {
        return true;
    }
    TRACKING_PARAMS
        .iter()
        .any(|param| param.as_bytes().eq_ignore_ascii_case(key))
}

pub fn strip_tracking_params(url: &[u8], enabled: bool) -> Option<Vec<u8>> {
    if !is_http_or_https(url) || !enabled {
        return None;
    }
    let frag = url.iter().position(|&c| c == b'#');
    let query_end = frag.unwrap_or(url.len());
    let query = url
        .iter()
        .position(|&c| c == b'?')
        .filter(|&q| q < query_end)?;
    let mut kept = Vec::new();
    let mut removed = false;
    for token in url[query + 1..query_end].split(|&c| c == b'&') {
        let key = &token[..token.iter().position(|&c| c == b'=').unwrap_or(token.len())];
        if is_tracking(key) {
            removed = true;
        } else {
            if !kept.is_empty() {
                kept.push(b'&');
            }
            kept.extend_from_slice(token);
        }
    }
    if !removed {
        return None;
    }
    let mut out = url[..query].to_vec();
    if !kept.is_empty() {
        out.push(b'?');
        out.extend_from_slice(&kept);
    }
    if let Some(frag) = frag {
        out.extend_from_slice(&url[frag..]);
    }
    Some(out)
}

pub fn parse_refresh(input: &[u8]) -> Option<(f64, Option<Vec<u8>>)> {
    let at = |i: usize| input.get(i).copied().unwrap_or(0);
    let mut p = 0;
    while is_space(at(p)) {
        p += 1;
    }
    let digits = p;
    while at(p).is_ascii_digit() {
        p += 1;
    }
    if p == digits && at(p) != b'.' {
        return None;
    }
    let mut seconds = 0.0f64;
    for &d in &input[digits..p] {
        seconds = seconds * 10.0 + f64::from(d - b'0');
        if seconds > 2147483647.0 {
            seconds = 2147483647.0;
            break;
        }
    }
    while at(p) == b'.' || at(p).is_ascii_digit() {
        p += 1;
    }
    if at(p) != 0 {
        if at(p) != b';' && at(p) != b',' && !is_space(at(p)) {
            return None;
        }
        while is_space(at(p)) {
            p += 1;
        }
        if at(p) == b';' || at(p) == b',' {
            p += 1;
        }
        while is_space(at(p)) {
            p += 1;
        }
    }
    if at(p).eq_ignore_ascii_case(&b'u')
        && at(p + 1).eq_ignore_ascii_case(&b'r')
        && at(p + 2).eq_ignore_ascii_case(&b'l')
    {
        let mut q = p + 3;
        while is_space(at(q)) {
            q += 1;
        }
        if at(q) == b'=' {
            q += 1;
            while is_space(at(q)) {
                q += 1;
            }
            p = q;
        }
    }
    let rest = &input[p.min(input.len())..];
    let url = match rest.first() {
        Some(&quote) if quote == b'\'' || quote == b'"' => {
            let body = &rest[1..];
            Some(body[..body.iter().position(|&c| c == quote).unwrap_or(body.len())].to_vec())
        }
        Some(_) => {
            let end = rest
                .iter()
                .rposition(|&c| !is_space(c))
                .map_or(0, |i| i + 1);
            Some(rest[..end].to_vec())
        }
        None => None,
    };
    Some((seconds, url.filter(|u| !u.is_empty())))
}

pub fn referer_for(url: Option<&[u8]>, top_url: Option<&[u8]>, policy: i32) -> Option<Vec<u8>> {
    let top = top_url.filter(|t| !t.is_empty())?;
    let url = url.filter(|u| is_http_or_https(u))?;
    if !is_http_or_https(top) || policy == REFERRER_NONE {
        return None;
    }
    if top.starts_with(b"https://") && url.starts_with(b"http://") {
        return None;
    }
    let same = same_origin(top, url);
    if policy == REFERRER_SAME_ORIGIN && !same {
        return None;
    }
    if policy == REFERRER_UNSAFE_URL || same {
        let p = parts(top).filter(|p| !p.origin.is_empty())?;
        return Some([&p.origin[..], &p.pathname, &p.search].concat());
    }
    let mut out = origin_from(top)?;
    out.push(b'/');
    Some(out)
}

pub fn site_from(url: &[u8]) -> Option<Vec<u8>> {
    if url.is_empty() {
        return None;
    }
    let p = parts(url)?;
    let registrable = host::registrable_domain(&p.hostname).filter(|r| !r.is_empty());
    let site_host = registrable.as_deref().unwrap_or(&p.hostname);
    let mut out = [&p.protocol[..], b"://", site_host].concat();
    if !p.port.is_empty() {
        out.push(b':');
        out.extend_from_slice(&p.port);
    }
    Some(out)
}

fn is_subdomain_of(a: &[u8], b: &[u8]) -> bool {
    a.len() > b.len() + 1
        && a[a.len() - b.len() - 1] == b'.'
        && a[a.len() - b.len()..].eq_ignore_ascii_case(b)
}

pub fn is_same_site(a: &[u8], b: &[u8]) -> bool {
    if let (Some(sa), Some(sb)) = (site_from(a), site_from(b)) {
        return sa.eq_ignore_ascii_case(&sb);
    }
    let (Some(ha), Some(hb)) = (host_from(a), host_from(b)) else {
        return false;
    };
    ha.eq_ignore_ascii_case(&hb) || is_subdomain_of(&ha, &hb) || is_subdomain_of(&hb, &ha)
}

pub fn raw_header_values(raw: &[u8], name: &[u8]) -> Option<Vec<u8>> {
    let n = name.len();
    let mut out: Option<Vec<u8>> = None;
    let mut rest = raw;
    while !rest.is_empty() {
        let eol = rest.iter().position(|&c| c == b'\n');
        let line = &rest[..eol.unwrap_or(rest.len())];
        if line.len() > n && line[n] == b':' && line[..n].eq_ignore_ascii_case(name) {
            let value = strip(&line[n + 1..]);
            match out.as_mut() {
                None => out = Some(value.to_vec()),
                Some(out) => {
                    out.push(b',');
                    out.extend_from_slice(value);
                }
            }
        }
        match eol {
            Some(eol) => rest = &rest[eol + 1..],
            None => break,
        }
    }
    out
}

fn is_loopback(host: &[u8]) -> bool {
    host.eq_ignore_ascii_case(b"localhost")
        || host.ends_with(b".localhost")
        || host == b"127.0.0.1"
        || host == b"::1"
        || host == b"[::1]"
}

pub fn https_first_upgrade(url: &[u8], enabled: bool) -> Option<Vec<u8>> {
    if !enabled {
        return None;
    }
    let rest = url.strip_prefix(b"http://")?;
    let host = host_from(url)?;
    if is_loopback(&host) {
        return None;
    }
    Some([&b"https://"[..], rest].concat())
}

pub fn is_nosniff(value: &[u8]) -> bool {
    value
        .split(|&c| c == b',' || c == b' ' || c == b'\t')
        .any(|token| token.eq_ignore_ascii_case(b"nosniff"))
}

pub fn user_agent_for_mode(compat_mode: Option<&[u8]>) -> &'static str {
    match compat_mode {
        Some(mode) if mode.eq_ignore_ascii_case(b"ladybird") => USER_AGENT_LADYBIRD,
        Some(mode) if mode.eq_ignore_ascii_case(b"firefox") => USER_AGENT_FIREFOX,
        _ => USER_AGENT,
    }
}

pub fn has_client_hints(user_agent: &[u8]) -> bool {
    find(user_agent, b"Chrome/").is_some() && find(user_agent, b"Ladybird/").is_none()
}
