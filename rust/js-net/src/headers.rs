//! Southstar — HTTP header, method and status rules shared by fetch, XMLHttpRequest and WebSocket.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use crate::ffi;

const FORBIDDEN: [&[u8]; 20] = [
    b"accept-charset",
    b"accept-encoding",
    b"access-control-request-headers",
    b"access-control-request-method",
    b"connection",
    b"content-length",
    b"cookie",
    b"cookie2",
    b"date",
    b"dnt",
    b"expect",
    b"host",
    b"keep-alive",
    b"origin",
    b"referer",
    b"te",
    b"trailer",
    b"transfer-encoding",
    b"upgrade",
    b"via",
];

const STANDARD_METHODS: [&[u8]; 6] = [b"DELETE", b"GET", b"HEAD", b"OPTIONS", b"POST", b"PUT"];

pub(crate) fn is_token(name: &[u8]) -> bool {
    !name.is_empty()
        && name
            .iter()
            .all(|&c| c > 0x20 && c < 0x7f && !b"(),/:;<=>?@[\\]{}\"".contains(&c))
}

pub(crate) fn value_is_safe(value: &[u8]) -> bool {
    !value.iter().any(|&c| c == b'\r' || c == b'\n' || c == 0)
}

pub(crate) fn is_forbidden(name: &[u8]) -> bool {
    FORBIDDEN.iter().any(|f| name.eq_ignore_ascii_case(f))
        || crate::ascii_starts_with(name, b"proxy-")
        || crate::ascii_starts_with(name, b"sec-")
        || crate::ascii_starts_with(name, b"x-nd-")
}

pub(crate) fn is_forbidden_method(method: &[u8]) -> bool {
    [&b"CONNECT"[..], b"TRACE", b"TRACK"]
        .iter()
        .any(|m| method.eq_ignore_ascii_case(m))
}

pub(crate) fn normalize_method(method: &[u8]) -> Option<Vec<u8>> {
    if method.is_empty() {
        return None;
    }
    Some(
        if STANDARD_METHODS
            .iter()
            .any(|m| method.eq_ignore_ascii_case(m))
        {
            method.to_ascii_uppercase()
        } else {
            method.to_vec()
        },
    )
}

pub(crate) fn status_text(status: i32) -> &'static str {
    match status {
        100 => "Continue",
        101 => "Switching Protocols",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        203 => "Non-Authoritative Information",
        204 => "No Content",
        205 => "Reset Content",
        206 => "Partial Content",
        300 => "Multiple Choices",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        307 => "Temporary Redirect",
        308 => "Permanent Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        408 => "Request Timeout",
        409 => "Conflict",
        410 => "Gone",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Payload Too Large",
        415 => "Unsupported Media Type",
        416 => "Range Not Satisfiable",
        418 => "I'm a teapot",
        422 => "Unprocessable Entity",
        425 => "Too Early",
        426 => "Upgrade Required",
        428 => "Precondition Required",
        429 => "Too Many Requests",
        431 => "Request Header Fields Too Large",
        451 => "Unavailable For Legal Reasons",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        505 => "HTTP Version Not Supported",
        _ => "",
    }
}

fn trim_tabs_spaces(mut value: &[u8]) -> &[u8] {
    while let [b' ' | b'\t', rest @ ..] = value {
        value = rest;
    }
    while let [rest @ .., b' ' | b'\t'] = value {
        value = rest;
    }
    value
}

pub(crate) fn raw_header_lines(raw: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    raw.split(|&b| b == b'\n').filter_map(|line| {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let colon = line.iter().position(|&b| b == b':')?;
        Some((&line[..colon], trim_tabs_spaces(&line[colon + 1..])))
    })
}

pub(crate) fn raw_headers_have(raw: Option<&[u8]>, name: &[u8]) -> bool {
    let Some(raw) = raw else {
        return false;
    };
    raw.split(|&b| b == b'\n').any(|line| {
        crate::ascii_starts_with(line, name) && {
            let rest = &line[name.len()..];
            let rest = rest
                .iter()
                .position(|&b| b != b' ' && b != b'\t')
                .map_or(&[][..], |at| &rest[at..]);
            rest.first() == Some(&b':')
        }
    })
}

pub(crate) fn cors_allows(
    doc_url: Option<&[u8]>,
    resp_url: Option<&[u8]>,
    header: Option<&[u8]>,
) -> bool {
    if let Some(resp) = resp_url {
        if [&b"data:"[..], b"blob:", b"about:"]
            .iter()
            .any(|p| resp.starts_with(p))
        {
            return true;
        }
        if doc_url.is_some_and(|doc| doc.starts_with(b"file:")) && resp.starts_with(b"file:") {
            return true;
        }
    }
    if ffi::url_same_origin(doc_url, resp_url) {
        return true;
    }
    let Some(header) = header.filter(|h| !h.is_empty()) else {
        return false;
    };
    let trimmed = header.trim_ascii();
    if trimmed == b"*" {
        return true;
    }
    if trimmed.is_empty() {
        return false;
    }
    ffi::url_origin_from(doc_url)
        .is_some_and(|origin| !origin.is_empty() && trimmed.eq_ignore_ascii_case(&origin))
}
