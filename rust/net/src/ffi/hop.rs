//! Southstar — one HTTP, HTTPS or FTP hop over the in-tree Rust HTTP client: the request the fetch path builds, the result it reads back, the jar's cookies and HSTS recording, written into the body and header sinks, directly or through an HTTP or SOCKS proxy.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_long};
use std::ffi::CString;
use std::time::Duration;

use southstar_glib as glib;
use southstar_http::{Handler, Outcome, Request, TlsSettings, Version};
use southstar_netstore::cookies::Context;

use super::queue::Cancellable;
use super::sinks::{self, NsHeaderCtx, NsWriteCtx};
use super::transport::{ca_bundle_path, ns_net_aborting, ns_net_ec_curves};
use crate::response::cstr;
use crate::{cookies, hsts, url};

const HTTP_VERSION_1_1: c_long = 2;
const HTTP_VERSION_2_0: c_long = 3;

pub struct HopReq<'a> {
    pub url: &'a [u8],
    pub method: Option<&'a [u8]>,
    pub body: Option<&'a [u8]>,
    pub headers: &'a [Vec<u8>],
    pub user_agent: &'a [u8],
    pub referer: Option<&'a [u8]>,
    pub timeout_s: c_long,
    pub connect_timeout_s: c_long,
    pub proxy: Option<&'a [u8]>,
    pub no_proxy: Option<&'a [u8]>,
    pub cookie_jar_path: Option<&'a [u8]>,
    pub cookie_js_path: Option<&'a [u8]>,
    pub cookie_context: Context,
    pub request_ftp: bool,
    pub navigation: bool,
}

#[derive(Default)]
pub struct HopOut {
    pub status: c_long,
    pub effective_url: Option<CString>,
    pub remote_ip: Option<CString>,
    pub http_version: c_long,
    pub num_connects: c_long,
    pub t_namelookup_ms: f64,
    pub t_connect_ms: f64,
    pub t_appconnect_ms: f64,
    pub t_pretransfer_ms: f64,
    pub t_starttransfer_ms: f64,
    pub t_total_ms: f64,
    pub ok: bool,
    pub cancelled: bool,
    pub tls_verify_failed: bool,
    pub connect_failed: bool,
    pub tls_warning: Option<CString>,
    pub error_message: Option<CString>,
}

struct Sinks<'a> {
    url: &'a [u8],
    jar: Option<&'a [u8]>,
    context: Context,
    wctx: &'a mut NsWriteCtx,
    hctx: &'a mut NsHeaderCtx,
    cancellable: Option<&'a Cancellable>,
    sts: Option<Vec<u8>>,
}

impl Handler for Sinks<'_> {
    fn should_abort(&self) -> bool {
        ns_net_aborting() != 0 || self.cancellable.is_some_and(Cancellable::is_cancelled)
    }

    fn status_line(&mut self, line: &[u8]) {
        sinks::feed(self.hctx, line);
    }

    fn header(&mut self, line: &[u8], name: &[u8], value: &[u8]) {
        if name.eq_ignore_ascii_case(b"set-cookie")
            && let Some(jar) = self.jar
        {
            cookies::store_in(self.url, value, jar, self.context);
        }
        if name.eq_ignore_ascii_case(b"strict-transport-security") && self.sts.is_none() {
            self.sts = Some(value.to_vec());
        }
        sinks::feed(self.hctx, line);
    }

    fn body(&mut self, data: &[u8]) -> bool {
        sinks::write(self.wctx, data) == data.len()
    }

    fn tls_unverified(&mut self) {
        self.jar = None;
    }
}

fn seconds(value: c_long, fallback: u64) -> Duration {
    Duration::from_secs(if value > 0 { value as u64 } else { fallback })
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

pub fn transport(
    hop: &HopReq,
    wctx: &mut NsWriteCtx,
    hctx: &mut NsHeaderCtx,
    out: &mut HopOut,
    cancellable: Option<&Cancellable>,
) -> bool {
    let url_bytes = hop.url;
    crate::route::apply_dns_over_https();
    out.effective_url = Some(cstr(url_bytes));
    let proxy_spec = hop.proxy.filter(|p| !p.is_empty());
    let proxy = proxy_spec.and_then(southstar_http::parse_proxy);
    if proxy_spec.is_some() && proxy.is_none() {
        out.error_message = Some(cstr(b"unsupported proxy"));
        out.connect_failed = true;
        return true;
    }
    let no_proxy = hop.no_proxy.unwrap_or_default();
    let ftp = hop.request_ftp || url_bytes.starts_with(b"ftp://");
    if ftp {
        let host = url::host_from(url_bytes).unwrap_or_default();
        let host = String::from_utf8_lossy(&host).into_owned();
        let proxy = proxy.filter(|_| !southstar_http::proxy_bypassed(no_proxy, &host));
        let mut sinks = Sinks {
            url: url_bytes,
            jar: None,
            context: hop.cookie_context,
            wctx,
            hctx,
            cancellable,
            sts: None,
        };
        let outcome = southstar_http::ftp::get(
            url_bytes,
            proxy.as_ref(),
            seconds(hop.connect_timeout_s, 10),
            seconds(hop.timeout_s, 30),
            &mut sinks,
        );
        return write_outcome(out, &outcome, sinks.wctx, false);
    }
    let parts = url::parts(url_bytes).filter(|p| {
        !p.hostname.is_empty()
            && (p.protocol.eq_ignore_ascii_case(b"https:")
                || p.protocol.eq_ignore_ascii_case(b"http:"))
    });
    let Some(parts) = parts else {
        out.error_message = Some(cstr(b"invalid URL"));
        out.connect_failed = true;
        return true;
    };
    let https = parts.protocol.eq_ignore_ascii_case(b"https:");
    let port = match core::str::from_utf8(&parts.port)
        .ok()
        .and_then(|p| p.parse::<u32>().ok())
    {
        Some(p) if p > 0 && p < 65_536 => p as u16,
        _ if https => 443,
        _ => 80,
    };
    let host = String::from_utf8_lossy(&parts.hostname).into_owned();
    let proxy = proxy.filter(|_| !southstar_http::proxy_bypassed(no_proxy, &host));
    let authority = if parts.port.is_empty() {
        parts.hostname.clone()
    } else {
        [&parts.hostname[..], b":", &parts.port].concat()
    };
    let pathname: &[u8] = if parts.pathname.is_empty() {
        b"/"
    } else {
        &parts.pathname
    };
    let path = [pathname, &parts.search].concat();
    let jars = [
        hop.cookie_jar_path.map(<[u8]>::to_vec),
        hop.cookie_js_path.map(<[u8]>::to_vec),
    ];
    let cookie = jars[0]
        .as_ref()
        .and_then(|_| cookies::collect_in(url_bytes, &jars, hop.cookie_context));
    let insecure_opt_in =
        southstar_config::get().is_some_and(|c| c.tls_allow_insecure_override != 0);
    let pinned = url::host_from(url_bytes).is_some_and(|h| hsts::should_upgrade(&h));
    let request = Request {
        url: url_bytes,
        https,
        host: &host,
        port,
        authority: &authority,
        path: &path,
        method: hop.method.unwrap_or(b"GET"),
        user_agent: Some(hop.user_agent),
        referer: hop.referer,
        cookie,
        extra_headers: hop.headers.iter().map(Vec::as_slice).collect(),
        body: hop.body.unwrap_or_default(),
        timeout: seconds(hop.timeout_s, 30),
        connect_timeout: seconds(hop.connect_timeout_s, 10).min(seconds(hop.timeout_s, 30)),
        allow_insecure: insecure_opt_in && !pinned && hop.navigation,
        tls: TlsSettings {
            ca_bundle: ca_bundle_path(),
            curves: text(ns_net_ec_curves()).unwrap_or_default().to_vec(),
        },
        proxy,
    };
    let mut sinks = Sinks {
        url: url_bytes,
        jar: hop.cookie_jar_path,
        context: hop.cookie_context,
        wctx,
        hctx,
        cancellable,
        sts: None,
    };
    let outcome = southstar_http::perform(&request, &mut sinks);
    if let Some(sts) = sinks
        .sts
        .as_deref()
        .filter(|_| https && outcome.ok && outcome.tls_warning.is_none())
    {
        hsts::record(&parts.hostname, sts);
    }
    write_outcome(out, &outcome, sinks.wctx, true)
}

fn write_outcome(out: &mut HopOut, outcome: &Outcome, wctx: &mut NsWriteCtx, http: bool) -> bool {
    out.t_namelookup_ms = outcome.namelookup_ms;
    out.t_connect_ms = outcome.connect_ms;
    out.t_appconnect_ms = outcome.appconnect_ms;
    out.t_pretransfer_ms = outcome.pretransfer_ms;
    out.t_starttransfer_ms = outcome.starttransfer_ms;
    out.t_total_ms = outcome.total_ms;
    out.http_version = match outcome.version {
        _ if !http => 0,
        Version::Http2 => HTTP_VERSION_2_0,
        Version::Http11 => HTTP_VERSION_1_1,
    };
    out.num_connects = c_long::from(outcome.connects as u8);
    if let Some(ip) = &outcome.remote_ip {
        out.remote_ip = Some(cstr(ip.as_bytes()));
    }
    if let Some(warning) = &outcome.tls_warning {
        out.tls_warning = Some(cstr(warning.as_bytes()));
    }
    out.tls_verify_failed = outcome.tls_verify_failed;
    out.connect_failed = outcome.connect_failed;
    if outcome.cancelled {
        out.cancelled = true;
        return false;
    }
    if outcome.sink_full {
        wctx.mark_exceeded();
    }
    out.status = outcome.status as c_long;
    out.ok = outcome.ok;
    if let Some(error) = &outcome.error {
        out.error_message = Some(cstr(error.as_bytes()));
    }
    true
}

pub fn backend_shutdown() {
    southstar_http::shutdown();
}
