//! Southstar — the HTTP wire formats without I/O: HTTP/1.1 heads and chunked bodies, HTTP/2 frames, HPACK and the sans-I/O HTTP/2 client connection.
//! Copyright 2026 Andreas Røsdal
//! SPDX-License-Identifier: LicenseRef-NSL-1.0 OR GPL-3.0-or-later

#![forbid(unsafe_code)]

pub mod frame;
pub mod h1;
pub mod h2;
pub mod hpack;
mod hpack_tables;
