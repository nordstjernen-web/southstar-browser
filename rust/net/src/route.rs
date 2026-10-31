//! Southstar — how a URL reaches the network through rust/http: the TLS settings, DNS over HTTPS and the configured proxy unless the no-proxy list exempts the host; a route never skips certificate verification.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use southstar_http::{Route, TlsSettings};

use crate::ffi::proxy::{configured_no_proxy, pick_configured_proxy};
use crate::ffi::transport::{ca_bundle_path, ec_curves};
use crate::url;

pub fn tls_settings() -> TlsSettings {
    TlsSettings {
        ca_bundle: ca_bundle_path(),
        curves: ec_curves().to_vec(),
    }
}

pub fn apply_dns_over_https() {
    let config = southstar_config::get();
    let url = config.as_deref().and_then(|c| c.doh_url.bytes());
    southstar_http::set_dns_over_https(url, tls_settings());
}

pub fn route(target: &[u8], host: &str) -> Route {
    apply_dns_over_https();
    let proxy = pick_configured_proxy(target)
        .filter(|p| !p.is_empty())
        .and_then(|p| southstar_http::parse_proxy(&p))
        .filter(|_| {
            !southstar_http::proxy_bypassed(
                configured_no_proxy().as_deref().unwrap_or_default(),
                host,
            )
        });
    Route {
        tls: tls_settings(),
        proxy,
        allow_insecure: false,
    }
}

pub fn user_agent() -> &'static [u8] {
    let ua = url::user_agent_for_mode(None).as_bytes();
    ua.strip_suffix(b"\0").unwrap_or(ua)
}
