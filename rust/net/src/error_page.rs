//! Southstar — the page shown when a load fails: the failure classified from the transport error or HTTP status, with an icon, an explanation and what to try.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_long;

use southstar_html_util::escape_text;

macro_rules! icon {
    ($paths:expr) => {
        concat!(
            "<svg width=\"30\" height=\"30\" viewBox=\"0 0 16 16\" fill=\"none\" ",
            "stroke=\"currentColor\" stroke-width=\"1.4\" ",
            "stroke-linecap=\"round\" stroke-linejoin=\"round\">",
            $paths,
            "</svg>"
        )
    };
}

const ICON_OFFLINE: &str = icon!(concat!(
    "<path d=\"M1.75 6.25a9 9 0 0 1 12.5 0M4 8.75a5.75 5.75 0 0 1 8",
    " 0M6.25 11.1a2.5 2.5 0 0 1 3.5 0M2.75 2l10.5 12\"/>"
));
const ICON_SEARCH: &str = icon!(concat!(
    "<circle cx=\"7\" cy=\"7\" r=\"4.75\"/>",
    "<path d=\"M10.5 10.5 14 14\"/>"
));
const ICON_BLOCKED: &str = icon!(concat!(
    "<circle cx=\"8\" cy=\"8\" r=\"5.75\"/>",
    "<path d=\"M3.95 3.95l8.1 8.1\"/>"
));
const ICON_CLOCK: &str = icon!(concat!(
    "<circle cx=\"8\" cy=\"8\" r=\"5.75\"/>",
    "<path d=\"M8 4.75V8l2.25 1.5\"/>"
));
const ICON_LOCK: &str = icon!(concat!(
    "<rect x=\"3.25\" y=\"7\" width=\"9.5\" height=\"7\" rx=\"1.75\"/>",
    "<path d=\"M5.25 7V5.25a2.75 2.75 0 0 1 5.5 0V7\"/>"
));
const ICON_LINK: &str = icon!(concat!(
    "<path d=\"M6.75 9.25l2.5-2.5M7.25 4.5l1-1a2.47 2.47 0 0 1 3.5",
    " 3.5l-1 1M8.75 11.5l-1 1a2.47 2.47 0 0 1-3.5-3.5l1-1\"/>"
));
const ICON_PAGE: &str = icon!(concat!(
    "<path d=\"M4 1.75h4.75L12 5v9.25H4zM8.5 1.75V5.25H12M6.25",
    " 8.25l3.5 3.5M9.75 8.25l-3.5 3.5\"/>"
));
const ICON_SERVER: &str = icon!(concat!(
    "<rect x=\"2.75\" y=\"2.75\" width=\"10.5\" height=\"4.25\"",
    " rx=\"1.25\"/>",
    "<rect x=\"2.75\" y=\"9\" width=\"10.5\" height=\"4.25\"",
    " rx=\"1.25\"/>",
    "<path d=\"M5 4.9h.5M5 11.1h.5\"/>"
));
const ICON_ALERT: &str = icon!("<path d=\"M8 2.25 14.25 13.25H1.75zM8 6.5v3M8 11.4v.1\"/>");

struct ErrorInfo {
    icon: &'static str,
    title: &'static str,
    summary: &'static str,
}

const fn info(icon: &'static str, title: &'static str, summary: &'static str) -> ErrorInfo {
    ErrorInfo {
        icon,
        title,
        summary,
    }
}

const NO_NETWORK: ErrorInfo = info(
    ICON_OFFLINE,
    "Can't reach the network",
    "Southstar couldn't connect to any server. Your device may be offline, or a firewall is blocking outbound traffic.",
);
const DNS: ErrorInfo = info(
    ICON_SEARCH,
    "Server address not found",
    "Southstar couldn't look up the host name. The address may be mistyped, or your DNS resolver isn't responding.",
);
const REFUSED: ErrorInfo = info(
    ICON_BLOCKED,
    "Server refused the connection",
    "The host is reachable but no service is listening on that port, or it actively closed the connection.",
);
const TIMEOUT: ErrorInfo = info(
    ICON_CLOCK,
    "The connection timed out",
    "The server didn't respond within the allowed time. It may be overloaded or temporarily unreachable.",
);
const TLS: ErrorInfo = info(
    ICON_LOCK,
    "Secure connection failed",
    "Southstar couldn't establish a trustworthy TLS connection. The certificate may be invalid, expired, or self-signed.",
);
const BAD_URL: ErrorInfo = info(
    ICON_LINK,
    "That address looks malformed",
    "The URL couldn't be parsed. Check for typos, missing slashes, or an unsupported scheme.",
);
const HTTP_404: ErrorInfo = info(
    ICON_PAGE,
    "Page not found",
    "The server is reachable, but it has no resource at that URL. The link may be outdated or the page may have moved.",
);
const HTTP_410: ErrorInfo = info(
    ICON_PAGE,
    "This page is gone",
    "The server is telling us the resource has been permanently removed.",
);
const HTTP_401: ErrorInfo = info(
    ICON_LOCK,
    "Authentication required",
    "The server needs credentials Southstar doesn't have. Sign in elsewhere first, or try a different URL.",
);
const HTTP_403: ErrorInfo = info(
    ICON_BLOCKED,
    "Access denied",
    "The server understood the request but refused to share this resource with us.",
);
const HTTP_429: ErrorInfo = info(
    ICON_CLOCK,
    "Too many requests",
    "The server is throttling us. Wait a moment and try again.",
);
const HTTP_500: ErrorInfo = info(
    ICON_SERVER,
    "Server error",
    "The server hit an internal error processing this request. Nothing to do on our end — try again later.",
);
const HTTP_502: ErrorInfo = info(
    ICON_SERVER,
    "Bad gateway",
    "An upstream server returned an invalid response. The site's infrastructure may be misconfigured.",
);
const HTTP_503: ErrorInfo = info(
    ICON_SERVER,
    "Service unavailable",
    "The server is temporarily refusing requests, usually because it is overloaded or down for maintenance.",
);
const HTTP_504: ErrorInfo = info(
    ICON_CLOCK,
    "Gateway timeout",
    "An upstream server didn't answer in time.",
);
const HTTP_GENERIC_4XX: ErrorInfo = info(
    ICON_ALERT,
    "Request rejected",
    "The server didn't accept this request.",
);
const HTTP_GENERIC_5XX: ErrorInfo = info(
    ICON_SERVER,
    "Server error",
    "The server reported a failure handling this request.",
);
const GENERIC: ErrorInfo = info(
    ICON_ALERT,
    "Couldn't load page",
    "Something went wrong fetching this URL.",
);
const FILE_MISSING: ErrorInfo = info(
    ICON_PAGE,
    "File not found",
    "There is nothing at that path. The file may have been moved, renamed or deleted.",
);
const FILE_DENIED: ErrorInfo = info(
    ICON_LOCK,
    "Cannot read that file",
    "The file exists but this account is not allowed to read it.",
);

fn contains(hay: &[u8], needle: &str) -> bool {
    hay.windows(needle.len()).any(|w| w == needle.as_bytes())
}

fn classify(
    status: c_long,
    transport_error: Option<&[u8]>,
    is_file_url: bool,
) -> &'static ErrorInfo {
    if is_file_url {
        if status == 403 || transport_error.is_some_and(|e| contains(e, "ermission")) {
            return &FILE_DENIED;
        }
        return &FILE_MISSING;
    }
    if let Some(e) = transport_error.filter(|e| !e.is_empty()) {
        let any = |needles: &[&str]| needles.iter().any(|n| contains(e, n));
        if any(&["Could not resolve", "resolve host", "name resolution"]) {
            return &DNS;
        }
        if any(&["Connection refused", "refused"]) {
            return &REFUSED;
        }
        if any(&["imed out", "Timeout"]) {
            return &TIMEOUT;
        }
        if any(&["SSL", "TLS", "certificate"]) {
            return &TLS;
        }
        if any(&["URL", "Protocol", "malformed"]) {
            return &BAD_URL;
        }
        if any(&["network", "unreachable", "No route", "onnect"]) {
            return &NO_NETWORK;
        }
    }
    match status {
        401 => &HTTP_401,
        403 => &HTTP_403,
        404 => &HTTP_404,
        410 => &HTTP_410,
        429 => &HTTP_429,
        500 => &HTTP_500,
        502 => &HTTP_502,
        503 => &HTTP_503,
        504 => &HTTP_504,
        s if (500..600).contains(&s) => &HTTP_GENERIC_5XX,
        s if (400..500).contains(&s) => &HTTP_GENERIC_4XX,
        _ if transport_error.is_some_and(|e| !e.is_empty()) => &NO_NETWORK,
        _ => &GENERIC,
    }
}

const STYLE: &str = concat!(
    "body{display:flex;align-items:center;justify-content:center;",
    "min-height:100vh;padding:32px 18px}",
    ".card{width:100%;max-width:600px;padding:36px 38px 30px}",
    ".icon{display:flex;align-items:center;justify-content:center;",
    "width:64px;height:64px;margin-bottom:20px;border-radius:20px;",
    "background:var(--accent-soft);color:var(--accent)}",
    "h1{margin:0 0 10px;font-size:24px;letter-spacing:-.01em;",
    "line-height:1.25}",
    "p.summary{margin:0 0 20px;color:var(--muted);font-size:15.5px}",
    ".url{margin:0 0 10px;padding:10px 14px;border-radius:12px;",
    "background:var(--field);font-family:var(--mono);font-size:13px;",
    "color:var(--muted);overflow-wrap:anywhere}",
    ".detail{margin:0 0 22px;font-family:var(--mono);font-size:12px;",
    "color:var(--faint);overflow-wrap:anywhere}",
    ".actions{display:flex;flex-wrap:wrap;gap:10px;margin-top:22px}",
    ".tips{margin-top:26px;padding-top:20px;",
    "border-top:1px solid var(--line);",
    "color:var(--muted);font-size:13.5px}",
    ".tips strong{color:var(--text)}",
    ".tips ul{margin:8px 0 0;padding-left:20px}",
    ".tips li{margin:4px 0}"
);

pub fn build(url: Option<&[u8]>, status: c_long, transport_error: Option<&[u8]>) -> Vec<u8> {
    let is_file_url = url.is_some_and(|u| u.starts_with(b"file:"));
    let info = classify(status, transport_error, is_file_url);
    let url = url.filter(|u| !u.is_empty());
    let detail = match transport_error.filter(|e| !e.is_empty()) {
        Some(e) => {
            let mut d = b"Technical detail: ".to_vec();
            d.extend_from_slice(&escape_text(e));
            Some(d)
        }
        None => (status > 0).then(|| format!("HTTP status: {status}").into_bytes()),
    };
    let can_retry = url.is_some_and(|u| !u.starts_with(b"about:"));
    let mut out = Vec::new();
    out.extend_from_slice(b"<!doctype html><html><head><meta charset=\"utf-8\"><title>");
    out.extend_from_slice(&escape_text(info.title.as_bytes()));
    out.extend_from_slice(
        " — Southstar</title><meta name=\"color-scheme\" content=\"light dark\"><style>".as_bytes(),
    );
    out.extend_from_slice(southstar_about_style::base_css().as_bytes());
    out.extend_from_slice(STYLE.as_bytes());
    out.extend_from_slice(b"</style></head><body><main class=\"card\"><div class=\"icon\">");
    out.extend_from_slice(info.icon.as_bytes());
    out.extend_from_slice(b"</div><h1>");
    out.extend_from_slice(&escape_text(info.title.as_bytes()));
    out.extend_from_slice(b"</h1><p class=\"summary\">");
    out.extend_from_slice(&escape_text(info.summary.as_bytes()));
    out.extend_from_slice(b"</p><p class=\"url\">");
    out.extend_from_slice(&escape_text(url.unwrap_or(b"(no URL)")));
    out.extend_from_slice(b"</p>");
    if let Some(detail) = detail {
        out.extend_from_slice(b"<p class=\"detail\">");
        out.extend_from_slice(&detail);
        out.extend_from_slice(b"</p>");
    }
    out.extend_from_slice(b"<div class=\"actions\">");
    if can_retry {
        out.extend_from_slice(b"<a class=\"btn primary\" href=\"");
        out.extend_from_slice(&url.map(escape_text).unwrap_or_default());
        out.extend_from_slice(b"\">Try again</a>");
    }
    out.extend_from_slice(
        b"<button class=\"btn\" onclick=\"history.back()\">Go back</button>\
<a class=\"btn\" href=\"about:start\">New Tab</a></div><div class=\"tips\"><strong>What to try:</strong><ul>",
    );
    if is_file_url {
        out.extend_from_slice(
            b"<li>Check the path in the address bar for typos.</li>\
<li>Confirm the file is still where you expect it.</li>\
<li>Open the enclosing folder to browse what is there.</li>",
        );
    } else {
        out.extend_from_slice(
            "<li>Double-check the address bar for typos.</li>\
<li>Make sure your internet connection is working.</li>\
<li>Reload the page in a moment — temporary outages do happen.</li>"
                .as_bytes(),
        );
    }
    out.extend_from_slice(b"</ul></div></main></body></html>");
    out
}
