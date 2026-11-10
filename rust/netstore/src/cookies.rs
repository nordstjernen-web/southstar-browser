//! Southstar — the cookie jar's rules over bytes: Netscape-format jar lines, Set-Cookie parsing with the __Secure-/__Host- prefixes, domain and path matching, and the Cookie header.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::collections::HashMap;

use southstar_url::bytes::{Parts, is_http_or_https, parts};

use crate::http_date;
use crate::text::strtoll;

const HTTP_ONLY: &[u8] = b"#HttpOnly_";
const MAX_PAIR_BYTES: usize = 4096;
const MAX_PER_DOMAIN: usize = 180;
const MAX_IN_JAR: usize = 3000;

type Identity = (Vec<u8>, Vec<u8>, Vec<u8>);

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum SameSite {
    None,
    #[default]
    Lax,
    Strict,
}

impl SameSite {
    fn parse(value: &[u8]) -> Option<SameSite> {
        [
            (&b"none"[..], SameSite::None),
            (b"lax", SameSite::Lax),
            (b"strict", SameSite::Strict),
        ]
        .into_iter()
        .find(|(name, _)| value.eq_ignore_ascii_case(name))
        .map(|(_, same_site)| same_site)
    }

    fn name(self) -> &'static [u8] {
        match self {
            SameSite::None => b"None",
            SameSite::Lax => b"Lax",
            SameSite::Strict => b"Strict",
        }
    }

    fn of_line(f: &[&[u8]]) -> SameSite {
        f.get(7)
            .and_then(|field| SameSite::parse(field))
            .unwrap_or_default()
    }

    fn allowed_in(self, context: Context) -> bool {
        match context {
            Context::SameSite => true,
            Context::TopLevelSafeNavigation => self != SameSite::Strict,
            Context::CrossSite => self == SameSite::None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Context {
    SameSite,
    TopLevelSafeNavigation,
    CrossSite,
}

fn fields(line: &[u8]) -> Vec<&[u8]> {
    line.splitn(8, |&c| c == b'\t').collect()
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
    context: Context,
    now: i64,
) -> Option<Vec<u8>> {
    let host = &url.hostname;
    let path: &[u8] = if url.pathname.is_empty() {
        b"/"
    } else {
        &url.pathname
    };
    let is_https = url.protocol.eq_ignore_ascii_case(b"https:");
    let mut sent: Vec<(Vec<u8>, Vec<u8>, usize)> = Vec::new();
    let mut index: HashMap<Identity, usize> = HashMap::new();
    for contents in jars {
        for raw in contents.split(|&c| c == b'\n') {
            let mut line = raw.trim_ascii_end();
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
                && (expiry == 0 || expiry >= now)
                && SameSite::of_line(&f).allowed_in(context);
            if matched && !f[5].is_empty() {
                let key = (f[5].to_vec(), f[0].to_ascii_lowercase(), f[2].to_vec());
                match index.get(&key) {
                    Some(&at) => sent[at].1 = f[6].to_vec(),
                    None => {
                        index.insert(key, sent.len());
                        sent.push((f[5].to_vec(), f[6].to_vec(), f[2].len()));
                    }
                }
            }
        }
    }
    sent.sort_by_key(|(_, _, path_len)| core::cmp::Reverse(*path_len));
    let mut out = Vec::new();
    for (name, value, _) in &sent {
        if !out.is_empty() {
            out.extend_from_slice(b"; ");
        }
        out.extend_from_slice(name);
        out.push(b'=');
        out.extend_from_slice(value);
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
    pub same_site: SameSite,
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
        } else if let (true, Some(v)) = (named(b"samesite"), value) {
            a.same_site = SameSite::parse(v).unwrap_or_default();
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
    pub secure_origin: bool,
}

fn is_ip_literal(host: &[u8]) -> bool {
    host.contains(&b':')
        || (!host.is_empty() && host.iter().all(|c| c.is_ascii_digit() || *c == b'.'))
}

fn bare_domain(domain: &[u8]) -> &[u8] {
    domain.strip_prefix(b".").unwrap_or(domain)
}

fn domains_overlap(a: &[u8], b: &[u8]) -> bool {
    let (a, b) = (bare_domain(a), bare_domain(b));
    let (long, short) = if a.len() >= b.len() { (a, b) } else { (b, a) };
    let tail = &long[long.len() - short.len()..];
    tail.eq_ignore_ascii_case(short)
        && (long.len() == short.len() || long[long.len() - short.len() - 1] == b'.')
}

fn shadows_secure(jar: &[u8], cookie: &Cookie) -> bool {
    jar.split(|&c| c == b'\n').any(|line| {
        let f = fields(line.strip_prefix(HTTP_ONLY).unwrap_or(line));
        if f.len() < 7 || !f[3].eq_ignore_ascii_case(b"TRUE") {
            return false;
        }
        let expiry = expiry_of(f[4]);
        f[5] == cookie.name.as_slice()
            && (expiry == 0 || expiry >= cookie.now)
            && domains_overlap(f[0], &cookie.file_domain)
            && path_matches(&cookie.path, f[2])
    })
}

fn line_domain(line: &[u8]) -> &[u8] {
    let line = line.strip_prefix(HTTP_ONLY).unwrap_or(line);
    line.split(|&c| c == b'\t').next().unwrap_or_default()
}

fn is_cookie_line(line: &[u8]) -> bool {
    !line.is_empty() && (line[0] != b'#' || line.starts_with(HTTP_ONLY))
}

fn evict_oldest(jar: Vec<u8>, domain: &[u8]) -> Vec<u8> {
    let lines: Vec<&[u8]> = jar.split(|&c| c == b'\n').collect();
    let cookies = lines.iter().filter(|line| is_cookie_line(line));
    let in_jar = cookies.clone().count();
    let in_domain = cookies
        .filter(|line| line_domain(line).eq_ignore_ascii_case(domain))
        .count();
    let mut drop_domain = in_domain.saturating_sub(MAX_PER_DOMAIN);
    let mut drop_any = (in_jar - drop_domain).saturating_sub(MAX_IN_JAR);
    if drop_domain == 0 && drop_any == 0 {
        return jar;
    }
    let mut out = Vec::with_capacity(jar.len());
    for line in lines.iter().filter(|line| !line.is_empty()) {
        if is_cookie_line(line) {
            if drop_domain > 0 && line_domain(line).eq_ignore_ascii_case(domain) {
                drop_domain -= 1;
                continue;
            }
            if drop_any > 0 {
                drop_any -= 1;
                continue;
            }
        }
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    out
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
    if name.is_empty() || name.len() + value.len() > MAX_PAIR_BYTES {
        return None;
    }
    let a = semi.map_or_else(Attributes::default, |s| parse_attributes(&cookie[s..], now));
    let is_https = url.protocol.eq_ignore_ascii_case(b"https:");
    if a.secure && !is_https || a.same_site == SameSite::None && !a.secure {
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
            if is_ip_literal(host) {
                if !host.eq_ignore_ascii_case(d) {
                    return None;
                }
                (host.clone(), false)
            } else {
                ([&b"."[..], d].concat(), true)
            }
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
        secure_origin: is_https,
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
        secure_origin,
    } = cookie;
    if a.httponly && !from_http {
        return jar.unwrap_or_default().to_vec();
    }
    if !secure_origin && shadows_secure(jar.unwrap_or_default(), cookie) {
        return jar.unwrap_or_default().to_vec();
    }
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
        out.push(b'\t');
        out.extend_from_slice(a.same_site.name());
        out.push(b'\n');
        return evict_oldest(out, file_domain);
    }
    out
}

pub fn settable_in(cookie: &Cookie, context: Context) -> bool {
    context != Context::CrossSite || cookie.attributes.same_site == SameSite::None
}
