//! Southstar — opening a page: the safe-browsing gate, local paths, HTTPS-first, error pages, turning images, PDFs, JSON, XML and text into documents, parsing and the security state.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int};
use core::sync::atomic::{AtomicI32, Ordering};
use std::ffi::CString;
use std::sync::Mutex;

use southstar_glib::GStr;

use crate::ffi::{self, NsBrowser, Post, Response};

const UNSAFE_CONTINUE_SCHEME: &[u8] = b"southstar-unsafe-continue:";
const SEC_NONE: c_int = 0;
const SEC_SECURE: c_int = 1;
const SEC_INVALID: c_int = 2;
const SEC_PLAIN: c_int = 3;
const HTML_UTF8: &CStr = c"text/html; charset=utf-8";

static PENDING_REFERRER: Mutex<Option<CString>> = Mutex::new(None);
static PENDING_USER_ACTIVATED: AtomicI32 = AtomicI32::new(-1);

pub fn set_next_referrer(url: Option<&CStr>) {
    if let Ok(mut pending) = PENDING_REFERRER.lock() {
        *pending = url.filter(|u| !u.is_empty()).map(CStr::to_owned);
    }
}

pub fn set_next_user_activated(user_activated: bool) {
    PENDING_USER_ACTIVATED.store(c_int::from(user_activated), Ordering::Relaxed);
}

pub struct Viewport {
    pub width: c_int,
    pub height: f64,
    pub settle_ms: c_int,
}

fn starts_ci(text: Option<&CStr>, prefix: &[u8]) -> bool {
    text.is_some_and(|t| {
        let t = t.to_bytes();
        t.len() >= prefix.len() && t[..prefix.len()].eq_ignore_ascii_case(prefix)
    })
}

fn contains(text: &CStr, needle: &[u8]) -> bool {
    text.to_bytes().windows(needle.len()).any(|w| w == needle)
}

fn is_html(ct: Option<&CStr>) -> bool {
    starts_ci(ct, b"text/html") || starts_ci(ct, b"application/xhtml")
}

fn is_json(ct: Option<&CStr>) -> bool {
    starts_ci(ct, b"application/json")
        || starts_ci(ct, b"text/json")
        || ct.is_some_and(|c| contains(c, b"+json"))
}

fn is_xml(ct: Option<&CStr>) -> bool {
    let Some(c) = ct else {
        return false;
    };
    if contains(c, b"xhtml") || contains(c, b"svg") {
        return false;
    }
    starts_ci(ct, b"text/xml") || starts_ci(ct, b"application/xml") || contains(c, b"+xml")
}

fn is_pdf(ct: Option<&CStr>) -> bool {
    starts_ci(ct, b"application/pdf") || starts_ci(ct, b"application/x-pdf")
}

fn text_document(url: &CStr, text: Option<&CStr>) -> Option<GStr> {
    let title = if url.is_empty() { c"text file" } else { url };
    let mut html = b"<!doctype html><html><head><meta charset=\"utf-8\"><title>".to_vec();
    if let Some(esc_url) = ffi::html_escape_text(title) {
        html.extend_from_slice(esc_url.to_bytes());
        html.extend_from_slice(
            b"</title><style>body{margin:0;background:#fff;color:#111}pre{margin:0;padding:12px;\
font:13px/1.45 ui-monospace,\"SF Mono\",Menlo,Consolas,monospace;white-space:pre-wrap;\
overflow-wrap:anywhere}</style></head><body><pre>",
        );
        if let Some(esc_text) = ffi::html_escape_text(text.unwrap_or(c"")) {
            html.extend_from_slice(esc_text.to_bytes());
            html.extend_from_slice(b"</pre></body></html>");
        }
    }
    ffi::gstr_from(&html)
}

fn prepare_document_response(resp: &mut Response) {
    let Some(ct) = resp.content_type() else {
        return;
    };
    if !resp.has_body() {
        return;
    }
    let ct = Some(ct);
    let final_url = resp
        .final_url()
        .map_or_else(|| c"".to_owned(), CStr::to_owned);
    let html = if starts_ci(ct, b"image/") {
        ffi::image_document(&final_url)
    } else if is_pdf(ct) {
        resp.pdf_document(&final_url)
    } else if is_json(ct) {
        let (decoded, _) = resp.decode_body(false);
        ffi::json_document(&final_url, decoded.as_deref())
            .or_else(|| text_document(&final_url, decoded.as_deref()))
    } else if is_xml(ct) {
        let (decoded, _) = resp.decode_body(false);
        ffi::xml_document(&final_url, decoded.as_deref())
            .or_else(|| text_document(&final_url, decoded.as_deref()))
    } else if starts_ci(ct, b"text/") && !is_html(ct) {
        let (decoded, _) = resp.decode_body(false);
        text_document(&final_url, decoded.as_deref())
    } else {
        None
    };
    let Some(html) = html else {
        return;
    };
    resp.set_body(html.to_bytes());
    resp.set_content_type(HTML_UTF8);
}

fn headers_have_no_store(raw: Option<&CStr>) -> bool {
    raw.is_some_and(|r| {
        r.to_bytes()
            .to_ascii_lowercase()
            .windows(8)
            .any(|w| w == b"no-store")
    })
}

fn resolve_local_path(url: &CStr) -> Option<GStr> {
    let u = url.to_bytes();
    if contains(url, b"://")
        || u.starts_with(b"about:")
        || u.starts_with(b"data:")
        || !ffi::file_exists(url)
    {
        return None;
    }
    ffi::local_file_uri(url)
}

fn show_interstitial(url: &CStr, host: &CStr, view: &Viewport) -> &'static NsBrowser {
    let html = ffi::safebrowsing_interstitial(url, host);
    let doc = ffi::html_parse(html.as_deref(), true);
    drop(html);
    ffi::create_browser(ffi::Created {
        doc,
        base: ffi::dup_text(url),
        view,
        bfcache_ok: false,
        refresh_header: None,
        doc_language: None,
        csp_header: None,
        doc_charset: ffi::dup_text(c"UTF-8"),
        url: Some(url),
        timing: None,
    })
}

fn error_page(resp: &mut Response, fetch_url: &CStr) {
    let tls_failure = resp.error().is_some() && fetch_url.to_bytes().starts_with(b"https://");
    let status = if resp.error().is_some() {
        0
    } else {
        resp.status()
    };
    let error = resp.error().map(CStr::to_owned);
    let Some(html) = ffi::build_error_page(fetch_url, status, error.as_deref()) else {
        return;
    };
    resp.set_body(html.to_bytes());
    drop(html);
    resp.clear_error();
    resp.set_content_type(HTML_UTF8);
    resp.set_final_url(fetch_url);
    if tls_failure {
        resp.set_security(SEC_INVALID);
    }
}

fn security_of(resp: &Response, fetch_url: &CStr) -> c_int {
    let sec = resp.security();
    if sec != SEC_NONE {
        return sec;
    }
    let u = resp.final_url().unwrap_or(fetch_url).to_bytes();
    if u.starts_with(b"https://") {
        SEC_SECURE
    } else if u.starts_with(b"http://") {
        SEC_PLAIN
    } else {
        sec
    }
}

pub fn open(url: &CStr, view: &Viewport, post: Option<&Post<'_>>) -> Option<&'static NsBrowser> {
    if url.is_empty() {
        return None;
    }
    let referrer = PENDING_REFERRER.lock().ok().and_then(|mut p| p.take());
    let pending_user_activated = PENDING_USER_ACTIVATED.swap(-1, Ordering::Relaxed);

    if url.to_bytes().starts_with(UNSAFE_CONTINUE_SCHEME) {
        let mut rest = url.to_bytes();
        while rest.starts_with(UNSAFE_CONTINUE_SCHEME) {
            rest = &rest[UNSAFE_CONTINUE_SCHEME.len()..];
        }
        if rest.is_empty() {
            return None;
        }
        let real = CString::new(rest).unwrap_or_default();
        if let Some(host) = ffi::url_host_from(&real) {
            ffi::safebrowsing_allow_host(&host);
        }
        return open(&real, view, post);
    }

    if post.is_none() {
        if let Some(host) = ffi::url_host_from(url) {
            if ffi::safebrowsing_blocked(&host) {
                return Some(show_interstitial(url, &host, view));
            }
        }
    }

    let file_url = resolve_local_path(url);
    let mut fetch_url: &CStr = file_url.as_deref().unwrap_or(url);
    let stripped = if post.is_none() {
        ffi::url_strip_tracking_params(fetch_url)
    } else {
        None
    };
    if let Some(stripped) = &stripped {
        fetch_url = stripped;
    }
    let https_url = if post.is_none() {
        ffi::https_first_upgrade(fetch_url)
    } else {
        None
    };
    let user_activated = if pending_user_activated >= 0 {
        pending_user_activated != 0
    } else {
        referrer.is_none()
    };
    let mut resp = None;
    if let Some(https_url) = &https_url {
        let upgraded = Response::navigate(https_url, referrer.as_deref(), user_activated, None);
        if let Some(r) = upgraded.filter(|r| r.error().is_none() && r.has_body()) {
            fetch_url = https_url;
            resp = Some(r);
        }
    }
    let mut resp = match resp {
        Some(r) => Some(r),
        None => Response::navigate(fetch_url, referrer.as_deref(), user_activated, post),
    };
    if let Some(r) = resp.as_mut() {
        if post.is_none() && (r.error().is_some() || r.status() >= 400) && r.body().is_empty() {
            error_page(r, fetch_url);
        }
    }
    let mut resp = resp.filter(|r| r.error().is_none() && r.has_body())?;

    let mut base = ffi::dup_text(resp.final_url().unwrap_or(fetch_url));
    if post.is_none() {
        if let Some(stripped) = base.as_deref().and_then(ffi::url_strip_tracking_params) {
            base = Some(stripped);
        }
    }
    let base_bytes = base.as_deref().map_or(&b""[..], CStr::to_bytes);
    let bfcache_ok = post.is_none()
        && (base_bytes.starts_with(b"http://") || base_bytes.starts_with(b"https://"))
        && (200..400).contains(&resp.status())
        && !headers_have_no_store(resp.raw_headers());
    let refresh_header = resp.refresh().and_then(ffi::dup_text);
    let doc_language = resp.content_language().and_then(ffi::dup_text);
    let csp_header = resp.csp_header().and_then(ffi::dup_text);
    prepare_document_response(&mut resp);
    let (decoded, doc_charset) = resp.decode_body(true);
    let scripting = southstar_config::get().is_none_or(|c| c.javascript_enabled != 0);
    let doc = ffi::html_parse(Some(decoded.as_deref().unwrap_or(c"")), scripting);
    drop(decoded);
    let security = security_of(&resp, fetch_url);
    let remote_ip = resp.remote_ip().and_then(ffi::dup_text);
    let timing = resp.timing();
    drop(resp);

    let b = ffi::create_browser(ffi::Created {
        doc,
        base,
        view,
        bfcache_ok,
        refresh_header,
        doc_language,
        csp_header,
        doc_charset,
        url: Some(url),
        timing: Some(&timing),
    });
    b.security.set(security);
    b.remote_ip.adopt(remote_ip);
    Some(b)
}
