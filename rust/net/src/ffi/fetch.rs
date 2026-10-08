//! Southstar — fetching a URL: one hop with its synthesized responses, cache lookup and revalidation, cookies, request headers and logging, the redirect loop around it, the Accept lines per destination and the coalescing key.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_int, c_long, c_uint, c_void};
use core::ptr::{self, NonNull, addr_of_mut};
use std::ffi::CString;

use southstar_glib::{self as glib, GBoolean, GError, GPtrArray};
use southstar_http_cache as http_cache;

use super::hop::{self, HopOut, HopReq};
use super::proxy::{ns_net_configured_no_proxy, ns_net_pick_configured_proxy};
use super::sinks::{NsHeaderCtx, NsWriteCtx};
use super::transport::{ns_net_accept_encoding, ns_net_http_version, ns_net_slist_serialize};
use super::{GByteArray, NsResponse, curl};
use crate::request::{self, DEFAULT_TIMEOUT_S, MAX_TIMEOUT_S};
use crate::{hsts, netlog, storage, transport, url};

const IO_ERROR_CANCELLED: c_int = 19;
const COOKIE_FIRST_PARTY: c_int = 1;
const COOKIE_NEVER: c_int = 2;
const REFERER_STRICT_ORIGIN_WHEN_CROSS: c_int = 2;
const SECURITY_SECURE: c_int = 1;
const SECURITY_INVALID: c_int = 2;
const SECURITY_PLAIN: c_int = 3;

const FETCH_DEST_SCRIPT: c_int = 1;
const FETCH_DEST_STYLE: c_int = 2;
const FETCH_DEST_IMAGE: c_int = 3;
const FETCH_DEST_FONT: c_int = 4;

unsafe extern "C" {
    fn g_quark_from_static_string(string: *const c_char) -> u32;
    fn g_io_error_quark() -> u32;
    fn g_set_error_literal(err: *mut *mut GError, domain: u32, code: c_int, message: *const c_char);
    fn g_get_monotonic_time() -> i64;
    fn g_get_real_time() -> i64;
    fn g_ascii_strtoll(nptr: *const c_char, endptr: *mut *mut c_char, base: c_uint) -> i64;
    fn g_byte_array_new() -> *mut GByteArray;
    fn g_byte_array_append(array: *mut GByteArray, data: *const u8, len: c_uint)
    -> *mut GByteArray;
    fn g_byte_array_set_size(array: *mut GByteArray, length: c_uint) -> *mut GByteArray;
    fn g_cancellable_is_cancelled(cancellable: *mut c_void) -> GBoolean;
    fn ns_ext_should_block(url: *const c_char, top_url: *const c_char) -> GBoolean;
    fn ns_net_effective_accept_language() -> *const c_char;
}

struct AcceptLines([*const c_char; 3]);

unsafe impl Sync for AcceptLines {}

static SCRIPT_ACCEPT: AcceptLines = AcceptLines([
    c"Accept: text/javascript, application/javascript, application/ecmascript, application/x-javascript, */*;q=0.8".as_ptr(),
    c"X-ND-Fetch-Dest: script".as_ptr(),
    ptr::null(),
]);
static STYLE_ACCEPT: AcceptLines = AcceptLines([
    c"Accept: text/css,*/*;q=0.1".as_ptr(),
    c"X-ND-Fetch-Dest: style".as_ptr(),
    ptr::null(),
]);
static IMAGE_ACCEPT: AcceptLines = AcceptLines([
    c"Accept: image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8".as_ptr(),
    c"X-ND-Fetch-Dest: image".as_ptr(),
    ptr::null(),
]);
static FONT_ACCEPT: AcceptLines = AcceptLines([
    c"Accept: font/woff2,font/woff,application/font-woff,application/octet-stream;q=0.8,*/*;q=0.5"
        .as_ptr(),
    c"X-ND-Fetch-Dest: font".as_ptr(),
    ptr::null(),
]);

pub enum Failure {
    Cancelled,
    Net(c_int, Vec<u8>),
}

impl Failure {
    fn parts(&self) -> (u32, c_int, CString) {
        match self {
            Failure::Cancelled => (
                unsafe { g_io_error_quark() },
                IO_ERROR_CANCELLED,
                c"fetch cancelled".to_owned(),
            ),
            Failure::Net(code, message) => (net_error_domain(), *code, cstr(message)),
        }
    }

    pub fn set(&self, error: *mut *mut GError) {
        let (domain, code, message) = self.parts();
        unsafe { g_set_error_literal(error, domain, code, message.as_ptr()) };
    }
}

pub fn net_error_domain() -> u32 {
    unsafe { g_quark_from_static_string(c"nd-net-error".as_ptr()) }
}

pub struct Response(NonNull<NsResponse>);

unsafe impl Send for Response {}

impl Response {
    pub fn new() -> Response {
        let raw =
            unsafe { glib::g_malloc0(core::mem::size_of::<NsResponse>()) }.cast::<NsResponse>();
        let resp = Response(NonNull::new(raw).expect("g_malloc0 aborts on failure"));
        unsafe { (*resp.as_ptr()).body = g_byte_array_new() };
        resp
    }

    pub fn as_ptr(&self) -> *mut NsResponse {
        self.0.as_ptr()
    }

    pub fn get(&mut self) -> &mut NsResponse {
        unsafe { self.0.as_mut() }
    }

    pub fn into_raw(self) -> *mut NsResponse {
        let raw = self.as_ptr();
        core::mem::forget(self);
        raw
    }

    fn with_error(url: &[u8], status: c_long, error: &[u8]) -> Response {
        let mut resp = Response::new();
        let r = resp.get();
        r.final_url = glib::strdup(url);
        r.status = status;
        r.error = glib::strdup(error);
        resp
    }

    fn from_cache(entry: &http_cache::Entry) -> Response {
        let mut resp = Response::new();
        let r = resp.get();
        r.status = entry.status as c_long;
        r.final_url = dup(entry.final_url.as_deref());
        r.content_type = dup(entry.content_type.as_deref());
        r.cors_allow_origin = dup(entry.cors_allow_origin.as_deref());
        append(r.body, &entry.body);
        resp
    }
}

impl Drop for Response {
    fn drop(&mut self) {
        unsafe { super::ns_response_free(self.as_ptr()) };
    }
}

struct HeaderList(*mut curl::Slist);

impl HeaderList {
    fn new(lines: &[Vec<u8>]) -> HeaderList {
        let mut list = ptr::null_mut();
        for line in lines {
            let line = cstr(line);
            list = unsafe { curl::curl_slist_append(list, line.as_ptr()) };
        }
        HeaderList(list)
    }
}

impl Drop for HeaderList {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { curl::curl_slist_free_all(self.0) };
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
    pub cancellable: *mut c_void,
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

pub fn cstr(bytes: &[u8]) -> CString {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    CString::new(&bytes[..end]).unwrap_or_default()
}

fn opt_cstr(bytes: Option<&[u8]>) -> Option<CString> {
    bytes.map(cstr)
}

fn opt_ptr(value: &Option<CString>) -> *const c_char {
    value.as_ref().map_or(ptr::null(), |c| c.as_ptr())
}

fn dup(value: Option<&[u8]>) -> *mut c_char {
    value.map_or(ptr::null_mut(), glib::strdup)
}

fn append(body: *mut GByteArray, bytes: &[u8]) {
    if !bytes.is_empty() {
        unsafe { g_byte_array_append(body, bytes.as_ptr(), bytes.len() as c_uint) };
    }
}

fn replace(field: &mut *mut c_char, value: Option<&[u8]>) {
    unsafe { glib::g_free((*field).cast()) };
    *field = dup(value);
}

pub fn widen<T: Into<i64>>(value: T) -> i64 {
    value.into()
}

fn body_len(r: &NsResponse) -> usize {
    unsafe { r.body.as_ref() }.map_or(0, |b| b.len as usize)
}

fn without_nul(s: &'static str) -> &'static [u8] {
    s.trim_end_matches('\0').as_bytes()
}

pub fn is_cancelled(cancellable: *mut c_void) -> bool {
    !cancellable.is_null() && unsafe { g_cancellable_is_cancelled(cancellable) } != 0
}

fn monotonic_us() -> i64 {
    unsafe { g_get_monotonic_time() }
}

fn accept_language() -> Vec<u8> {
    text(unsafe { ns_net_effective_accept_language() })
        .unwrap_or_default()
        .to_vec()
}

fn user_agent(cfg: Option<&southstar_config::NsConfig>) -> Vec<u8> {
    let configured = cfg
        .and_then(|c| text(c.user_agent))
        .filter(|ua| !ua.is_empty());
    configured.map_or_else(
        || {
            without_nul(url::user_agent_for_mode(
                cfg.and_then(|c| text(c.compat_mode)),
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
        let v = cstr(v);
        unsafe { g_ascii_strtoll(v.as_ptr(), ptr::null_mut(), 10) as c_long }
    });
    requested.clamp(1, MAX_TIMEOUT_S as c_long)
}

fn host_is_dead(url: &[u8]) -> bool {
    url::origin_from_any(url).is_some_and(|o| !o.is_empty() && transport::host_recently_dead(&o))
}

fn synthesized(f: &Fetch, resp: &Response) -> bool {
    let url = cstr(f.url);
    let top = opt_cstr(f.top_url);
    let method = opt_cstr(f.method);
    let (body, body_len) = f
        .body
        .map_or((ptr::null(), 0), |b| (b.as_ptr().cast::<c_void>(), b.len()));
    let raw = resp.as_ptr();
    unsafe {
        super::ns_net_synthesize_about_response(
            url.as_ptr(),
            opt_ptr(&top),
            opt_ptr(&method),
            body,
            body_len,
            raw,
        ) != 0
            || super::ns_net_synthesize_view_source_response(
                url.as_ptr(),
                opt_ptr(&top),
                f.cancellable,
                raw,
            ) != 0
            || super::ns_net_synthesize_data_response(url.as_ptr(), raw) != 0
            || super::ns_net_synthesize_file_response(url.as_ptr(), opt_ptr(&top), raw) != 0
    }
}

fn upgraded(url: &[u8]) -> Vec<u8> {
    let mut current = url.to_vec();
    if let Some(ascii) = url::to_ascii(&current) {
        if ascii != current {
            current = ascii;
        }
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
        let site = request::fetch_site(f.top_url, url);
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
            let mobile = super::url::ns_net_is_mobile_mode() != 0;
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
        if out.tls_verify_failed != 0 || has_warning {
            SECURITY_INVALID
        } else if out.ok != 0 {
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
    let resp = Response::new();

    if !is_navigation && host_is_dead(f.url) {
        drop(resp);
        return Ok(Response::with_error(
            f.url,
            0,
            b"host unreachable (recent connection failure)",
        ));
    }
    if synthesized(f, &resp) {
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
        drop(resp);
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
    let request_origin = request::request_origin(&url, f.top_url, top_origin.as_deref(), f.method);
    let vary_origin = [b"Origin: ", request_origin].concat();
    let cache_headers: [&[u8]; 4] = [&vary_accept, &vary_language, &vary_agent, &vary_origin];

    let cookie_policy = cfg.map_or(COOKIE_FIRST_PARTY, |c| c.cookie_policy);
    let mut cookies_allowed = request_http && cookie_policy != COOKIE_NEVER;
    if cookies_allowed
        && cookie_policy == COOKIE_FIRST_PARTY
        && f.top_url.is_some()
        && !url::is_same_site(&url, effective_top)
    {
        cookies_allowed = false;
    }
    if partition.is_empty() {
        cookies_allowed = false;
    }
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
                    .is_some_and(|fu| url::same_origin(effective_top, fu));
            if has_cors {
                drop(resp);
                return Ok(Response::from_cache(entry));
            }
            cached = None;
        }
    }

    let referer_policy = cfg.map_or(REFERER_STRICT_ORIGIN_WHEN_CROSS, |c| c.referer_policy);
    let referer = url::referer_for(Some(&url), f.top_url, referer_policy);
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

    let max_redirs = request::clamp_redirects(cfg.map(|c| i64::from(c.max_redirects)));
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
    let header_list = HeaderList::new(&lines);

    let raw = resp.as_ptr();
    let mut wctx = NsWriteCtx::new(unsafe { (*raw).body });
    let mut hctx = unsafe {
        NsHeaderCtx::new([
            addr_of_mut!((*raw).content_type),
            addr_of_mut!((*raw).content_disposition),
            addr_of_mut!((*raw).csp_header),
            addr_of_mut!((*raw).xframe_options),
            addr_of_mut!((*raw).x_content_type_options),
            addr_of_mut!((*raw).cors_allow_origin),
            addr_of_mut!((*raw).refresh),
            addr_of_mut!((*raw).content_language),
        ])
    };

    let url_c = cstr(&url);
    let method_c = opt_cstr(f.method);
    let ua_c = cstr(&ua);
    let referer_c = opt_cstr(referer.as_deref());
    let jar_c = opt_cstr(cookie_jar.as_deref());
    let js_jar = cookie_jar
        .as_ref()
        .and_then(|_| storage::cookie_jar_path(Some(&partition), true));
    let js_jar_c = opt_cstr(js_jar.as_deref());
    let accept_encoding = ns_net_accept_encoding();
    let req = HopReq {
        url: url_c.as_ptr(),
        method: opt_ptr(&method_c),
        body: f.body.map_or(ptr::null(), |b| b.as_ptr().cast()),
        body_len: f.body.map_or(0, <[u8]>::len),
        headers: header_list.0,
        user_agent: ua_c.as_ptr(),
        referer: opt_ptr(&referer_c),
        referer_policy,
        accept_encoding: if accept_encoding.is_null() {
            c"".as_ptr()
        } else {
            accept_encoding
        },
        timeout_s: timeout,
        connect_timeout_s: if is_navigation {
            request::NAVIGATION_CONNECT_TIMEOUT_S
        } else {
            request::SUBRESOURCE_CONNECT_TIMEOUT_S
        } as c_long,
        proxy: unsafe { ns_net_pick_configured_proxy(url_c.as_ptr()) },
        no_proxy: ns_net_configured_no_proxy(),
        cookie_jar_path: opt_ptr(&jar_c),
        cookie_js_path: opt_ptr(&js_jar_c),
        follow_redirects: 0,
        max_redirs: max_redirs as c_long,
        is_navigation: glib::boolean(is_navigation),
        request_ftp: glib::boolean(request_ftp),
        initial_https: glib::boolean(url.starts_with(b"https://")),
        http_version_pref: ns_net_http_version(),
    };

    let mut out = HopOut::new();
    let start_us = monotonic_us();
    let start_real_ms = unsafe { g_get_real_time() } as f64 / 1000.0;
    let produced =
        unsafe { hop::ns_hop_transport(&req, &mut wctx, &mut hctx, &mut out, f.cancellable) } != 0;
    let mut captured = hctx.take();

    if !produced {
        let failure = if out.cancelled != 0 {
            Failure::Cancelled
        } else {
            Failure::Net(
                1,
                text(out.error_message)
                    .unwrap_or(b"transport init failed")
                    .to_vec(),
            )
        };
        unsafe { hop::ns_hop_out_clear(&mut out) };
        return Err(failure);
    }

    let transport_ok = out.ok != 0;
    let r = unsafe { &mut *raw };
    r.status = out.status;
    r.final_url = dup(Some(text(out.effective_url).unwrap_or(&url)));
    r.redirect_count = 0;
    r.request_start_us = start_us;
    r.request_start_real_ms = start_real_ms;
    r.domain_lookup_ms = out.t_namelookup_ms;
    r.connect_ms = out.t_connect_ms;
    r.tls_ms = out.t_appconnect_ms;
    r.pretransfer_ms = out.t_pretransfer_ms;
    r.response_start_ms = out.t_starttransfer_ms;
    r.response_end_ms = out.t_total_ms;
    if !out.remote_ip.is_null() {
        r.remote_ip = unsafe { glib::g_strdup(out.remote_ip) };
    }
    if out.http_version != 0 {
        r.next_hop_protocol =
            unsafe { glib::g_strdup(super::netlog::ns_net_http_version_name(out.http_version)) };
    }
    if !out.tls_warning.is_null() {
        r.tls_warning = unsafe { glib::g_strdup(out.tls_warning) };
    }
    let final_url = text(r.final_url).unwrap_or_default().to_vec();
    r.security = security_of(&final_url, &out, !r.tls_warning.is_null());
    if netlog::logging() {
        unsafe {
            super::netlog::ns_net_conn_stat_record(r.final_url, out.http_version, out.num_connects)
        };
    }
    if transport_ok && request_ftp {
        unsafe { super::ns_net_finish_ftp_response(raw) };
    }

    if let Some(reach) = url::origin_from_any(&url).filter(|o| !o.is_empty()) {
        if transport_ok || out.status > 0 {
            transport::mark_host_alive(&reach);
        } else if out.status == 0 && out.connect_failed != 0 {
            transport::mark_host_dead(&reach);
        }
    }

    let r = unsafe { &mut *raw };
    if !transport_ok {
        let message = if wctx.exceeded() {
            request::too_large_message(wctx.total())
        } else {
            text(out.error_message)
                .unwrap_or(b"transport error")
                .to_vec()
        };
        r.error = glib::strdup(&message);
    }
    unsafe { hop::ns_hop_out_clear(&mut out) };

    if transport_ok
        && request_http
        && request::is_simple_get(f.method)
        && !captured.set_cookie_seen
        && r.tls_warning.is_null()
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
                unsafe { g_byte_array_set_size(r.body, 0) };
                append(r.body, &entry.body);
                r.status = entry.status as c_long;
                replace(&mut r.content_type, entry.content_type.as_deref());
                replace(&mut r.cors_allow_origin, entry.cors_allow_origin.as_deref());
            }
            None if r.status > 0 && r.status < 300 && body_len(r) > 0 => {
                let body = super::body_bytes(r.body).unwrap_or_default();
                let stored = http_cache::Response {
                    final_url: text(r.final_url),
                    status: widen(r.status),
                    content_type: text(r.content_type),
                    cors_allow_origin: text(r.cors_allow_origin),
                    etag: captured.etag.as_deref(),
                    last_modified: captured.last_modified.as_deref(),
                    cache_control: captured.cache_control.as_deref(),
                    expires_header: captured.expires.as_deref(),
                    vary: captured.vary.as_deref(),
                    body,
                };
                http_cache::put(&url, Some(&cache_partition), &stored, &cache_headers);
            }
            None => {}
        }
    }

    *location = captured.location.take();
    if let Some(raw_headers) = captured.take_raw() {
        unsafe { glib::g_free(r.raw_headers.cast()) };
        r.raw_headers = raw_headers;
    }
    let end_us = monotonic_us();
    let body_len = body_len(r) as u64;
    super::netlog::ns_net_perf_record(start_us, end_us, body_len);
    let sent = unsafe { ns_net_slist_serialize(header_list.0) };
    let method_log = opt_cstr(f.method);
    unsafe {
        super::netlog::ns_net_log_record(
            opt_ptr(&method_log),
            url_c.as_ptr(),
            r.status,
            r.content_type,
            body_len,
            (end_us - start_us) as f64 / 1000.0,
            sent,
            r.raw_headers,
            r.error,
        );
        glib::g_free(sent.cast());
    }
    Ok(resp)
}

pub fn fetch(f: &Fetch) -> Result<Response, Failure> {
    let navigation = request::is_navigation(f.headers);
    if !navigation {
        let url = cstr(f.url);
        let top = opt_cstr(f.top_url);
        if unsafe { ns_ext_should_block(url.as_ptr(), opt_ptr(&top)) } != 0 {
            return Ok(Response::with_error(f.url, 0, b"blocked by extension"));
        }
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
        };
        let mut resp = fetch_hop(&hop, &mut location)?;
        let r = resp.get();
        let status = widen(r.status);
        let location = location.filter(|l| !l.is_empty());
        let Some(location) = location.filter(|_| request::is_redirect(status) && r.error.is_null())
        else {
            r.redirect_count = hops as c_int;
            return Ok(resp);
        };
        if hops >= max_redirs {
            replace(&mut r.error, Some(b"too many redirects"));
            r.redirect_count = hops as c_int;
            return Ok(resp);
        }
        let base = text(r.final_url).unwrap_or(&url).to_vec();
        let Some(next) = url::resolve(Some(&base), &location) else {
            r.redirect_count = hops as c_int;
            return Ok(resp);
        };
        if !request::redirect_allowed(&next, started_https, navigation) {
            replace(&mut r.error, Some(b"redirect to a disallowed URL blocked"));
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

unsafe fn header_lines(list: *const *const c_char) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    if list.is_null() {
        return lines;
    }
    let mut p = list;
    while let Some(line) = text(unsafe { *p }) {
        lines.push(line.to_vec());
        p = unsafe { p.add(1) };
    }
    lines
}

unsafe fn ptr_array_lines(array: *const GPtrArray) -> Vec<Vec<u8>> {
    let Some(array) = (unsafe { array.as_ref() }) else {
        return Vec::new();
    };
    (0..array.len as usize)
        .filter_map(|i| text(unsafe { *array.pdata.add(i) }.cast()))
        .map(<[u8]>::to_vec)
        .collect()
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_accept_headers_for(dest: c_int) -> *const *const c_char {
    let lines = match dest {
        FETCH_DEST_SCRIPT => &SCRIPT_ACCEPT,
        FETCH_DEST_STYLE => &STYLE_ACCEPT,
        FETCH_DEST_IMAGE => &IMAGE_ACCEPT,
        FETCH_DEST_FONT => &FONT_ACCEPT,
        _ => return ptr::null(),
    };
    lines.0.as_ptr()
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_net_request_key(
    url: *const c_char,
    top_url: *const c_char,
    method: *const c_char,
    extra_headers: *const *const c_char,
) -> *mut c_char {
    let Some(url) = text(url).filter(|u| request::is_coalescable(u, text(method))) else {
        return ptr::null_mut();
    };
    let partition = cache_partition(url, text(top_url));
    let lines = unsafe { header_lines(extra_headers) };
    let lines: Vec<&[u8]> = lines.iter().map(Vec::as_slice).collect();
    glib::strdup(&request::coalescing_key(url, &partition, &lines))
}

#[unsafe(no_mangle)]
#[allow(clippy::too_many_arguments)]
pub unsafe extern "C" fn ns_fetch_sync(
    url: *const c_char,
    top_url: *const c_char,
    method: *const c_char,
    body: *const c_void,
    body_len: usize,
    content_type: *const c_char,
    extra_headers: *const GPtrArray,
    cancellable: *mut c_void,
    error: *mut *mut GError,
) -> *mut NsResponse {
    let Some(url) = text(url) else {
        return ptr::null_mut();
    };
    let headers = unsafe { ptr_array_lines(extra_headers) };
    let f = Fetch {
        url,
        top_url: text(top_url),
        method: text(method),
        body: (!body.is_null()).then(|| unsafe { glib::slice(body.cast(), body_len) }),
        content_type: text(content_type),
        headers: &headers,
        cancellable,
    };
    match fetch(&f) {
        Ok(resp) => resp.into_raw(),
        Err(failure) => {
            failure.set(error);
            ptr::null_mut()
        }
    }
}
