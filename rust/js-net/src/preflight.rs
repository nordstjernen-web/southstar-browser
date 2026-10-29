//! Southstar — CORS preflight: deciding when a cross-origin request needs an OPTIONS check first, and whether the server's answer allows it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use crate::ffi::{self, Credentials, Request, Response, cstring};
use crate::headers;

const SIMPLE_METHODS: [&[u8]; 3] = [b"GET", b"HEAD", b"POST"];
const SIMPLE_CONTENT_TYPES: [&[u8]; 3] = [
    b"application/x-www-form-urlencoded",
    b"multipart/form-data",
    b"text/plain",
];
const SAFELISTED_HEADERS: [&[u8]; 3] = [b"accept", b"accept-language", b"content-language"];
const DEFAULT_MAX_AGE_S: u64 = 5;
const MAX_AGE_CAP_S: u64 = 7200;
const MAX_GRANTS: usize = 128;

struct Grant {
    origin: Vec<u8>,
    url: Vec<u8>,
    methods: Vec<u8>,
    headers: Vec<u8>,
    expires: Instant,
}

static GRANTS: Mutex<Vec<Grant>> = Mutex::new(Vec::new());

pub(crate) struct Preflight {
    method: Vec<u8>,
    header_names: Vec<Vec<u8>>,
}

fn header_name(line: &[u8]) -> &[u8] {
    let colon = line.iter().position(|&c| c == b':').unwrap_or(line.len());
    line[..colon].trim_ascii()
}

fn header_value_of(line: &[u8]) -> &[u8] {
    line.iter()
        .position(|&c| c == b':')
        .map_or(&[][..], |colon| &line[colon + 1..])
}

fn simple_content_type(content_type: &[u8]) -> bool {
    let essence = content_type
        .split(|&c| c == b';')
        .next()
        .unwrap_or_default()
        .trim_ascii();
    SIMPLE_CONTENT_TYPES
        .iter()
        .any(|simple| essence.eq_ignore_ascii_case(simple))
}

pub(crate) fn needed(
    method: &[u8],
    content_type: Option<&[u8]>,
    author_headers: &[Vec<u8>],
) -> Option<Preflight> {
    let mut header_names: Vec<Vec<u8>> = author_headers
        .iter()
        .filter(|line| {
            !header_name(line).eq_ignore_ascii_case(b"content-type")
                || !simple_content_type(header_value_of(line))
        })
        .map(|line| header_name(line).to_ascii_lowercase())
        .filter(|name| !SAFELISTED_HEADERS.contains(&name.as_slice()))
        .collect();
    if content_type.is_some_and(|ct| !simple_content_type(ct)) {
        header_names.push(b"content-type".to_vec());
    }
    header_names.sort();
    header_names.dedup();
    let simple_method = SIMPLE_METHODS.contains(&method);
    (!simple_method || !header_names.is_empty()).then(|| Preflight {
        method: method.to_vec(),
        header_names,
    })
}

fn listed(list: &[u8], item: &[u8], case_sensitive: bool) -> bool {
    list.split(|&c| c == b',')
        .map(<[u8]>::trim_ascii)
        .any(|entry| {
            entry == b"*"
                || if case_sensitive {
                    entry == item
                } else {
                    entry.eq_ignore_ascii_case(item)
                }
        })
}

fn max_age(raw: &[u8]) -> Duration {
    let value = header_value(raw, b"access-control-max-age");
    let value = value.trim_ascii();
    let seconds = if !value.is_empty() && value.iter().all(u8::is_ascii_digit) {
        std::str::from_utf8(value)
            .ok()
            .and_then(|digits| digits.parse::<u64>().ok())
            .unwrap_or(MAX_AGE_CAP_S)
    } else {
        DEFAULT_MAX_AGE_S
    };
    Duration::from_secs(seconds.min(MAX_AGE_CAP_S))
}

fn header_value(raw: &[u8], name: &[u8]) -> Vec<u8> {
    let mut joined = Vec::new();
    for (key, value) in headers::raw_header_lines(raw) {
        if key.trim_ascii().eq_ignore_ascii_case(name) {
            if !joined.is_empty() {
                joined.push(b',');
            }
            joined.extend_from_slice(value);
        }
    }
    joined
}

impl Preflight {
    pub(crate) fn request(
        &self,
        url: &CString,
        top: Option<&CString>,
        document: Option<&CString>,
    ) -> Request {
        let mut lines = vec![[&b"Access-Control-Request-Method: "[..], &self.method].concat()];
        if !self.header_names.is_empty() {
            lines.push(
                [
                    &b"Access-Control-Request-Headers: "[..],
                    &self.header_names.join(&b","[..]),
                ]
                .concat(),
            );
        }
        Request {
            url: url.clone(),
            top: top.cloned(),
            document: document.cloned(),
            method: cstring(b"OPTIONS"),
            body: Vec::new(),
            has_body: false,
            content_type: None,
            headers: lines.iter().map(|line| cstring(line)).collect(),
            credentials: Credentials::Omit,
        }
    }

    fn allowed_by(&self, methods: &[u8], allowed_headers: &[u8]) -> bool {
        let method_ok =
            SIMPLE_METHODS.contains(&self.method.as_slice()) || listed(methods, &self.method, true);
        method_ok
            && self
                .header_names
                .iter()
                .all(|name| listed(allowed_headers, name, false))
    }

    pub(crate) fn cached(&self, url: &[u8], origin_url: Option<&[u8]>) -> bool {
        let Some(origin) = ffi::url_origin_from(origin_url) else {
            return false;
        };
        let now = Instant::now();
        let mut grants = GRANTS.lock().unwrap_or_else(PoisonError::into_inner);
        grants.retain(|grant| grant.expires > now);
        grants.iter().any(|grant| {
            grant.origin == origin
                && grant.url == url
                && self.allowed_by(&grant.methods, &grant.headers)
        })
    }

    fn remember(&self, url: &[u8], origin_url: Option<&[u8]>, raw: &[u8]) {
        let lifetime = max_age(raw);
        let Some(origin) = ffi::url_origin_from(origin_url).filter(|_| !lifetime.is_zero()) else {
            return;
        };
        let mut grants = GRANTS.lock().unwrap_or_else(PoisonError::into_inner);
        if grants.len() >= MAX_GRANTS {
            grants.remove(0);
        }
        grants.push(Grant {
            origin,
            url: url.to_vec(),
            methods: header_value(raw, b"access-control-allow-methods"),
            headers: header_value(raw, b"access-control-allow-headers"),
            expires: Instant::now() + lifetime,
        });
    }

    pub(crate) fn passed(&self, resp: &Response, origin_url: Option<&[u8]>, url: &[u8]) -> bool {
        if resp.is_null() || resp.error().is_some() || !(200..300).contains(&resp.status()) {
            return false;
        }
        let final_url = resp.final_url();
        if !headers::cors_allows(
            origin_url,
            final_url.as_deref(),
            resp.cors_allow_origin().as_deref(),
        ) {
            return false;
        }
        let raw = resp.raw_headers().unwrap_or_default();
        let methods = header_value(&raw, b"access-control-allow-methods");
        let allowed_headers = header_value(&raw, b"access-control-allow-headers");
        let allowed = self.allowed_by(&methods, &allowed_headers);
        if allowed {
            self.remember(url, origin_url, &raw);
        }
        allowed
    }
}
