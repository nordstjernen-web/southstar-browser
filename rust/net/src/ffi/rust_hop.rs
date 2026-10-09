//! Southstar — the hop transport over the in-tree Rust HTTP client: one HTTP or HTTPS request with the jar's cookies and HSTS recording, written into the body and header sinks, directly or through an HTTP or SOCKS proxy, and FTP downloads and listings.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::{c_char, c_long, c_void};
use std::time::Duration;

use southstar_glib::{self as glib, GBoolean};
use southstar_http::{Handler, Outcome, Request, TlsSettings, Version};

use super::curl;
use super::hop::{HopOut, HopReq, ns_hop_transport_curl};
use super::sinks::{self, NsHeaderCtx, NsWriteCtx};
use super::transport::{ns_net_aborting, ns_net_ca_bundle_path, ns_net_ec_curves};
use crate::{cookies, hsts, url};

const HTTP_VERSION_1_1: c_long = 2;
const HTTP_VERSION_2_0: c_long = 3;

unsafe extern "C" {
    fn g_cancellable_is_cancelled(cancellable: *mut c_void) -> GBoolean;
}

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

fn header_lines<'a>(mut list: *const curl::Slist) -> Vec<&'a [u8]> {
    let mut lines = Vec::new();
    while let Some(node) = unsafe { list.as_ref() } {
        if let Some(line) = text(node.data) {
            lines.push(line);
        }
        list = node.next;
    }
    lines
}

struct Sinks<'a> {
    url: &'a [u8],
    jar: Option<&'a [u8]>,
    wctx: &'a mut NsWriteCtx,
    hctx: &'a mut NsHeaderCtx,
    cancellable: *mut c_void,
    sts: Option<Vec<u8>>,
}

impl Handler for Sinks<'_> {
    fn should_abort(&self) -> bool {
        ns_net_aborting() != 0
            || (!self.cancellable.is_null()
                && unsafe { g_cancellable_is_cancelled(self.cancellable) } != 0)
    }

    fn status_line(&mut self, line: &[u8]) {
        sinks::feed(self.hctx, line);
    }

    fn header(&mut self, line: &[u8], name: &[u8], value: &[u8]) {
        if name.eq_ignore_ascii_case(b"set-cookie") {
            if let Some(jar) = self.jar {
                cookies::store_in(self.url, value, jar);
            }
        }
        if name.eq_ignore_ascii_case(b"strict-transport-security") && self.sts.is_none() {
            self.sts = Some(value.to_vec());
        }
        sinks::feed(self.hctx, line);
    }

    fn body(&mut self, data: &[u8]) -> bool {
        sinks::write(self.wctx, data) == data.len()
    }
}

fn seconds(value: c_long, fallback: u64) -> Duration {
    Duration::from_secs(if value > 0 { value as u64 } else { fallback })
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn ns_hop_transport(
    req: *const HopReq,
    wctx: *mut NsWriteCtx,
    hctx: *mut NsHeaderCtx,
    out: *mut HopOut,
    cancellable: *mut c_void,
) -> GBoolean {
    let (hop, out) = unsafe { (&*req, &mut *out) };
    let url_bytes = text(hop.url).unwrap_or_default();
    let proxy_spec = text(hop.proxy).filter(|p| !p.is_empty());
    let proxy = proxy_spec.and_then(southstar_http::parse_proxy);
    let ftp = hop.request_ftp != 0 || url_bytes.starts_with(b"ftp://");
    if (proxy_spec.is_some() && proxy.is_none()) || (!ftp && !url::is_http_or_https(url_bytes)) {
        return unsafe { ns_hop_transport_curl(req, wctx, hctx, out, cancellable) };
    }
    out.effective_url = unsafe { glib::g_strdup(hop.url) };
    if ftp {
        let host = url::host_from(url_bytes).unwrap_or_default();
        let host = String::from_utf8_lossy(&host).into_owned();
        let no_proxy = text(hop.no_proxy).unwrap_or_default();
        let proxy = proxy.filter(|_| !southstar_http::proxy_bypassed(no_proxy, &host));
        let mut sinks = Sinks {
            url: url_bytes,
            jar: None,
            wctx: unsafe { &mut *wctx },
            hctx: unsafe { &mut *hctx },
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
    let Some(parts) = url::parts(url_bytes).filter(|p| !p.hostname.is_empty()) else {
        out.error_message = glib::strdup(b"invalid URL");
        out.connect_failed = 1;
        return 1;
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
    let no_proxy = text(hop.no_proxy).unwrap_or_default();
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
        text(hop.cookie_jar_path).map(<[u8]>::to_vec),
        text(hop.cookie_js_path).map(<[u8]>::to_vec),
    ];
    let cookie = jars[0]
        .as_ref()
        .and_then(|_| cookies::collect_in(url_bytes, &jars));
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
        method: text(hop.method).unwrap_or(b"GET"),
        user_agent: text(hop.user_agent),
        referer: text(hop.referer),
        cookie,
        extra_headers: header_lines(hop.headers),
        body: if hop.body.is_null() {
            b""
        } else {
            unsafe { glib::slice(hop.body.cast(), hop.body_len) }
        },
        timeout: seconds(hop.timeout_s, 30),
        connect_timeout: seconds(hop.connect_timeout_s, 10).min(seconds(hop.timeout_s, 30)),
        allow_insecure: insecure_opt_in && !pinned,
        tls: TlsSettings {
            ca_bundle: text(ns_net_ca_bundle_path()).map(<[u8]>::to_vec),
            curves: text(ns_net_ec_curves()).unwrap_or_default().to_vec(),
        },
        proxy,
    };
    let mut sinks = Sinks {
        url: url_bytes,
        jar: text(hop.cookie_jar_path),
        wctx: unsafe { &mut *wctx },
        hctx: unsafe { &mut *hctx },
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

fn write_outcome(
    out: &mut HopOut,
    outcome: &Outcome,
    wctx: &mut NsWriteCtx,
    http: bool,
) -> GBoolean {
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
        out.remote_ip = glib::strdup(ip.as_bytes());
    }
    if let Some(warning) = &outcome.tls_warning {
        out.tls_warning = glib::strdup(warning.as_bytes());
    }
    out.tls_verify_failed = glib::boolean(outcome.tls_verify_failed);
    out.connect_failed = glib::boolean(outcome.connect_failed);
    if outcome.cancelled {
        out.cancelled = 1;
        return 0;
    }
    if outcome.sink_full {
        wctx.mark_exceeded();
    }
    out.status = outcome.status as c_long;
    out.ok = glib::boolean(outcome.ok);
    if let Some(error) = &outcome.error {
        out.error_message = glib::strdup(error.as_bytes());
    }
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn ns_net_backend_shutdown() {
    southstar_http::shutdown();
}
