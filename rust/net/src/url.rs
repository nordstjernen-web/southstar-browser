//! Southstar — URL helpers over lexbor's WHATWG parser: resolving, the URL setters, origins, sites, hosts, components, referrers, tracking-parameter stripping, Refresh parsing and the user agent strings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi::host;
use crate::ffi::lexbor::{self, Parser, Setter, Url};

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

pub fn is_http_or_https(url: &[u8]) -> bool {
    url.starts_with(b"http://") || url.starts_with(b"https://")
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

pub fn resolve(base: Option<&[u8]>, href: &[u8]) -> Option<Vec<u8>> {
    let base = base.filter(|b| !b.is_empty());
    if href.is_empty() && base.is_none() {
        return None;
    }
    let parser = Parser::open()?;
    let base_url = match base {
        Some(base) => {
            let url = parser.parse(None, base)?;
            parser.clean();
            Some(url)
        }
        None => None,
    };
    let resolved = parser.parse(base_url.as_ref(), href)?;
    resolved.serialize().filter(|s| !s.is_empty())
}

fn is_special(u: &Url) -> bool {
    (lexbor::SCHEME_HTTP..=lexbor::SCHEME_FILE).contains(&u.scheme_type())
}

fn has_credentials_or_port(u: &Url) -> bool {
    !u.username().is_empty() || !u.password().is_empty() || u.port().is_some()
}

fn set_host(u: &mut Url, parser: &Parser, v: &[u8], hostname_only: bool) {
    let special = is_special(u);
    let mut brackets = false;
    let mut colon = v.len();
    let mut end = 0;
    while end < v.len() {
        let c = v[end];
        if c == b'/' || c == b'?' || c == b'#' || (special && c == b'\\') {
            break;
        }
        match c {
            b'[' => brackets = true,
            b']' => brackets = false,
            b':' if !brackets && colon == v.len() => colon = end,
            _ => {}
        }
        end += 1;
    }
    if colon == v.len() && end == 0 && !special && has_credentials_or_port(u) {
        return;
    }
    if colon == v.len() || hostname_only || u.scheme_type() == lexbor::SCHEME_FILE {
        let setter = if hostname_only {
            Setter::Hostname
        } else {
            Setter::Host
        };
        u.set(parser, setter, v);
        return;
    }
    if colon == 0 || !u.set(parser, Setter::Hostname, &v[..colon]) {
        return;
    }
    let digits = colon
        + 1
        + v[colon + 1..end]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
    if digits == colon + 1 {
        return;
    }
    let mut port = 0u64;
    for &d in &v[colon + 1..digits] {
        if port > 65535 {
            break;
        }
        port = port * 10 + u64::from(d - b'0');
    }
    if port > 65535 {
        return;
    }
    u.set(parser, Setter::Port, &v[colon + 1..digits]);
}

fn apply_setter(u: &mut Url, parser: &Parser, component: &[u8], value: &[u8]) {
    let userinfo_or_port = matches!(component, b"username" | b"password" | b"port");
    if userinfo_or_port && u.host_is_empty() {
        return;
    }
    let setter = match component {
        b"protocol" => Setter::Protocol,
        b"username" => Setter::Username,
        b"password" => Setter::Password,
        b"host" => return set_host(u, parser, value, false),
        b"hostname" => return set_host(u, parser, value, true),
        b"port" => Setter::Port,
        b"pathname" => Setter::Pathname,
        b"search" => Setter::Search,
        b"hash" => Setter::Hash,
        _ => return,
    };
    u.set(parser, setter, value);
}

pub fn set_component(href: &[u8], component: &[u8], value: &[u8]) -> Option<Vec<u8>> {
    let parser = Parser::open()?;
    let mut url = parser.parse(None, href)?;
    apply_setter(&mut url, &parser, component, value);
    url.serialize().filter(|s| !s.is_empty())
}

pub fn to_ascii(url: &[u8]) -> Option<Vec<u8>> {
    if url.is_empty() {
        return None;
    }
    if url.starts_with(b"data:") || url.starts_with(b"about:") || url.starts_with(b"file:") {
        return Some(url.to_vec());
    }
    resolve(None, url)
}

fn parse_with_host(parser: &Parser, url: &[u8]) -> Option<Url> {
    parser.parse(None, url).filter(Url::has_host)
}

fn origin(url: &[u8], default_port: bool) -> Option<Vec<u8>> {
    let parser = Parser::open()?;
    let u = parse_with_host(&parser, url)?;
    let host = u.serialize_host()?;
    let mut out = u.scheme_name().to_vec();
    out.extend_from_slice(b"://");
    out.extend_from_slice(&host);
    let port = match (u.port(), default_port) {
        (Some(port), _) => Some(port),
        (None, false) => None,
        (None, true) => Some(match u.scheme_type() {
            lexbor::SCHEME_HTTP | lexbor::SCHEME_WS => 80,
            lexbor::SCHEME_HTTPS | lexbor::SCHEME_WSS => 443,
            lexbor::SCHEME_FTP => 21,
            _ => 0,
        }),
    };
    if let Some(port) = port {
        out.extend_from_slice(format!(":{port}").as_bytes());
    }
    Some(out)
}

pub fn origin_from(url: &[u8]) -> Option<Vec<u8>> {
    if url.is_empty() || !is_http_or_https(url) {
        return None;
    }
    origin(url, false)
}

pub fn origin_from_any(url: &[u8]) -> Option<Vec<u8>> {
    if url.is_empty() {
        return None;
    }
    origin(url, true)
}

pub fn host_from(url: &[u8]) -> Option<Vec<u8>> {
    let parser = Parser::open()?;
    let u = parse_with_host(&parser, url)?;
    u.serialize_host().filter(|h| !h.is_empty())
}

pub fn same_origin(a: &[u8], b: &[u8]) -> bool {
    match (origin_from(a), origin_from(b)) {
        (Some(oa), Some(ob)) => !oa.is_empty() && !ob.is_empty() && oa.eq_ignore_ascii_case(&ob),
        _ => false,
    }
}

pub struct Parts {
    pub href: Vec<u8>,
    pub protocol: Vec<u8>,
    pub origin: Vec<u8>,
    pub host: Vec<u8>,
    pub hostname: Vec<u8>,
    pub port: Vec<u8>,
    pub pathname: Vec<u8>,
    pub search: Vec<u8>,
    pub hash: Vec<u8>,
    pub username: Vec<u8>,
    pub password: Vec<u8>,
}

fn prefixed(prefix: u8, value: &[u8]) -> Vec<u8> {
    if value.is_empty() {
        Vec::new()
    } else {
        [&[prefix][..], value].concat()
    }
}

fn parts_at_depth(url: &[u8], depth: u32) -> Option<Parts> {
    let parser = Parser::open()?;
    let u = parser.parse(None, url)?;
    let href = u
        .serialize()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| url.to_vec());
    let scheme = u.serialize_scheme().unwrap_or_default();
    let protocol = if scheme.is_empty() {
        Vec::new()
    } else {
        [&scheme[..], b":"].concat()
    };
    let hostname = if u.has_host() {
        u.serialize_host().unwrap_or_default()
    } else {
        Vec::new()
    };
    let port = u
        .port()
        .map_or_else(Vec::new, |p| p.to_string().into_bytes());
    let host = if !hostname.is_empty() && !port.is_empty() {
        [&hostname[..], b":", &port].concat()
    } else {
        hostname.clone()
    };
    let tuple_origin = !hostname.is_empty()
        && matches!(
            &protocol[..],
            b"http:" | b"https:" | b"ws:" | b"wss:" | b"ftp:"
        );
    let mut origin = if tuple_origin {
        [&protocol[..], b"//", &host].concat()
    } else {
        b"null".to_vec()
    };
    let pathname = u.serialize_path().unwrap_or_default();
    if depth == 0
        && protocol == b"blob:"
        && !pathname.is_empty()
        && let Some(inner) = parts_at_depth(&pathname, depth + 1)
        && (inner.protocol == b"http:" || inner.protocol == b"https:")
    {
        origin = inner.origin;
    }
    Some(Parts {
        href,
        protocol,
        origin,
        host,
        hostname,
        port,
        pathname,
        search: prefixed(b'?', u.query()),
        hash: prefixed(b'#', u.fragment()),
        username: u.username().to_vec(),
        password: u.password().to_vec(),
    })
}

pub fn parts(url: &[u8]) -> Option<Parts> {
    parts_at_depth(url, 0)
}

pub fn is_valid_absolute(url: &[u8]) -> bool {
    !url.is_empty()
        && !url
            .iter()
            .any(|&c| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0c))
        && parts(url).is_some_and(|p| !p.protocol.is_empty())
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
