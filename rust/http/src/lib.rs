//! Southstar — the HTTP client: HTTP/1.1 and HTTP/2 over TCP and TLS, connection pooling, proxies, HPACK, content decoding and redirect following.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

mod client;
mod decode;
mod ffi;
mod fetch;
mod frame;
mod h1;
mod h2;
mod hpack;
mod hpack_tables;
mod proxy;
mod transfer;

pub use client::{Outcome, Request, Version, perform, shutdown};
pub use decode::accept_encoding;
pub use fetch::{Fetch, Route, Target, fetch, parse_target, resolve};
pub use ffi::tls::Settings as TlsSettings;
pub use proxy::{Proxy, bypassed as proxy_bypassed, parse as parse_proxy};
pub use transfer::Handler;
