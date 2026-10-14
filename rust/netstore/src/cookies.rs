//! Southstar — the cookie jar's rules over bytes: Netscape-format jar lines, Set-Cookie parsing with the __Secure-/__Host- prefixes, domain and path matching, and the Cookie header.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use southstar_url::bytes::{Parts, is_http_or_https, parts};

use crate::http_date;
use crate::text::{is_space, strtoll};

const HTTP_ONLY: &[u8] = b"#HttpOnly_";

fn chomp(s: &[u8]) -> &[u8] {
    &s[..s.iter().rposition(|&c| !is_space(c)).map_or(0, |i| i + 1)]
}

fn fields(line: &[u8]) -> Vec<&[u8]> {
    line.splitn(7, |&c| c == b'\t').collect()
}

fn expiry_of(field: &[u8]) -> i64 {
    strtoll(field).0
}

fn domain_matches(host: &[u8], cookie_domain: &[u8]) -> bool {
    match cookie_domain.strip_prefix(b".") {
        Some(bare) => {
            (host.len() >= cookie_domain.len()
                && host[host.len() - cookie_domain.len()..].eq_ignore_ascii_case(cookie_domain))
                || host.eq_ignore_ascii_case(bare)
        }
        None => host.eq_ignore_ascii_case(cookie_domain),
    }
}

fn path_matches(path: &[u8], cookie_path: &[u8]) -> bool {
    if cookie_path.is_empty() {
        return true;
    }
    if !path.starts_with(cookie_path) {
        return false;
    }
    let cl = cookie_path.len();
    path.len() == cl || path[cl] == b'/' || cookie_path[cl - 1] == b'/'
}

pub fn header<'a>(
    url: &Parts,
    jars: impl IntoIterator<Item = &'a [u8]>,
    include_httponly: bool,
    now: i64,
) -> Option<Vec<u8>> {
    let host = &url.hostname;
    let path: &[u8] = if url.pathname.is_empty() {
        b"/"
    } else {
        &url.pathname
    };
    let is_https = url.protocol.eq_ignore_ascii_case(b"https:");
    let mut order: Vec<Vec<u8>> = Vec::new();
    let mut values: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    for contents in jars {
        for raw in contents.split(|&c| c == b'\n') {
            let mut line = chomp(raw);
            if line.is_empty() {
                continue;
            }
            if line[0] == b'#' {
                match line.strip_prefix(HTTP_ONLY) {
                    Some(rest) if include_httponly => line = rest,
                    _ => continue,
                }
            }
            let f = fields(line);
            if f.len() < 7 {
                continue;
            }
            let secure = f[3].eq_ignore_ascii_case(b"TRUE");
            let expiry = expiry_of(f[4]);
            let matched = domain_matches(host, f[0])
                && path_matches(path, f[2])
                && (!secure || is_https)
                && (expiry == 0 || expiry >= now);
            if matched && !f[5].is_empty() {
                if !values.contains_key(f[5]) {
                    order.push(f[5].to_vec());
                }
                values.insert(f[5].to_vec(), f[6].to_vec());
            }
        }
    }
    let mut out = Vec::new();
    for name in &order {
        if !out.is_empty() {
            out.extend_from_slice(b"; ");
        }
        out.extend_from_slice(name);
        out.push(b'=');
        out.extend_from_slice(values.get(name).map_or(&b""[..], Vec::as_slice));
    }
    (!out.is_empty()).then_some(out)
}

pub fn has_live_httponly(contents: &[u8], cookie: &Cookie) -> bool {
    contents.split(|&c| c == b'\n').any(|line| {
        let Some(rest) = line.strip_prefix(HTTP_ONLY) else {
            return false;
        };
        let f = fields(rest);
        if f.len() < 7 {
            return false;
        }
        let expiry = expiry_of(f[4]);
        f[0].eq_ignore_ascii_case(&cookie.file_domain)
            && f[2] == cookie.path.as_slice()
            && f[5] == cookie.name.as_slice()
            && (expiry == 0 || expiry >= cookie.now)
    })
}

fn trim_blanks(mut s: &[u8]) -> &[u8] {
    while let [b' ' | b'\t', rest @ ..] = s {
        s = rest;
    }
    while let [rest @ .., b' ' | b'\t'] = s {
        s = rest;
    }
    s
}

fn trim_leading_blanks(mut s: &[u8]) -> &[u8] {
    while let [b' ' | b'\t', rest @ ..] = s {
        s = rest;
    }
    s
}

fn trim_trailing_blanks(mut s: &[u8]) -> &[u8] {
    while let [rest @ .., b' ' | b'\t'] = s {
        s = rest;
    }
    s
}

#[derive(Default)]
pub struct Attributes {
    pub domain: Option<Vec<u8>>,
    pub path: Option<Vec<u8>>,
    pub secure: bool,
    pub httponly: bool,
    pub has_expiry: bool,
    pub expired: bool,
    pub expiry: i64,
}

fn parse_attributes(attrs: &[u8], now: i64) -> Attributes {
    let mut a = Attributes::default();
    let mut p = 0;
    while p < attrs.len() {
        while p < attrs.len() && matches!(attrs[p], b';' | b' ' | b'\t') {
            p += 1;
        }
        if p == attrs.len() {
            break;
        }
        let end = attrs[p..].iter().position(|&c| c == b';').map(|i| p + i);
        let attr = trim_trailing_blanks(&attrs[p..end.unwrap_or(attrs.len())]);
        let eq = attr.iter().position(|&c| c == b'=');
        let key = &attr[..eq.unwrap_or(attr.len())];
        let value = eq.map(|e| trim_leading_blanks(&attr[e + 1..]));
        let named = |k: &[u8]| key.eq_ignore_ascii_case(k);
        if named(b"secure") {
            a.secure = true;
        } else if named(b"httponly") {
            a.httponly = true;
        } else if let (true, Some(v)) = (named(b"max-age"), value) {
            let (max_age, consumed) = strtoll(v);
            if consumed > 0 {
                a.has_expiry = true;
                a.expired = max_age <= 0;
                if max_age > 0 {
                    a.expiry = now.wrapping_add(max_age);
                }
            }
        } else if let (true, Some(v), false) = (named(b"expires"), value, a.has_expiry) {
            if let Some(t) = http_date::parse(v) {
                a.has_expiry = true;
                a.expiry = t;
                if t <= now {
                    a.expired = true;
                }
            }
        } else if let (true, Some(v)) = (named(b"domain"), value) {
            a.domain = Some(v.to_vec());
        } else if let (true, Some(v)) = (named(b"path"), value) {
            a.path = Some(v.to_vec());
        }
        match end {
            Some(end) => p = end + 1,
            None => break,
        }
    }
    a
}

fn has_prefix_ignore_case(s: &[u8], prefix: &[u8]) -> bool {
    s.len() >= prefix.len() && s[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn default_path(request_path: &[u8]) -> Vec<u8> {
    match request_path.iter().rposition(|&c| c == b'/') {
        None | Some(0) => b"/".to_vec(),
        Some(last) => request_path[..last].to_vec(),
    }
}

pub struct Cookie {
    pub name: Vec<u8>,
    pub value: Vec<u8>,
    pub attributes: Attributes,
    pub file_domain: Vec<u8>,
    pub include_subdomains: bool,
    pub path: Vec<u8>,
    pub now: i64,
}

pub fn parse(
    target: &[u8],
    cookie: &[u8],
    now: i64,
    is_public_suffix: impl Fn(&[u8]) -> bool,
) -> Option<Cookie> {
    if target.is_empty() || !is_http_or_https(target) || cookie.iter().any(|&c| c < 0x20) {
        return None;
    }
    let url = parts(target).filter(|p| !p.hostname.is_empty())?;
    let semi = cookie.iter().position(|&c| c == b';');
    let pair = &cookie[..semi.unwrap_or(cookie.len())];
    let eq = pair.iter().position(|&c| c == b'=').filter(|&e| e > 0)?;
    let name = trim_blanks(&pair[..eq]);
    let value = trim_blanks(&pair[eq + 1..]);
    if name.is_empty() {
        return None;
    }
    let a = semi.map_or_else(Attributes::default, |s| parse_attributes(&cookie[s..], now));
    let is_https = url.protocol.eq_ignore_ascii_case(b"https:");
    if a.secure && !is_https {
        return None;
    }
    if has_prefix_ignore_case(name, b"__Secure-") && !(a.secure && is_https) {
        return None;
    }
    let domain_attr = a.domain.as_deref().filter(|d| !d.is_empty());
    let path_attr = a.path.as_deref().filter(|p| !p.is_empty());
    if has_prefix_ignore_case(name, b"__Host-")
        && (!a.secure || !is_https || domain_attr.is_some() || path_attr.is_some_and(|p| p != b"/"))
    {
        return None;
    }
    let host = &url.hostname;
    let (file_domain, include_subdomains) = match domain_attr {
        Some(d) => {
            let d = d.strip_prefix(b".").unwrap_or(d);
            let ok = host.eq_ignore_ascii_case(d)
                || (host.len() > d.len()
                    && host[host.len() - d.len() - 1] == b'.'
                    && host[host.len() - d.len()..].eq_ignore_ascii_case(d));
            if !ok || d.is_empty() || is_public_suffix(&d.to_ascii_lowercase()) {
                return None;
            }
            ([&b"."[..], d].concat(), true)
        }
        None => (host.clone(), false),
    };
    let path = match a.path.as_deref() {
        Some(p) if p.first() == Some(&b'/') => p.to_vec(),
        _ => default_path(&url.pathname),
    };
    Some(Cookie {
        name: name.to_vec(),
        value: value.to_vec(),
        attributes: a,
        file_domain,
        include_subdomains,
        path,
        now,
    })
}

pub fn store(jar: Option<&[u8]>, cookie: &Cookie, from_http: bool) -> Vec<u8> {
    let Cookie {
        name,
        value,
        attributes: a,
        file_domain,
        include_subdomains,
        path,
        now,
    } = cookie;
    let (name, value, file_domain, path, now) =
        (&name[..], &value[..], &file_domain[..], &path[..], *now);
    let mut out = Vec::new();
    let mut blocked_httponly = false;
    for line in jar.unwrap_or_default().split(|&c| c == b'\n') {
        if line.is_empty() {
            continue;
        }
        if line[0] == b'#' {
            let mut drop = false;
            if let Some(rest) = line.strip_prefix(HTTP_ONLY) {
                let f = fields(rest);
                let same = f.len() >= 7
                    && f[0].eq_ignore_ascii_case(file_domain)
                    && f[2] == path
                    && f[5] == name;
                if same {
                    if from_http {
                        drop = true;
                    } else {
                        blocked_httponly = true;
                    }
                }
            }
            if !drop {
                out.extend_from_slice(line);
                out.push(b'\n');
            }
            continue;
        }
        let f = fields(line);
        if f.len() < 7 {
            continue;
        }
        let expiry = expiry_of(f[4]);
        let same = f[0].eq_ignore_ascii_case(file_domain) && f[2] == path && f[5] == name;
        let dead = expiry != 0 && expiry < now;
        if !same && !dead {
            out.extend_from_slice(line);
            out.push(b'\n');
        }
    }
    if !a.expired && !blocked_httponly {
        let prefix: &[u8] = if from_http && a.httponly {
            HTTP_ONLY
        } else {
            b""
        };
        out.extend_from_slice(prefix);
        out.extend_from_slice(file_domain);
        out.push(b'\t');
        out.extend_from_slice(if *include_subdomains {
            b"TRUE"
        } else {
            b"FALSE"
        });
        out.push(b'\t');
        out.extend_from_slice(path);
        out.push(b'\t');
        out.extend_from_slice(if a.secure { b"TRUE" } else { b"FALSE" });
        out.extend_from_slice(format!("\t{}\t", a.expiry).as_bytes());
        out.extend_from_slice(name);
        out.push(b'\t');
        out.extend_from_slice(value);
        out.push(b'\n');
    }
    out
}
