//! Southstar — fetching a URL with redirects followed, for the callers that hand the client a plain absolute URL: splitting it into a request target and resolving each Location against it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::time::Duration;

use crate::client::{Outcome, Request, perform};
use crate::ffi::tls::Settings;
use crate::h1;
use crate::proxy::Proxy;
use crate::transfer::Handler;

pub struct Target {
    pub https: bool,
    pub host: String,
    pub port: u16,
    pub authority: Vec<u8>,
    pub path: Vec<u8>,
}

pub fn parse_target(url: &[u8]) -> Option<Target> {
    let (https, rest) = if url.len() > 8 && url[..8].eq_ignore_ascii_case(b"https://") {
        (true, &url[8..])
    } else if url.len() > 7 && url[..7].eq_ignore_ascii_case(b"http://") {
        (false, &url[7..])
    } else {
        return None;
    };
    let rest = &rest[..rest.iter().position(|&c| c == b'#').unwrap_or(rest.len())];
    let split = rest
        .iter()
        .position(|&c| c == b'/' || c == b'?')
        .unwrap_or(rest.len());
    let (authority, path) = rest.split_at(split);
    let authority = match authority.iter().rposition(|&c| c == b'@') {
        Some(at) => &authority[at + 1..],
        None => authority,
    };
    let default_port = if https { 443 } else { 80 };
    let (host, port, explicit) = if authority.first() == Some(&b'[') {
        let close = authority.iter().position(|&c| c == b']')?;
        let port = match authority[close + 1..].strip_prefix(b":") {
            Some(p) if !p.is_empty() => Some(core::str::from_utf8(p).ok()?.parse().ok()?),
            _ => None,
        };
        (&authority[..=close], port.unwrap_or(default_port), port.is_some())
    } else {
        match authority.iter().position(|&c| c == b':') {
            Some(colon) if colon + 1 < authority.len() => (
                &authority[..colon],
                core::str::from_utf8(&authority[colon + 1..]).ok()?.parse().ok()?,
                true,
            ),
            Some(colon) => (&authority[..colon], default_port, false),
            None => (authority, default_port, false),
        }
    };
    if host.is_empty() {
        return None;
    }
    let host = String::from_utf8_lossy(host).to_ascii_lowercase();
    let authority = if explicit && port != default_port {
        format!("{host}:{port}").into_bytes()
    } else {
        host.clone().into_bytes()
    };
    let path = match path.first() {
        Some(b'/') => path.to_vec(),
        Some(_) => [b"/", path].concat(),
        None => b"/".to_vec(),
    };
    Some(Target {
        https,
        host,
        port,
        authority,
        path,
    })
}

fn has_scheme(s: &[u8]) -> bool {
    match s.iter().position(|&c| c == b':') {
        Some(i) if i > 0 => {
            s[0].is_ascii_alphabetic()
                && s[..i]
                    .iter()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
        }
        _ => false,
    }
}

fn remove_dots(path: &[u8]) -> Vec<u8> {
    let mut out: Vec<&[u8]> = Vec::new();
    let segments: Vec<&[u8]> = path.split(|&c| c == b'/').collect();
    let last = segments.len().saturating_sub(1);
    for (i, seg) in segments.iter().enumerate() {
        match *seg {
            b"." => {
                if i == last {
                    out.push(b"");
                }
            }
            b".." => {
                if out.len() > 1 {
                    out.pop();
                }
                if i == last {
                    out.push(b"");
                }
            }
            s => out.push(s),
        }
    }
    out.join(&b'/')
}

pub fn resolve(base: &[u8], location: &[u8]) -> Option<Vec<u8>> {
    let location = location.trim_ascii();
    if has_scheme(location) {
        return Some(location.to_vec());
    }
    let scheme_end = base.windows(3).position(|w| w == b"://")? + 3;
    if let Some(rest) = location.strip_prefix(b"//") {
        return Some([&base[..scheme_end], rest].concat());
    }
    let after = &base[scheme_end..];
    let authority_end = scheme_end
        + after
            .iter()
            .position(|&c| matches!(c, b'/' | b'?' | b'#'))
            .unwrap_or(after.len());
    let origin = &base[..authority_end];
    let base_path_full = &base[authority_end..];
    let base_path_full =
        &base_path_full[..base_path_full.iter().position(|&c| c == b'#').unwrap_or(base_path_full.len())];
    let query_at = base_path_full.iter().position(|&c| c == b'?');
    let base_path = &base_path_full[..query_at.unwrap_or(base_path_full.len())];
    if location.is_empty() {
        return Some([origin, base_path_full].concat());
    }
    if location[0] == b'?' {
        return Some([origin, base_path, location].concat());
    }
    if location[0] == b'#' {
        return Some([origin, base_path_full, location].concat());
    }
    let (loc_path, loc_tail) = match location.iter().position(|&c| c == b'?' || c == b'#') {
        Some(i) => location.split_at(i),
        None => (location, &b""[..]),
    };
    let merged = if loc_path.first() == Some(&b'/') {
        loc_path.to_vec()
    } else {
        let dir = match base_path.iter().rposition(|&c| c == b'/') {
            Some(i) => &base_path[..=i],
            None => b"/",
        };
        [dir, loc_path].concat()
    };
    Some([origin, &remove_dots(&merged), loc_tail].concat())
}

pub struct Route {
    pub tls: Settings,
    pub proxy: Option<Proxy>,
    pub allow_insecure: bool,
}

pub struct Fetch<'a> {
    pub url: Vec<u8>,
    pub method: &'a [u8],
    pub body: &'a [u8],
    pub user_agent: Option<&'a [u8]>,
    pub headers: Vec<Vec<u8>>,
    pub timeout: Duration,
    pub connect_timeout: Duration,
    pub max_redirects: u32,
    pub https_only_redirects: bool,
    pub route: &'a dyn Fn(&[u8], &str) -> Route,
}

struct Follow<'h> {
    inner: &'h mut dyn Handler,
    status: i64,
    location: Option<Vec<u8>>,
    swallow: bool,
}

fn is_redirect(status: i64) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

impl Handler for Follow<'_> {
    fn should_abort(&self) -> bool {
        self.inner.should_abort()
    }

    fn status_line(&mut self, line: &[u8]) {
        self.status = h1::status_code(line).unwrap_or(0);
        self.location = None;
        self.swallow = false;
        self.inner.status_line(line);
    }

    fn header(&mut self, line: &[u8], name: &[u8], value: &[u8]) {
        if name.eq_ignore_ascii_case(b"location") {
            self.location = Some(value.trim_ascii().to_vec());
        }
        self.inner.header(line, name, value);
    }

    fn headers_done(&mut self) {
        self.swallow = is_redirect(self.status) && self.location.is_some();
        if !self.swallow {
            self.inner.headers_done();
        }
    }

    fn body(&mut self, data: &[u8]) -> bool {
        self.swallow || self.inner.body(data)
    }
}

pub fn fetch(f: &Fetch, handler: &mut dyn Handler) -> (Outcome, Vec<u8>) {
    let mut url = f.url.clone();
    let mut method = f.method;
    let mut body = f.body;
    let mut follow = Follow {
        inner: handler,
        status: 0,
        location: None,
        swallow: false,
    };
    let mut redirects = 0;
    loop {
        let Some(target) = parse_target(&url) else {
            let mut out = crate::client::failed(String::from("unsupported or invalid URL"));
            out.connect_failed = true;
            return (out, url);
        };
        let route = (f.route)(&url, &target.host);
        let lines: Vec<&[u8]> = f.headers.iter().map(Vec::as_slice).collect();
        let request = Request {
            url: &url,
            https: target.https,
            host: &target.host,
            port: target.port,
            authority: &target.authority,
            path: &target.path,
            method,
            user_agent: f.user_agent,
            referer: None,
            cookie: None,
            extra_headers: lines,
            body,
            timeout: f.timeout,
            connect_timeout: f.connect_timeout,
            allow_insecure: route.allow_insecure,
            tls: route.tls,
            proxy: route.proxy,
        };
        follow.status = 0;
        follow.location = None;
        follow.swallow = false;
        let outcome = perform(&request, &mut follow);
        let next = follow
            .location
            .as_deref()
            .filter(|_| outcome.ok && is_redirect(outcome.status) && redirects < f.max_redirects)
            .and_then(|loc| resolve(&url, loc));
        let Some(next) = next else {
            return (outcome, url);
        };
        let allowed = parse_target(&next).is_some_and(|t| t.https || !f.https_only_redirects);
        if !allowed {
            return (outcome, url);
        }
        if outcome.status == 303 && !method.eq_ignore_ascii_case(b"HEAD")
            || (matches!(outcome.status, 301 | 302) && method.eq_ignore_ascii_case(b"POST"))
        {
            method = b"GET";
            body = b"";
        }
        url = next;
        redirects += 1;
    }
}
