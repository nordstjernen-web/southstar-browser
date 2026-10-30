//! Southstar — fetching a URL: one hop with its synthesized responses, cache lookup and revalidation, cookies, request headers and logging, the redirect loop around it, the Accept lines per destination and the coalescing key.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{CStr, c_int, c_long};

use southstar_glib as glib;
use southstar_http_cache as http_cache;

use super::hop::{self, HopOut, HopReq};
use super::proxy::{configured_no_proxy, pick_configured_proxy};
use super::queue::Cancellable;
use super::sinks::{NsHeaderCtx, NsWriteCtx};
use crate::request::{self, DEFAULT_TIMEOUT_S, MAX_TIMEOUT_S};
use crate::response::{Response, cstr, text as field};
use crate::{credentials, hsts, netlog, storage, transport, url};

const REFERER_STRICT_ORIGIN_WHEN_CROSS: c_int = 2;
const SECURITY_SECURE: c_int = 1;
const SECURITY_INVALID: c_int = 2;
const SECURITY_PLAIN: c_int = 3;

const FETCH_DEST_SCRIPT: c_int = 1;
const FETCH_DEST_STYLE: c_int = 2;
const FETCH_DEST_IMAGE: c_int = 3;
const FETCH_DEST_FONT: c_int = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FetchDest {
    Script,
    Style,
    Image,
    Font,
}

impl FetchDest {
    fn from_raw(dest: c_int) -> Option<FetchDest> {
        match dest {
            FETCH_DEST_SCRIPT => Some(FetchDest::Script),
            FETCH_DEST_STYLE => Some(FetchDest::Style),
            FETCH_DEST_IMAGE => Some(FetchDest::Image),
            FETCH_DEST_FONT => Some(FetchDest::Font),
            _ => None,
        }
    }

    pub fn accept_lines(self) -> &'static [&'static CStr] {
        match self {
            FetchDest::Script => &SCRIPT_LINES,
            FetchDest::Style => &STYLE_LINES,
            FetchDest::Image => &IMAGE_LINES,
            FetchDest::Font => &FONT_LINES,
        }
    }

    pub fn header_lines(self) -> Vec<Vec<u8>> {
        self.accept_lines()
            .iter()
            .map(|line| line.to_bytes().to_vec())
            .collect()
    }
}

const SCRIPT_LINES: [&CStr; 2] = [
    c"Accept: text/javascript, application/javascript, application/ecmascript, application/x-javascript, */*;q=0.8",
    c"X-ND-Fetch-Dest: script",
];
const STYLE_LINES: [&CStr; 2] = [c"Accept: text/css,*/*;q=0.1", c"X-ND-Fetch-Dest: style"];
#[cfg(all(feature = "avif", not(windows)))]
const IMAGE_ACCEPT_TYPES: &CStr =
    c"Accept: image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";
#[cfg(not(all(feature = "avif", not(windows))))]
const IMAGE_ACCEPT_TYPES: &CStr = c"Accept: image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";
const IMAGE_LINES: [&CStr; 2] = [IMAGE_ACCEPT_TYPES, c"X-ND-Fetch-Dest: image"];
const FONT_LINES: [&CStr; 2] = [
    c"Accept: font/woff2,font/woff,application/font-woff,application/octet-stream;q=0.8,*/*;q=0.5",
    c"X-ND-Fetch-Dest: font",
];

#[derive(Clone)]
pub enum Failure {
    Cancelled,
    Net(c_int, Vec<u8>),
}

impl Failure {
    pub fn message(&self) -> &[u8] {
        match self {
            Failure::Cancelled => b"fetch cancelled",
            Failure::Net(_, message) => message,
        }
    }
}

struct OriginSlot(Vec<u8>);

impl Drop for OriginSlot {
    fn drop(&mut self) {
        transport::release_origin_slot(&self.0);
    }
}

pub struct Fetch<'a> {
    pub url: &'a [u8],
    pub top_url: Option<&'a [u8]>,
    pub method: Option<&'a [u8]>,
    pub body: Option<&'a [u8]>,
    pub content_type: Option<&'a [u8]>,
    pub headers: &'a [Vec<u8>],
    pub cancellable: Option<&'a Cancellable>,
    pub max_bytes: Option<u64>,
}

pub fn widen<T: Into<i64>>(value: T) -> i64 {
    value.into()
}

fn without_nul(s: &'static str) -> &'static [u8] {
    s.trim_end_matches('\0').as_bytes()
}

pub fn is_cancelled(cancellable: Option<&Cancellable>) -> bool {
    cancellable.is_some_and(Cancellable::is_cancelled)
}

fn monotonic_us() -> i64 {
    super::sys::monotonic_us()
}

fn real_time_ms() -> f64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(after) => after.as_micros() as f64 / 1000.0,
        Err(before) => -(before.duration().as_micros() as f64) / 1000.0,
    }
}

fn accept_language() -> Vec<u8> {
    southstar_netutil::effective_accept_language()
}

fn user_agent(cfg: Option<&southstar_config::NsConfig>) -> Vec<u8> {
    let configured = cfg.and_then(|c| c.user_agent.non_empty());
    configured.map_or_else(
        || {
            without_nul(url::user_agent_for_mode(
                cfg.and_then(|c| c.compat_mode.bytes()),
            ))
            .to_vec()
        },
        <[u8]>::to_vec,
    )
}

fn cache_partition(url: &[u8], top_url: Option<&[u8]>) -> Vec<u8> {
    let ua = user_agent(southstar_config::get());
    let effective_top = top_url.unwrap_or(url);
    let top_origin = url::origin_from(effective_top);
    let top_site = url::site_from(effective_top);
    let partition = request::partition_of(top_site.as_deref(), top_origin.as_deref());
    request::partition_key(&partition, &ua, &accept_language())
}

fn timeout_seconds(headers: &[Vec<u8>]) -> c_long {
    let requested = request::timeout_header(headers).map_or(DEFAULT_TIMEOUT_S as c_long, |v| {
        southstar_config::util::ascii_strtoll(v).unwrap_or(0) as c_long
    });
    requested.clamp(1, MAX_TIMEOUT_S as c_long)
}

fn host_is_dead(url: &[u8]) -> bool {
    url::origin_from_any(url).is_some_and(|o| !o.is_empty() && transport::host_recently_dead(&o))
}

fn synthesized(f: &Fetch, resp: &mut Response) -> bool {
    let url = cstr(f.url);
    let url = url.to_bytes();
    let top = f.top_url.map(cstr);
    let top = top.as_deref().map(CStr::to_bytes);
    let method = f.method.map(cstr);
    let method = method.as_deref().map(CStr::to_bytes);
    super::synthesize_about_response(url, top, method, f.body, resp)
        || super::synthesize_view_source_response(url, top, f.cancellable, resp)
        || super::synthesize_data_response(url, resp)
        || super::synthesize_file_response(url, top, resp)
}

fn upgraded(url: &[u8]) -> Vec<u8> {
    let mut current = url.to_vec();
    if let Some(ascii) = url::to_ascii(&current)
        && ascii != current
    {
        current = ascii;
    }
    if let Some(https) = hsts::upgrade(&current) {
        current = https;
    }
    current
}

#[allow(clippy::too_many_arguments)]
fn shape_headers(
    f: &Fetch,
    url: &[u8],
    cfg: Option<&southstar_config::NsConfig>,
    ua: &[u8],
    vary_language: &[u8],
    vary_origin: Option<&[u8]>,
    cached: Option<&http_cache::Entry>,
    request_http: bool,
) -> Vec<Vec<u8>> {
    let is_navigation = request::is_navigation(f.headers);
    let mut lines = vec![vary_language.to_vec()];
    let destination = if is_navigation {
        "document"
    } else {
        request::fetch_destination(f.headers)
    };
    let navigates = is_navigation || request::is_nested_navigation(destination);
    if request::caller_accept(f.headers).is_none() {
        lines.push(
            if navigates {
                request::DOCUMENT_ACCEPT
            } else {
                request::ANY_ACCEPT
            }
            .to_vec(),
        );
    }
    if cfg.is_none_or(|c| c.do_not_track != 0) {
        lines.push(b"DNT: 1".to_vec());
    }
    if let Some(origin) = vary_origin {
        lines.push(origin.to_vec());
    }
    if let Some(entry) = cached {
        if let Some(etag) = &entry.etag {
            lines.push([b"If-None-Match: ", &etag[..]].concat());
        }
        if let Some(modified) = &entry.last_modified {
            lines.push([b"If-Modified-Since: ", &modified[..]].concat());
        }
    }
    if request_http {
        let initiator = request::site_initiator(is_navigation, f.headers, f.top_url);
        let site = request::fetch_site(initiator, url);
        lines.push(format!("Sec-Fetch-Site: {site}").into_bytes());
        let mode = request::fetch_mode(navigates, destination, f.method);
        lines.push(format!("Sec-Fetch-Mode: {mode}").into_bytes());
        lines.push(format!("Sec-Fetch-Dest: {destination}").into_bytes());
        if is_navigation && request::has_user_activation(f.headers) {
            lines.push(b"Sec-Fetch-User: ?1".to_vec());
        }
        if navigates {
            lines.push(b"Upgrade-Insecure-Requests: 1".to_vec());
        }
        if url::has_client_hints(ua) {
            lines.push(request::CLIENT_HINTS_BRANDS.to_vec());
            let mobile = super::url::is_mobile_mode();
            lines.push(format!("Sec-CH-UA-Mobile: ?{}", u8::from(mobile)).into_bytes());
            let platform = without_nul(url::HINT_PLATFORM);
            lines.push([b"Sec-CH-UA-Platform: \"", platform, b"\""].concat());
        }
        if cfg.is_none_or(|c| c.global_privacy_control != 0) {
            lines.push(b"Sec-GPC: 1".to_vec());
        }
    }
    let has_body = f.body.is_some_and(|b| !b.is_empty());
    if let Some(line) =
        request::content_type_line(f.method, has_body, f.content_type, is_navigation, f.headers)
    {
        lines.push(line);
    }
    lines.extend(f.headers.iter().filter_map(|h| request::forwarded_line(h)));
    lines
}

fn security_of(final_url: &[u8], out: &HopOut, has_warning: bool) -> c_int {
    if final_url.starts_with(b"https://") {
        if out.tls_verify_failed || has_warning {
            SECURITY_INVALID
        } else if out.ok {
            SECURITY_SECURE
        } else {
            0
        }
    } else if final_url.starts_with(b"http://") {
        SECURITY_PLAIN
    } else {
        0
    }
}

pub fn fetch_hop(f: &Fetch, location: &mut Option<Vec<u8>>) -> Result<Response, Failure> {
    let is_navigation = request::is_navigation(f.headers);
    let mut resp = Response::new();

    if !is_navigation && host_is_dead(f.url) {
        return Ok(Response::with_error(
            f.url,
            0,
            b"host unreachable (recent connection failure)",
        ));
    }
    if synthesized(f, &mut resp) {
        return Ok(resp);
    }

    let url = upgraded(f.url);
    let request_http = url::is_http_or_https(&url);
    let request_ftp = url.starts_with(b"ftp://");
    if request_ftp
        && f.top_url
            .is_some_and(|t| !t.is_empty() && !t.starts_with(b"ftp://"))
        && !is_navigation
    {
        return Ok(Response::with_error(
            &url,
            0,
            b"FTP access is not allowed from this page",
        ));
    }

    let cfg = southstar_config::get();
    let ua = user_agent(cfg);
    let language = accept_language();
    let effective_top = f.top_url.unwrap_or(&url);
    let top_origin = url::origin_from(effective_top);
    let top_site = url::site_from(effective_top);
    let partition = request::partition_of(top_site.as_deref(), top_origin.as_deref());
    let cache_partition = cache_partition(&url, f.top_url);
    let vary_accept = [b"Accept: ", request::requested_accept(f.headers)].concat();
    let vary_language = [b"Accept-Language: ", &language[..]].concat();
    let vary_agent = [b"User-Agent: ", &ua[..]].concat();
    let initiator = request::site_initiator(is_navigation, f.headers, f.top_url);
    let document = request::requesting_document(f.headers, f.top_url);
    let initiator_origin = if is_navigation {
        initiator.and_then(url::origin_from)
    } else {
        document
            .and_then(url::origin_from)
            .or_else(|| top_origin.clone())
    };
    let origin_top = if is_navigation { None } else { document };
    let request_origin =
        request::request_origin(&url, origin_top, initiator_origin.as_deref(), f.method);
    let vary_origin = [b"Origin: ", request_origin].concat();
    let cache_headers: [&[u8]; 4] = [&vary_accept, &vary_language, &vary_agent, &vary_origin];

    let cookies_allowed = request_http
        && request::credentials_permit(f.headers, &url, document)
        && credentials::cookies_allowed(&url, f.top_url, &partition);
    let cookie_jar = if cookies_allowed {
        storage::cookie_jar_path(Some(&partition), false)
    } else {
        None
    };

    let mut cached = None;
    if request_http && request::is_simple_get(f.method) {
        cached = http_cache::get(&url, Some(&cache_partition), &cache_headers);
        if let Some(entry) = cached
            .as_ref()
            .filter(|e| http_cache::is_fresh(e.expires_at))
        {
            let has_cors = entry.cors_allow_origin.is_some()
                || entry
                    .final_url
                    .as_deref()
                    .is_some_and(|fu| url::same_origin(document.unwrap_or(effective_top), fu));
            if has_cors {
                return Ok(Response::from_cache(entry));
            }
            cached = None;
        }
    }

    let referer_policy = cfg.map_or(REFERER_STRICT_ORIGIN_WHEN_CROSS, |c| c.referer_policy);
    let referer = url::referer_for(Some(&url), document, referer_policy);
    let _slot = match url::origin_from(&url) {
        Some(origin) => {
            if !transport::acquire_origin_slot(&origin, || is_cancelled(f.cancellable)) {
                return Err(Failure::Cancelled);
            }
            Some(OriginSlot(origin))
        }
        None => None,
    };

    if std::env::var_os("NS_NET_LOG").is_some() {
        let method = f.method.unwrap_or(b"GET");
        glib::stderr_write(&[b"NS_NET ", method, b" ", &url, b"\n"].concat());
    }

    let timeout = timeout_seconds(f.headers);
    let lines = shape_headers(
        f,
        &url,
        cfg,
        &ua,
        &vary_language,
        (!request_origin.is_empty()).then_some(&vary_origin[..]),
        cached.as_ref(),
        request_http,
    );

    let mut wctx = NsWriteCtx::new(f.max_bytes);
    let mut hctx = NsHeaderCtx::new();

    let url_c = cstr(&url);
    let js_jar = cookie_jar
        .as_ref()
        .and_then(|_| storage::cookie_jar_path(Some(&partition), true));
    let proxy = pick_configured_proxy(&url);
    let no_proxy = configured_no_proxy();
    let req = HopReq {
        url: &url,
        method: f.method,
        body: f.body,
        headers: &lines,
        user_agent: &ua,
        referer: referer.as_deref(),
        timeout_s: timeout,
        connect_timeout_s: if is_navigation {
            request::NAVIGATION_CONNECT_TIMEOUT_S
        } else {
            request::SUBRESOURCE_CONNECT_TIMEOUT_S
        } as c_long,
        proxy: proxy.as_deref(),
        no_proxy: no_proxy.as_deref(),
        cookie_jar_path: cookie_jar.as_deref(),
        cookie_js_path: js_jar.as_deref(),
        cookie_context: request::cookie_context(is_navigation, f.method, initiator, &url),
        request_ftp,
        navigation: is_navigation,
    };

    let mut out = HopOut::default();
    let start_us = monotonic_us();
    let start_real_ms = real_time_ms();

    let produced = hop::transport(&req, &mut wctx, &mut hctx, &mut out, f.cancellable);
    let mut captured = hctx.take();

    if !produced {
        let failure = if out.cancelled {
            Failure::Cancelled
        } else {
            Failure::Net(
                1,
                field(&out.error_message)
                    .unwrap_or(b"transport init failed")
                    .to_vec(),
            )
        };
        return Err(failure);
    }

    let transport_ok = out.ok;
    let r = &mut resp;
    r.body = wctx.take_body();
    hctx.store(r);
    r.status = out.status;
    r.final_url = Some(cstr(field(&out.effective_url).unwrap_or(&url)));
    r.redirect_count = 0;
    r.request_start_us = start_us;
    r.request_start_real_ms = start_real_ms;
    r.domain_lookup_ms = out.t_namelookup_ms;
    r.connect_ms = out.t_connect_ms;
    r.tls_ms = out.t_appconnect_ms;
    r.pretransfer_ms = out.t_pretransfer_ms;
    r.response_start_ms = out.t_starttransfer_ms;
    r.response_end_ms = out.t_total_ms;
    if let Some(remote_ip) = out.remote_ip.take() {
        r.remote_ip = Some(remote_ip);
    }
    if out.http_version != 0 {
        r.next_hop_protocol = Some(netlog::version_name(out.http_version).to_owned());
    }
    if let Some(tls_warning) = out.tls_warning.take() {
        r.tls_warning = Some(tls_warning);
    }
    let final_url = field(&r.final_url).unwrap_or_default().to_vec();
    r.security = security_of(&final_url, &out, r.tls_warning.is_some());
    if netlog::logging() {
        super::netlog::conn_stat_record(&final_url, out.http_version, out.num_connects);
    }
    if transport_ok && request_ftp {
        super::finish_ftp_response(r);
    }

    if let Some(reach) = url::origin_from_any(&url).filter(|o| !o.is_empty()) {
        if transport_ok || out.status > 0 {
            transport::mark_host_alive(&reach);
        } else if out.status == 0 && out.connect_failed {
            transport::mark_host_dead(&reach);
        }
    }

    if !transport_ok {
        let message = if wctx.exceeded() {
            request::too_large_message(wctx.total())
        } else {
            field(&out.error_message)
                .unwrap_or(b"transport error")
                .to_vec()
        };
        r.error = Some(cstr(&message));
    }

    if transport_ok
        && request_http
        && request::is_simple_get(f.method)
        && !captured.set_cookie_seen
        && r.tls_warning.is_none()
    {
        match cached.as_ref().filter(|_| r.status == 304) {
            Some(entry) => {
                http_cache::promote_304(
                    &url,
                    Some(&cache_partition),
                    &cache_headers,
                    captured.cache_control.as_deref(),
                    captured.expires.as_deref(),
                );
                r.body.clear();
                r.body.extend_from_slice(&entry.body);
                r.status = entry.status as c_long;
                r.content_type = entry.content_type.as_deref().map(cstr);
                r.cors_allow_origin = entry.cors_allow_origin.as_deref().map(cstr);
                r.restore_cached_headers(entry.headers.as_deref().unwrap_or_default());
            }
            None if r.status > 0 && r.status < 300 && !r.body.is_empty() => {
                let headers = r.cached_headers();
                let stored = http_cache::Response {
                    final_url: field(&r.final_url),
                    status: widen(r.status),
                    content_type: field(&r.content_type),
                    cors_allow_origin: field(&r.cors_allow_origin),
                    headers: Some(&headers),
                    etag: captured.etag.as_deref(),
                    last_modified: captured.last_modified.as_deref(),
                    cache_control: captured.cache_control.as_deref(),
                    expires_header: captured.expires.as_deref(),
                    vary: captured.vary.as_deref(),
                    body: &r.body,
                };
                http_cache::put(&url, Some(&cache_partition), &stored, &cache_headers);
            }
            None => {}
        }
    }

    *location = captured.location.take();
    if let Some(raw_headers) = captured.take_raw() {
        r.raw_headers = Some(raw_headers);
    }
    let end_us = monotonic_us();
    let body_len = r.body.len() as u64;
    super::netlog::ns_net_perf_record(start_us, end_us, body_len);
    let sent = (!lines.is_empty()).then(|| {
        cstr(&transport::serialize_headers(
            lines.iter().map(Vec::as_slice),
        ))
    });
    let method_log = f.method.map(cstr);
    super::netlog::log_record(&super::netlog::LogRecord {
        method: method_log.as_deref().map(CStr::to_bytes),
        url: url_c.to_bytes(),
        status: r.status,
        content_type: field(&r.content_type),
        body_len,
        duration_ms: (end_us - start_us) as f64 / 1000.0,
        request_headers: sent.as_deref().map(CStr::to_bytes),
        response_headers: field(&r.raw_headers),
        error: field(&r.error),
    });
    Ok(resp)
}

pub fn fetch(f: &Fetch) -> Result<Response, Failure> {
    let navigation = request::is_navigation(f.headers);
    if !navigation && southstar_extensions::should_block(Some(f.url), f.top_url) {
        return Ok(Response::with_error(f.url, 0, b"blocked by extension"));
    }

    let max_redirs =
        request::clamp_redirects(southstar_config::get().map(|c| i64::from(c.max_redirects)));
    let mut url = f.url.to_vec();
    let mut top = f.top_url.map(<[u8]>::to_vec);
    let mut method = f
        .method
        .filter(|m| !m.is_empty())
        .unwrap_or(b"GET")
        .to_vec();
    let mut body = f.body;
    let mut content_type = f.content_type;
    let mut headers = f.headers.to_vec();
    let started_https = f.url.starts_with(b"https://");
    let mut hops: i64 = 0;

    loop {
        let mut location = None;
        let hop = Fetch {
            url: &url,
            top_url: top.as_deref(),
            method: Some(&method),
            body,
            content_type,
            headers: &headers,
            cancellable: f.cancellable,
            max_bytes: f.max_bytes,
        };
        let mut resp = fetch_hop(&hop, &mut location)?;
        let r = &mut resp;
        let status = widen(r.status);
        let location = location.filter(|l| !l.is_empty());
        let Some(location) = location.filter(|_| request::is_redirect(status) && r.error.is_none())
        else {
            r.redirect_count = hops as c_int;
            return Ok(resp);
        };
        if hops >= max_redirs {
            r.error = Some(cstr(b"too many redirects"));
            r.redirect_count = hops as c_int;
            return Ok(resp);
        }
        let base = field(&r.final_url).unwrap_or(&url).to_vec();
        let Some(next) = url::resolve(Some(&base), &location) else {
            r.redirect_count = hops as c_int;
            return Ok(resp);
        };
        if !request::redirect_allowed(&next, started_https, navigation) {
            r.error = Some(cstr(b"redirect to a disallowed URL blocked"));
            r.redirect_count = hops as c_int;
            return Ok(resp);
        }
        if url::origin_from(&base) != url::origin_from(&next) {
            request::strip_sensitive(&mut headers);
        }
        if request::redirect_drops_body(status, &method) {
            method = b"GET".to_vec();
            body = None;
            content_type = None;
            request::strip_body_headers(&mut headers);
        }
        if navigation && method.eq_ignore_ascii_case(b"GET") {
            top = None;
        }
        url = next;
        hops += 1;
        drop(resp);
        if is_cancelled(f.cancellable) {
            return Err(Failure::Cancelled);
        }
    }
}

pub fn request_key(
    url: &[u8],
    top_url: Option<&[u8]>,
    method: Option<&[u8]>,
    headers: &[Vec<u8>],
) -> Option<Vec<u8>> {
    if !request::is_coalescable(url, method) {
        return None;
    }
    let partition = cache_partition(url, top_url);
    let lines: Vec<&[u8]> = headers.iter().map(Vec::as_slice).collect();
    Some(request::coalescing_key(url, &partition, &lines))
}

pub fn accept_header_lines(dest: c_int) -> Vec<Vec<u8>> {
    FetchDest::from_raw(dest).map_or_else(Vec::new, FetchDest::header_lines)
}
