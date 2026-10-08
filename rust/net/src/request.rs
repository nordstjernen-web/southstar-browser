//! Southstar — shaping a request: its destination and Sec-Fetch metadata, the cache-partition and coalescing keys, the header lines handed to the transport, and how a redirect rewrites the next hop.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::url;

pub const MAX_REDIRECTS: i64 = 10;
pub const DEFAULT_TIMEOUT_S: i64 = 30;
pub const MAX_TIMEOUT_S: i64 = 60;
pub const NAVIGATION_CONNECT_TIMEOUT_S: i64 = 15;
pub const SUBRESOURCE_CONNECT_TIMEOUT_S: i64 = 6;

pub const DOCUMENT_ACCEPT: &[u8] =
    b"Accept: text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8";
pub const ANY_ACCEPT: &[u8] = b"Accept: */*";
pub const CLIENT_HINTS_BRANDS: &[u8] =
    b"Sec-CH-UA: \"Southstar\";v=\"1\", \"Not=A?Brand\";v=\"24\"";

const KNOWN_DESTINATIONS: [&str; 11] = [
    "script",
    "style",
    "image",
    "font",
    "iframe",
    "frame",
    "object",
    "embed",
    "worker",
    "sharedworker",
    "serviceworker",
];

const SENSITIVE_HEADERS: [&[u8]; 3] = [b"authorization", b"cookie", b"proxy-authorization"];

const BODY_HEADERS: [&[u8]; 4] = [
    b"Content-Encoding:",
    b"Content-Language:",
    b"Content-Location:",
    b"Content-Type:",
];

pub fn has_prefix_ci(line: &[u8], prefix: &[u8]) -> bool {
    line.len() >= prefix.len() && line[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn find_header<'a>(headers: &'a [Vec<u8>], prefix: &[u8]) -> Option<&'a [u8]> {
    headers
        .iter()
        .map(Vec::as_slice)
        .find(|h| has_prefix_ci(h, prefix))
}

fn skip_blanks(mut value: &[u8]) -> &[u8] {
    while let [b' ' | b'\t', rest @ ..] = value {
        value = rest;
    }
    value
}

fn is_method(method: Option<&[u8]>, name: &[u8]) -> bool {
    method.is_some_and(|m| m.eq_ignore_ascii_case(name))
}

fn is_named_method_other_than_get_or_head(method: Option<&[u8]>) -> bool {
    method.is_some_and(|m| {
        !m.is_empty() && !m.eq_ignore_ascii_case(b"GET") && !m.eq_ignore_ascii_case(b"HEAD")
    })
}

pub fn is_simple_get(method: Option<&[u8]>) -> bool {
    method.is_none_or(|m| m.is_empty() || m.eq_ignore_ascii_case(b"GET"))
}

pub fn is_navigation(headers: &[Vec<u8>]) -> bool {
    find_header(headers, b"X-ND-Navigate:").is_some()
}

pub fn has_user_activation(headers: &[Vec<u8>]) -> bool {
    find_header(headers, b"X-ND-User-Activated:").is_some()
}

pub fn fetch_destination(headers: &[Vec<u8>]) -> &'static str {
    const PREFIX: &[u8] = b"X-ND-Fetch-Dest:";
    headers
        .iter()
        .filter(|h| has_prefix_ci(h, PREFIX))
        .find_map(|h| {
            let value = skip_blanks(&h[PREFIX.len()..]);
            KNOWN_DESTINATIONS
                .into_iter()
                .find(|known| value.eq_ignore_ascii_case(known.as_bytes()))
        })
        .unwrap_or("empty")
}

pub fn is_nested_navigation(destination: &str) -> bool {
    matches!(destination, "iframe" | "frame" | "object" | "embed")
}

pub fn is_worker(destination: &str) -> bool {
    matches!(destination, "worker" | "sharedworker" | "serviceworker")
}

pub fn partition_key(partition: &[u8], user_agent: &[u8], accept_language: &[u8]) -> Vec<u8> {
    [
        b"top=",
        partition,
        b"\x1fua=",
        user_agent,
        b"\x1fal=",
        accept_language,
    ]
    .concat()
}

pub fn partition_of(top_site: Option<&[u8]>, top_origin: Option<&[u8]>) -> Vec<u8> {
    top_site
        .filter(|s| !s.is_empty())
        .or(top_origin)
        .unwrap_or_default()
        .to_vec()
}

pub fn request_origin<'a>(
    url: &[u8],
    top_url: Option<&[u8]>,
    top_origin: Option<&'a [u8]>,
    method: Option<&[u8]>,
) -> &'a [u8] {
    let Some(origin) = top_origin.filter(|o| !o.is_empty()) else {
        return b"";
    };
    if origin.iter().any(|&c| c == b'\r' || c == b'\n') || origin.len() >= 4096 {
        return b"";
    }
    if top_url.is_some_and(|top| !url::same_origin(top, url)) {
        return origin;
    }
    if is_named_method_other_than_get_or_head(method) {
        return origin;
    }
    b""
}

pub fn is_coalescable(url: &[u8], method: Option<&[u8]>) -> bool {
    url::is_http_or_https(url) && is_simple_get(method)
}

pub fn coalescing_key(url: &[u8], partition: &[u8], extra_headers: &[&[u8]]) -> Vec<u8> {
    let mut sorted = extra_headers.to_vec();
    sorted.sort_unstable();
    let mut key = [b"GET\x1f", url, b"\x1f", partition].concat();
    for line in sorted {
        key.push(0x1f);
        key.extend_from_slice(line);
    }
    key
}

pub fn caller_accept(headers: &[Vec<u8>]) -> Option<&[u8]> {
    find_header(headers, b"Accept:").map(|h| skip_blanks(&h[b"Accept:".len()..]))
}

pub fn requested_accept(headers: &[Vec<u8>]) -> &[u8] {
    caller_accept(headers).unwrap_or(b"*/*")
}

pub fn timeout_header(headers: &[Vec<u8>]) -> Option<&[u8]> {
    const PREFIX: &[u8] = b"X-ND-Timeout-Seconds:";
    headers.iter().find_map(|h| h.strip_prefix(PREFIX))
}

pub fn clamp_redirects(configured: Option<i64>) -> i64 {
    configured.unwrap_or(MAX_REDIRECTS).clamp(0, MAX_REDIRECTS)
}

pub fn fetch_site(top_url: Option<&[u8]>, url: &[u8]) -> &'static str {
    match top_url.filter(|t| !t.is_empty()) {
        None => "none",
        Some(top) if url::same_origin(top, url) => "same-origin",
        Some(top) if url::is_same_site(top, url) => "same-site",
        Some(_) => "cross-site",
    }
}

pub fn fetch_mode(navigates: bool, destination: &str, method: Option<&[u8]>) -> &'static str {
    if navigates {
        "navigate"
    } else if is_worker(destination) {
        "same-origin"
    } else if is_named_method_other_than_get_or_head(method) {
        "cors"
    } else {
        "no-cors"
    }
}

pub fn content_type_line(
    method: Option<&[u8]>,
    has_body: bool,
    content_type: Option<&[u8]>,
    is_navigation: bool,
    headers: &[Vec<u8>],
) -> Option<Vec<u8>> {
    let is_post = is_method(method, b"POST");
    let is_get = is_simple_get(method);
    if find_header(headers, b"Content-Type:").is_some() || !(is_post || (has_body && !is_get)) {
        return None;
    }
    Some(match content_type.filter(|ct| !ct.is_empty()) {
        Some(ct) => [b"Content-Type: ", ct].concat(),
        None if is_navigation && has_body => {
            b"Content-Type: application/x-www-form-urlencoded".to_vec()
        }
        None => b"Content-Type:".to_vec(),
    })
}

pub fn forwarded_line(line: &[u8]) -> Option<Vec<u8>> {
    if line.is_empty()
        || has_prefix_ci(line, b"X-ND-")
        || line.iter().any(|&c| c == b'\r' || c == b'\n')
    {
        return None;
    }
    match line.iter().position(|&c| c == b':') {
        Some(colon) if colon > 0 && skip_blanks(&line[colon + 1..]).is_empty() => {
            Some([&line[..colon], b";"].concat())
        }
        _ => Some(line.to_vec()),
    }
}

pub fn is_redirect(status: i64) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

pub fn strip_sensitive(headers: &mut Vec<Vec<u8>>) {
    headers.retain(|h| {
        let Some(colon) = h.iter().position(|&c| c == b':') else {
            return true;
        };
        let name = &h[..colon];
        !SENSITIVE_HEADERS
            .iter()
            .any(|s| name.eq_ignore_ascii_case(s))
    });
}

pub fn redirect_drops_body(status: i64, method: &[u8]) -> bool {
    let was_post = method.eq_ignore_ascii_case(b"POST");
    let get_or_head = method.eq_ignore_ascii_case(b"GET") || method.eq_ignore_ascii_case(b"HEAD");
    ((status == 301 || status == 302) && was_post) || (status == 303 && !get_or_head)
}

pub fn strip_body_headers(headers: &mut Vec<Vec<u8>>) {
    headers.retain(|h| !BODY_HEADERS.iter().any(|b| has_prefix_ci(h, b)));
}

pub fn redirect_allowed(next: &[u8], started_https: bool, is_navigation: bool) -> bool {
    url::is_http_or_https(next)
        && (!started_https || next.starts_with(b"https://") || is_navigation)
}

pub fn too_large_message(total: u64) -> Vec<u8> {
    format!(
        "response would exhaust available memory (stopped at {} MiB)",
        total >> 20
    )
    .into_bytes()
}
