//! Southstar — CORS preflight: deciding when a cross-origin request needs an OPTIONS check first, and whether the server's answer allows it.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use std::ffi::CString;

use crate::ffi::{Request, Response, cstring};
use crate::headers;

const SIMPLE_METHODS: [&[u8]; 3] = [b"GET", b"HEAD", b"POST"];
const SIMPLE_CONTENT_TYPES: [&[u8]; 3] = [
    b"application/x-www-form-urlencoded",
    b"multipart/form-data",
    b"text/plain",
];
const SAFELISTED_HEADERS: [&[u8]; 3] = [b"accept", b"accept-language", b"content-language"];

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
    pub(crate) fn request(&self, url: &CString, top: Option<&CString>) -> Request {
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
            method: cstring(b"OPTIONS"),
            body: Vec::new(),
            has_body: false,
            content_type: None,
            headers: lines.iter().map(|line| cstring(line)).collect(),
        }
    }

    pub(crate) fn passed(&self, resp: &Response, origin_url: Option<&[u8]>) -> bool {
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
        let method_ok = SIMPLE_METHODS.contains(&self.method.as_slice())
            || listed(&methods, &self.method, true);
        let allowed_headers = header_value(&raw, b"access-control-allow-headers");
        method_ok
            && self
                .header_names
                .iter()
                .all(|name| listed(&allowed_headers, name, false))
    }
}
