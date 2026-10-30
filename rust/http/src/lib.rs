//! Southstar — the HTTP client: HTTP/1.1 and HTTP/2 over TCP and TLS, connection pooling, proxies, HPACK, content decoding and redirect following, and FTP.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod client;
mod decode;
mod doh;
mod fetch;
mod ffi;
pub mod ftp;
mod proxy;
#[cfg(test)]
mod tests;
mod tls;
mod transfer;
mod x25519_mlkem768;

use southstar_http_wire::{frame, h1, h2};

pub use client::{
    Outcome, Received, Request, Upgraded, Version, perform, preconnect, shutdown, upgrade,
};

pub fn description() -> &'static [u8] {
    match (cfg!(feature = "brotli"), cfg!(feature = "zstd")) {
        (true, true) => b"southstar-http (HTTP/1.1, HTTP/2; gzip, deflate, br, zstd)",
        (true, false) => b"southstar-http (HTTP/1.1, HTTP/2; gzip, deflate, br)",
        (false, true) => b"southstar-http (HTTP/1.1, HTTP/2; gzip, deflate, zstd)",
        (false, false) => b"southstar-http (HTTP/1.1, HTTP/2; gzip, deflate)",
    }
}

pub fn tls_description() -> &'static [u8] {
    b"rustls with ring (TLS 1.3 and 1.2; X25519MLKEM768, X25519, P-256, P-384)"
}

pub fn set_dns_over_https(url: Option<&[u8]>, tls: TlsSettings) {
    doh::configure(url, tls);
}

pub fn init() {
    ffi::socket::init();
}
pub use decode::accept_encoding;
pub use fetch::{Fetch, Route, Target, fetch, parse_target, resolve};
pub use ffi::codec::{Inflate, Inflated};
pub use proxy::{
    EnvLookup, Proxy, bypassed as proxy_bypassed, env_valid as env_proxies_valid,
    from_env as env_proxy, parse as parse_proxy,
};
pub use tls::{Settings as TlsSettings, preload_trust_roots};
pub use transfer::Handler;
