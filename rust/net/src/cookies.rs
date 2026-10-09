//! Southstar — the cookie jar: Netscape-format files per top-level site, read for requests and document.cookie and written from Set-Cookie headers and scripts.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use crate::ffi::sys;
use crate::{storage, url};

const HTTP_ONLY: &[u8] = b"#HttpOnly_";

fn until_nul(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}

fn read_jar(path: &[u8]) -> Option<Vec<u8>> {
    sys::read_file(path)
        .ok()
        .map(|contents| until_nul(&contents).to_vec())
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

fn chomp(s: &[u8]) -> &[u8] {
    &s[..s.iter().rposition(|&c| !is_space(c)).map_or(0, |i| i + 1)]
}

fn fields(line: &[u8]) -> Vec<&[u8]> {
    line.splitn(7, |&c| c == b'\t').collect()
}

fn expiry_of(field: &[u8]) -> i64 {
    sys::ascii_strtoll(field).0
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

pub fn collect(target: &[u8], include_httponly: bool) -> Option<Vec<u8>> {
    if target.is_empty() {
        return None;
    }
    let parts = url::parts(target).filter(|p| !p.hostname.is_empty())?;
    let site = url::site_from(target).filter(|s| !s.is_empty())?;
    let jars = [
        storage::cookie_jar_path(Some(&site), false),
        storage::cookie_jar_path(Some(&site), true),
    ];
    collect_from(&parts, &jars, include_httponly)
}

pub fn collect_in(target: &[u8], jars: &[Option<Vec<u8>>]) -> Option<Vec<u8>> {
    let parts = url::parts(target).filter(|p| !p.hostname.is_empty())?;
    collect_from(&parts, jars, true)
}

fn collect_from(
    parts: &url::Parts,
    jars: &[Option<Vec<u8>>],
    include_httponly: bool,
) -> Option<Vec<u8>> {
    let host = &parts.hostname;
    let path: &[u8] = if parts.pathname.is_empty() {
        b"/"
    } else {
        &parts.pathname
    };
    let is_https = parts.protocol.eq_ignore_ascii_case(b"https:");
    let now = sys::now_seconds();
    let mut order: Vec<Vec<u8>> = Vec::new();
    let mut values: HashMap<Vec<u8>, Vec<u8>> = HashMap::new();
    for jar in jars.iter().flatten() {
        let Some(contents) = read_jar(jar) else {
            continue;
        };
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

fn jar_has_httponly(jar: &[u8], domain: &[u8], path: &[u8], name: &[u8], now: i64) -> bool {
    let Some(contents) = read_jar(jar) else {
        return false;
    };
    contents.split(|&c| c == b'\n').any(|line| {
        let Some(rest) = line.strip_prefix(HTTP_ONLY) else {
            return false;
        };
        let f = fields(rest);
        if f.len() < 7 {
            return false;
        }
        let expiry = expiry_of(f[4]);
        f[0].eq_ignore_ascii_case(domain)
            && f[2] == path
            && f[5] == name
            && (expiry == 0 || expiry >= now)
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
struct Attributes {
    domain: Option<Vec<u8>>,
    path: Option<Vec<u8>>,
    secure: bool,
    httponly: bool,
    has_expiry: bool,
    expired: bool,
    expiry: i64,
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
            let (max_age, consumed) = sys::ascii_strtoll(v);
            if consumed > 0 {
                a.has_expiry = true;
                a.expired = max_age <= 0;
                if max_age > 0 {
                    a.expiry = now.wrapping_add(max_age);
                }
            }
        } else if let (true, Some(v), false) = (named(b"expires"), value, a.has_expiry) {
            if let Some(t) = sys::http_date(v) {
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

struct Parsed {
    name: Vec<u8>,
    value: Vec<u8>,
    a: Attributes,
    file_domain: Vec<u8>,
    tail: &'static [u8],
    path: Vec<u8>,
    site: Vec<u8>,
    now: i64,
}

fn parse(target: &[u8], cookie: &[u8]) -> Option<Parsed> {
    if target.is_empty() || !url::is_http_or_https(target) || cookie.iter().any(|&c| c < 0x20) {
        return None;
    }
    let parts = url::parts(target).filter(|p| !p.hostname.is_empty())?;
    let semi = cookie.iter().position(|&c| c == b';');
    let pair = &cookie[..semi.unwrap_or(cookie.len())];
    let eq = pair.iter().position(|&c| c == b'=').filter(|&e| e > 0)?;
    let name = trim_blanks(&pair[..eq]);
    let value = trim_blanks(&pair[eq + 1..]);
    if name.is_empty() {
        return None;
    }
    let now = sys::now_seconds();
    let a = semi.map_or_else(Attributes::default, |s| parse_attributes(&cookie[s..], now));
    let is_https = parts.protocol.eq_ignore_ascii_case(b"https:");
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
    let host = &parts.hostname;
    let (file_domain, tail): (Vec<u8>, &'static [u8]) = match domain_attr {
        Some(d) => {
            let d = d.strip_prefix(b".").unwrap_or(d);
            let ok = host.eq_ignore_ascii_case(d)
                || (host.len() > d.len()
                    && host[host.len() - d.len() - 1] == b'.'
                    && host[host.len() - d.len()..].eq_ignore_ascii_case(d));
            if !ok || d.is_empty() || sys::is_public_suffix(&d.to_ascii_lowercase()) {
                return None;
            }
            ([&b"."[..], d].concat(), b"TRUE")
        }
        None => (host.clone(), b"FALSE"),
    };
    let path = match a.path.as_deref() {
        Some(p) if p.first() == Some(&b'/') => p.to_vec(),
        _ => default_path(&parts.pathname),
    };
    let site = url::site_from(target).filter(|s| !s.is_empty())?;
    Some(Parsed {
        name: name.to_vec(),
        value: value.to_vec(),
        a,
        file_domain,
        tail,
        path,
        site,
        now,
    })
}

pub fn store(target: &[u8], cookie: &[u8], from_http: bool) {
    let Some(c) = parse(target, cookie) else {
        return;
    };
    let Some(jar) = storage::cookie_jar_path(Some(&c.site), !from_http) else {
        return;
    };
    write_cookie(&jar, from_http, &c);
}

pub fn store_in(target: &[u8], cookie: &[u8], jar: &[u8]) {
    if let Some(c) = parse(target, cookie) {
        write_cookie(jar, true, &c);
    }
}

fn write_cookie(jar: &[u8], from_http: bool, c: &Parsed) {
    let Parsed {
        name,
        value,
        a,
        file_domain,
        tail,
        path,
        site,
        now,
    } = c;
    let (name, value, file_domain, path, now) =
        (&name[..], &value[..], &file_domain[..], &path[..], *now);
    if !from_http {
        let http_jar = storage::cookie_jar_path(Some(site), false);
        if http_jar.is_some_and(|j| jar_has_httponly(&j, file_domain, path, name, now)) {
            return;
        }
    }
    let mut out = Vec::new();
    let mut blocked_httponly = false;
    if let Some(contents) = read_jar(jar) {
        for line in contents.split(|&c| c == b'\n') {
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
        out.extend_from_slice(tail);
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
    if sys::write_file(jar, &out) {
        sys::chmod(jar, 0o600);
    }
}
