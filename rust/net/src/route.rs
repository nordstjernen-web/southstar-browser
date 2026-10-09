//! Southstar — how a URL outside the fetch path reaches the network through rust/http: the TLS settings, the configured proxy unless the no-proxy list exempts the host, and the insecure-certificate override.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

use core::ffi::c_char;

use southstar_glib as glib;
use southstar_http::{Route, TlsSettings};

use crate::ffi::proxy::{ns_net_configured_no_proxy, ns_net_pick_configured_proxy};
use crate::ffi::transport::{ns_net_ca_bundle_path, ns_net_ec_curves};
use crate::{hsts, url};

fn text<'a>(p: *const c_char) -> Option<&'a [u8]> {
    unsafe { glib::bytes(p) }
}

pub fn tls_settings() -> TlsSettings {
    TlsSettings {
        ca_bundle: text(ns_net_ca_bundle_path()).map(<[u8]>::to_vec),
        curves: text(ns_net_ec_curves()).unwrap_or_default().to_vec(),
    }
}

pub fn route(target: &[u8], host: &str) -> Route {
    let target_c = std::ffi::CString::new(target).unwrap_or_default();
    let proxy = text(unsafe { ns_net_pick_configured_proxy(target_c.as_ptr()) })
        .filter(|p| !p.is_empty())
        .and_then(southstar_http::parse_proxy)
        .filter(|_| {
            !southstar_http::proxy_bypassed(
                text(ns_net_configured_no_proxy()).unwrap_or_default(),
                host,
            )
        });
    let opt_in = southstar_config::get().is_some_and(|c| c.tls_allow_insecure_override != 0);
    let pinned = url::host_from(target).is_some_and(|h| hsts::should_upgrade(&h));
    Route {
        tls: tls_settings(),
        proxy,
        allow_insecure: opt_in && !pinned,
    }
}

pub fn user_agent() -> &'static [u8] {
    let ua = url::user_agent_for_mode(None).as_bytes();
    ua.strip_suffix(b"\0").unwrap_or(ua)
}
