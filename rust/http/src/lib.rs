//! Southstar — the HTTP client: HTTP/1.1 and HTTP/2 over TCP and TLS, connection pooling, proxies, HPACK, content decoding and redirect following, and FTP.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod client;
mod decode;
mod fetch;
mod ffi;
mod frame;
pub mod ftp;
mod h1;
mod h2;
mod hpack;
mod hpack_tables;
mod proxy;
mod transfer;

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

pub fn init() {
    ffi::tls::init();
    ffi::socket::init();
}
pub use decode::accept_encoding;
pub use fetch::{Fetch, Route, Target, fetch, parse_target, resolve};
pub use ffi::tls::Settings as TlsSettings;
pub use proxy::{Proxy, bypassed as proxy_bypassed, parse as parse_proxy};
pub use transfer::Handler;
